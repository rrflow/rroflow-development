//! SMTP is an adapter, not an account authority. Failed delivery remains queued.
use super::*;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use std::{future::Future, io::Read, time::Duration};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub from: String,
    /// Mounted private JSON {"username":"...","password":"..."}. Never logged.
    pub credentials_file: PathBuf,
}
impl SmtpConfig {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.host.is_empty()
                && self.host.len() <= 253
                && self
                    .host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
                && self.port > 0,
            "invalid SMTP host or port"
        );
        self.from
            .parse::<Mailbox>()
            .map_err(|_| anyhow::anyhow!("invalid sender address"))?;
        Ok(())
    }
    fn transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        self.validate()?;
        let file = std::fs::File::open(&self.credentials_file)
            .map_err(|_| anyhow::anyhow!("cannot open SMTP credentials file"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                file.metadata()?.permissions().mode() & 0o077 == 0,
                "SMTP credentials file must be private"
            );
        }
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(16385).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16384, "SMTP credentials file exceeds limit");
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Login {
            username: String,
            password: String,
        }
        let login: Login = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid SMTP credentials file"))?;
        ensure!(
            !login.username.is_empty() && !login.password.is_empty(),
            "empty SMTP credentials"
        );
        // STARTTLS is mandatory and verifies the host's certificate. No downgrade switch.
        Ok(
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)?
                .port(self.port)
                .credentials(Credentials::new(login.username, login.password))
                .timeout(Some(Duration::from_secs(15)))
                .build(),
        )
    }
}

#[derive(Serialize)]
pub struct MailResult {
    pub sent: usize,
    pub retried: usize,
}

impl Accounts {
    async fn deliver_one<F, Fut>(&self, send: F) -> Result<MailResult>
    where
        F: FnOnce(String, String, String) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let mut tx = self.pool.begin().await?;
        // Erase expired bearer envelopes even when nobody completes a request.
        sqlx::query(
            "DELETE FROM native_actions WHERE context_sha256=$1 AND expires_at<=clock_timestamp()",
        )
        .bind(&self.context)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query("SELECT a.token_sha256,a.email,a.purpose,a.expires_at,o.key_id,o.nonce,o.ciphertext FROM native_mail_outbox o JOIN native_actions a USING(token_sha256) WHERE a.context_sha256=$1 AND a.expires_at>clock_timestamp() AND o.sent_at IS NULL AND o.next_attempt_at<=clock_timestamp() ORDER BY o.next_attempt_at,a.token_sha256 LIMIT 1 FOR UPDATE OF o SKIP LOCKED")
            .bind(&self.context).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(MailResult {
                sent: 0,
                retried: 0,
            });
        };
        let digest: String = row.try_get("token_sha256")?;
        let email: String = row.try_get("email")?;
        let purpose: String = row.try_get("purpose")?;
        let expiry: DateTime<Utc> = row.try_get("expires_at")?;
        let key: String = row.try_get("key_id")?;
        let nonce: Vec<u8> = row.try_get("nonce")?;
        let ciphertext: Vec<u8> = row.try_get("ciphertext")?;
        let aad = self.mail_aad(&digest, &email, &purpose, expiry.timestamp(), &key);
        let token = self
            .cipher
            .open(&key, &nonce, &ciphertext, &aad)
            .and_then(|p| String::from_utf8(p).ok());
        let result = match token {
            Some(token) if valid_secret(&token, ACTION_PREFIX) && hash_key(&token) == digest => {
                // Fragment keeps the credential out of HTTP access logs and Referer headers.
                let link = format!("{}/app/account#{}", self.cfg.origin, token);
                let subject = if purpose == "register" {
                    "Finish creating your RROFLOW account"
                } else {
                    "Reset your RROFLOW password"
                };
                send(email,subject.into(),format!("Open this link to choose your password:\n\n{link}\n\nThis link expires at {} and can be used once. If you did not request it, you can ignore this email.\n",expiry.to_rfc3339())).await
            }
            _ => Err(anyhow::anyhow!("mail envelope unavailable")),
        };
        let sent = result.is_ok();
        if sent {
            sqlx::query("UPDATE native_mail_outbox SET sent_at=clock_timestamp(),attempts=attempts+1 WHERE token_sha256=$1")
                .bind(&digest).execute(&mut *tx).await?;
        } else {
            sqlx::query("UPDATE native_mail_outbox SET attempts=attempts+1,next_attempt_at=clock_timestamp()+make_interval(secs=>least(300,30*(attempts+1))) WHERE token_sha256=$1")
                .bind(&digest).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        // SMTP errors may include addresses. Only counts leave this boundary.
        Ok(MailResult {
            sent: usize::from(sent),
            retried: usize::from(!sent),
        })
    }
}

/// One delivery attempt, for the supervised worker. A crash after SMTP accepts may
/// redeliver the same one-use link; it cannot create another account or reset twice.
pub struct MailWorker {
    accounts: Accounts,
    from: Mailbox,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}
impl MailWorker {
    pub fn new(pg: &crate::PgStore, cfg: &AccountConfig) -> Result<Self> {
        let accounts = Accounts::new(cfg, pg.pool.clone())?;
        let smtp = cfg
            .smtp
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SMTP is not configured"))?;
        Ok(Self {
            accounts,
            from: smtp.from.parse()?,
            transport: smtp.transport()?,
        })
    }
    pub async fn run_once(&self) -> Result<MailResult> {
        self.accounts
            .deliver_one(|to, subject, body| async move {
                let message = Message::builder()
                    .from(self.from.clone())
                    .to(to.parse()?)
                    .subject(subject)
                    .body(body)?;
                tokio::time::timeout(Duration::from_secs(20), self.transport.send(message))
                    .await??;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
pub(super) async fn deliver_fixture<F, Fut>(accounts: &Accounts, send: F) -> Result<MailResult>
where
    F: FnOnce(String, String, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    accounts.deliver_one(send).await
}
