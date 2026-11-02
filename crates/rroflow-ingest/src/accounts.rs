//! Native identities are separate from federation. Matching emails never link accounts.
use crate::{hash_key, nomen::Subject, secret_box::SecretBox};
use anyhow::{Result, ensure};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

pub(crate) mod http;
mod mail;
pub use mail::{MailWorker, SmtpConfig};
pub(crate) const ISSUER: &str = "rroflow:native:v1";
pub(crate) const SESSION_PREFIX: &str = "rrna_";
const ACTION_PREFIX: &str = "rrnv_";
const PASSWORD_PROFILE: &str = "$argon2id$v=19$m=19456,t=2,p=1$";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountConfig {
    pub origin: String,
    pub encryption_keys_file: PathBuf,
    /// Deliberately off until delivery and public-signup review are qualified.
    pub signup_enabled: bool,
    pub action_ttl_seconds: i64,
    pub session_ttl_seconds: i64,
    pub password_concurrency: usize,
    pub sessions_per_user: i64,
    pub max_pending_emails: i64,
    pub mail_poll_seconds: u64,
    /// Shared across instances, per ten-minute window and operation.
    pub global_attempts: i32,
    pub email_attempts: i32,
    pub smtp: Option<SmtpConfig>,
}
impl Default for AccountConfig {
    fn default() -> Self {
        Self {
            origin: String::new(),
            encryption_keys_file: PathBuf::new(),
            signup_enabled: false,
            action_ttl_seconds: 1800,
            session_ttl_seconds: 86400,
            password_concurrency: 4,
            sessions_per_user: 10,
            max_pending_emails: 5000,
            mail_poll_seconds: 5,
            global_attempts: 300,
            email_attempts: 5,
            smtp: None,
        }
    }
}

pub(crate) struct Accounts {
    pub(crate) cfg: AccountConfig,
    pool: PgPool,
    cipher: SecretBox,
    context: String,
    hashing: Arc<Semaphore>,
    dummy: String,
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Purpose {
    Register,
    Reset,
}
impl Purpose {
    fn name(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Reset => "reset",
        }
    }
}
fn secret(prefix: &str) -> String {
    format!("{prefix}{}", hex::encode(rand::random::<[u8; 32]>()))
}
fn valid_secret(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
}
pub(crate) fn normalize_email(email: &str) -> Option<String> {
    // Explicit ASCII/case-insensitive policy. Never strip plus tags or provider dots.
    let email = email.trim();
    if email.len() > 254
        || !email.is_ascii()
        || email
            .bytes()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return None;
    }
    let address = email.parse::<lettre::Address>().ok()?;
    if !address.domain().contains('.') {
        return None;
    }
    Some(email.to_ascii_lowercase())
}
pub(crate) fn valid_password(password: &str) -> bool {
    (15..=128).contains(&password.chars().count()) && password.len() <= 512
}

async fn audit(tx: &mut Transaction<'_, Postgres>, user: &str, action: &str) -> Result<()> {
    sqlx::query("INSERT INTO audit_events(organization_id,actor_user_id,action,target) SELECT id,$1,$2,$1 FROM organizations WHERE personal AND created_by=$1")
        .bind(user).bind(action).execute(&mut **tx).await?;
    Ok(())
}

impl Accounts {
    pub(crate) fn new(cfg: &AccountConfig, pool: PgPool) -> Result<Self> {
        ensure!(
            crate::web_auth::https_origin(&cfg.origin),
            "native accounts need a canonical HTTPS origin"
        );
        ensure!(
            (300..=86400).contains(&cfg.action_ttl_seconds)
                && (300..=2592000).contains(&cfg.session_ttl_seconds)
                && (1..=16).contains(&cfg.password_concurrency)
                && (1..=100).contains(&cfg.sessions_per_user)
                && (1..=100000).contains(&cfg.max_pending_emails)
                && (1..=3600).contains(&cfg.mail_poll_seconds)
                && (1..=100000).contains(&cfg.global_attempts)
                && (1..=100).contains(&cfg.email_attempts),
            "native account limits are out of range"
        );
        if let Some(smtp) = &cfg.smtp {
            smtp.validate()?;
        }
        ensure!(
            !cfg.signup_enabled || cfg.smtp.is_some(),
            "signup requires configured email delivery"
        );
        let mut cfg = cfg.clone();
        cfg.origin = cfg.origin.trim_end_matches('/').to_string();
        let dummy = Argon2::default()
            .hash_password(secret("dummy").as_bytes())
            .map_err(|_| anyhow::anyhow!("password initialization failed"))?
            .to_string();
        ensure!(
            dummy.starts_with(PASSWORD_PROFILE),
            "unexpected Argon2 profile"
        );
        Ok(Self {
            cipher: SecretBox::load(&cfg.encryption_keys_file)?,
            context: hash_key(&format!("{ISSUER}\n{}", cfg.origin)),
            hashing: Arc::new(Semaphore::new(cfg.password_concurrency)),
            pool,
            cfg,
            dummy,
        })
    }

    /// Fast refusal when saturated; unbounded waiting work must not allocate password memory.
    async fn password_work(
        &self,
        password: String,
        stored: Option<String>,
    ) -> Result<Option<String>> {
        let permit = self
            .hashing
            .clone()
            .try_acquire_owned()
            .map_err(|_| anyhow::anyhow!("password work is busy"))?;
        let password = Zeroizing::new(password);
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if let Some(stored) = stored {
                // Bound both algorithm and memory/CPU parameters BEFORE calling a PHC verifier.
                ensure!(
                    stored.len() <= 512 && stored.starts_with(PASSWORD_PROFILE),
                    "unsupported password profile"
                );
                let parsed = PasswordHash::new(&stored)
                    .map_err(|_| anyhow::anyhow!("invalid password profile"))?;
                Ok(Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .ok()
                    .map(|_| stored))
            } else {
                Ok(Some(
                    Argon2::default()
                        .hash_password(password.as_bytes())
                        .map_err(|_| anyhow::anyhow!("password hashing failed"))?
                        .to_string(),
                ))
            }
        })
        .await?
    }

    /// Store-level abuse bounds are shared across processes; denied requests don't grow the table.
    pub(crate) async fn allow(&self, operation: &str, subject: &str) -> Result<bool> {
        let scope = format!("{}/{operation}", self.context);
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(&scope)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM native_rate_windows WHERE scope=$1 AND expires_at<=clock_timestamp()",
        )
        .bind(&scope)
        .execute(&mut *tx)
        .await?;
        let mut allowed = true;
        for (subject, limit) in [
            ("global".to_string(), self.cfg.global_attempts),
            (format!("email:{subject}"), self.cfg.email_attempts),
        ] {
            let digest = hash_key(&subject);
            let n: Option<i32> = sqlx::query_scalar(
                "SELECT attempts FROM native_rate_windows WHERE scope=$1 AND subject_sha256=$2",
            )
            .bind(&scope)
            .bind(&digest)
            .fetch_optional(&mut *tx)
            .await?;
            if n.unwrap_or(0) >= limit {
                allowed = false;
                break;
            }
            sqlx::query("INSERT INTO native_rate_windows(scope,subject_sha256,expires_at,attempts) VALUES($1,$2,clock_timestamp()+interval '10 minutes',1) ON CONFLICT(scope,subject_sha256) DO UPDATE SET attempts=native_rate_windows.attempts+1")
                .bind(&scope).bind(&digest).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(allowed)
    }

    async fn email_lock(&self, tx: &mut Transaction<'_, Postgres>, email: &str) -> Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("rroflow.native-email/{}", hash_key(email)))
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
    fn mail_aad(
        &self,
        digest: &str,
        email: &str,
        purpose: &str,
        expires: i64,
        key: &str,
    ) -> Vec<u8> {
        serde_json::to_vec(&(
            "rroflow.native-mail/v1",
            &self.context,
            digest,
            email,
            purpose,
            expires,
            key,
        ))
        .expect("fixed tuple serialization")
    }
    /// Returns no token to the requester. Ownership proof goes only to the mailbox.
    pub(crate) async fn request(&self, email: &str, purpose: Purpose) -> Result<()> {
        ensure!(
            normalize_email(email).as_deref() == Some(email),
            "invalid normalized address"
        );
        ensure!(
            purpose != Purpose::Register || self.cfg.signup_enabled,
            "registration disabled"
        );
        ensure!(self.cfg.smtp.is_some(), "email delivery is not configured");
        // Sweep in its own committed statement. Holding expired action rows while
        // waiting for the email lock would deadlock with completion deleting them.
        sqlx::query(
            "DELETE FROM native_actions WHERE context_sha256=$1 AND expires_at<=clock_timestamp()",
        )
        .bind(&self.context)
        .execute(&self.pool)
        .await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("rroflow.native-mail-capacity/{}", self.context))
            .execute(&mut *tx)
            .await?;
        self.email_lock(&mut tx, email).await?;
        let epoch: Option<i64> =
            sqlx::query_scalar("SELECT credential_epoch FROM native_credentials WHERE email=$1")
                .bind(email)
                .fetch_optional(&mut *tx)
                .await?;
        if (purpose == Purpose::Register && epoch.is_some())
            || (purpose == Purpose::Reset && epoch.is_none())
        {
            return Ok(());
        }
        let queued: i64 =
            sqlx::query_scalar("SELECT count(*) FROM native_actions WHERE context_sha256=$1")
                .bind(&self.context)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            queued < self.cfg.max_pending_emails,
            "mail capacity reached"
        );
        let token = Zeroizing::new(secret(ACTION_PREFIX));
        let digest = hash_key(&token);
        let expiry: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()))::bigint + $1")
                .bind(self.cfg.action_ttl_seconds)
                .fetch_one(&mut *tx)
                .await?;
        let key = self.cipher.active();
        let aad = self.mail_aad(&digest, email, purpose.name(), expiry, key);
        let (nonce, ciphertext) = self.cipher.seal(token.as_bytes(), &aad)?;
        sqlx::query("INSERT INTO native_actions(token_sha256,context_sha256,email,purpose,credential_epoch,expires_at) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&digest).bind(&self.context).bind(email).bind(purpose.name()).bind(epoch)
            .bind(DateTime::<Utc>::from_timestamp(expiry,0)).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO native_mail_outbox(token_sha256,key_id,nonce,ciphertext) VALUES($1,$2,$3,$4)")
            .bind(&digest).bind(key).bind(nonce).bind(ciphertext).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn complete(&self, token: &str, password: String) -> Result<bool> {
        if !valid_secret(token, ACTION_PREFIX) || !valid_password(&password) {
            return Ok(false);
        }
        let digest = hash_key(token);
        let email: Option<String> = sqlx::query_scalar("SELECT email FROM native_actions WHERE token_sha256=$1 AND context_sha256=$2 AND expires_at>clock_timestamp()")
            .bind(&digest).bind(&self.context).fetch_optional(&self.pool).await?;
        let Some(email) = email else {
            return Ok(false);
        };
        let phc = self
            .password_work(password, None)
            .await?
            .expect("hash operation");
        let mut tx = self.pool.begin().await?;
        self.email_lock(&mut tx, &email).await?;
        // Recheck after hashing and after the lock: expiry/reuse/reset may have changed.
        let action: Option<(String,Option<i64>)> = sqlx::query_as("SELECT purpose,credential_epoch FROM native_actions WHERE token_sha256=$1 AND context_sha256=$2 AND expires_at>clock_timestamp() FOR UPDATE")
            .bind(&digest).bind(&self.context).fetch_optional(&mut *tx).await?;
        let Some((purpose, epoch)) = action else {
            return Ok(false);
        };
        if purpose == "register" {
            if !self.cfg.signup_enabled {
                return Ok(false);
            }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM native_credentials WHERE email=$1)",
            )
            .bind(&email)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                return Ok(false);
            }
            let subject = Subject {
                id: secret("native_"),
                org: None,
                name: None,
                via: "native",
            };
            let membership =
                crate::tenancy::create_account_project(&mut tx, ISSUER, &subject).await?;
            sqlx::query(
                "INSERT INTO native_credentials(user_id,email,password_phc) VALUES($1,$2,$3)",
            )
            .bind(&membership.user)
            .bind(&email)
            .bind(phc)
            .execute(&mut *tx)
            .await?;
            audit(&mut tx, &membership.user, "account.verified").await?;
        } else {
            let user: Option<String> = sqlx::query_scalar("UPDATE native_credentials SET password_phc=$2,credential_epoch=credential_epoch+1,changed_at=clock_timestamp() WHERE email=$1 AND credential_epoch=$3 RETURNING user_id")
                .bind(&email).bind(phc).bind(epoch).fetch_optional(&mut *tx).await?;
            let Some(user) = user else {
                return Ok(false);
            };
            audit(&mut tx, &user, "account.password_reset").await?;
            sqlx::query("DELETE FROM native_sessions WHERE user_id=$1")
                .bind(user)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM native_actions WHERE email=$1")
            .bind(&email)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        // No automatic sign-in after recovery or signup; the person explicitly signs in.
        Ok(true)
    }

    pub(crate) async fn login(&self, email: &str, password: String) -> Result<Option<String>> {
        let row: Option<(String, String, i64)> = sqlx::query_as(
            "SELECT user_id,password_phc,credential_epoch FROM native_credentials WHERE email=$1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        let stored = row
            .as_ref()
            .map(|r| r.1.clone())
            .unwrap_or_else(|| self.dummy.clone());
        let verified = self
            .password_work(password, Some(stored.clone()))
            .await?
            .is_some();
        let Some((user, _, epoch)) = row.filter(|_| verified) else {
            return Ok(None);
        };
        let mut tx = self.pool.begin().await?;
        // Serializes session capacity and reset. A password verified before a reset cannot sign in after it.
        let current: Option<(String,i64)> = sqlx::query_as("SELECT password_phc,credential_epoch FROM native_credentials WHERE user_id=$1 FOR UPDATE")
            .bind(&user).fetch_optional(&mut *tx).await?;
        if current != Some((stored, epoch)) {
            return Ok(None);
        }
        sqlx::query(
            "DELETE FROM native_sessions WHERE user_id=$1 AND expires_at<=clock_timestamp()",
        )
        .bind(&user)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM native_sessions WHERE session_sha256 IN (SELECT session_sha256 FROM native_sessions WHERE user_id=$1 ORDER BY created_at DESC,session_sha256 OFFSET $2)")
            .bind(&user).bind(self.cfg.sessions_per_user-1).execute(&mut *tx).await?;
        audit(&mut tx, &user, "account.signed_in").await?;
        let token = secret(SESSION_PREFIX);
        sqlx::query("INSERT INTO native_sessions(session_sha256,context_sha256,user_id,credential_epoch,expires_at) VALUES($1,$2,$3,$4,clock_timestamp()+make_interval(secs=>$5))")
            .bind(hash_key(&token)).bind(&self.context).bind(user).bind(epoch).bind(self.cfg.session_ttl_seconds as f64)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(token))
    }

    pub(crate) async fn subject(&self, token: &str) -> Result<Option<Subject>> {
        if !valid_secret(token, SESSION_PREFIX) {
            return Ok(None);
        }
        let row: Option<(String,Option<String>)> = sqlx::query_as("SELECT i.subject,u.display_name FROM native_sessions s JOIN native_credentials c ON c.user_id=s.user_id AND c.credential_epoch=s.credential_epoch JOIN user_identities i ON i.user_id=s.user_id AND i.issuer=$3 JOIN users u ON u.id=s.user_id WHERE s.session_sha256=$1 AND s.context_sha256=$2 AND s.expires_at>clock_timestamp()")
            .bind(hash_key(token)).bind(&self.context).bind(ISSUER).fetch_optional(&self.pool).await?;
        Ok(row.map(|(id, name)| Subject {
            id,
            name,
            org: None,
            via: "native",
        }))
    }
    pub(crate) async fn logout(&self, token: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let user: Option<String> = sqlx::query_scalar("DELETE FROM native_sessions WHERE session_sha256=$1 AND context_sha256=$2 RETURNING user_id")
            .bind(hash_key(token))
            .bind(&self.context)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(user) = user {
            audit(&mut tx, &user, "account.signed_out").await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
