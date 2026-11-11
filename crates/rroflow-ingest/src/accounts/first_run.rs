//! The built-in admin account. Its first password is assumed exposed, because an installing
//! AI may paste it into chat. So it works once, expires, and is stored only as a digest.
//! Claiming it sets the person's own password, or generates one returned only to that browser.
use super::*;

pub(crate) const ADMIN_LOGIN: &str = "admin";
pub const SETUP_TTL_MINUTES: i64 = 60;
const SETUP_PREFIX: &str = "rrsetup-";
// Readable off a screen: no 0/O, 1/I/L or U. Six groups of five is about 147 bits.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const GROUPS: usize = 6;
const GROUP_LENGTH: usize = 5;

fn generated() -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(GROUPS * (GROUP_LENGTH + 1)));
    for group in 0..GROUPS {
        if group > 0 {
            out.push('-');
        }
        for _ in 0..GROUP_LENGTH {
            out.push(ALPHABET[rand::random_range(0..ALPHABET.len())] as char);
        }
    }
    out
}

pub(crate) fn valid_setup(value: &str) -> bool {
    value.strip_prefix(SETUP_PREFIX).is_some_and(|groups| {
        let parts: Vec<_> = groups.split('-').collect();
        parts.len() == GROUPS
            && parts
                .iter()
                .all(|p| p.len() == GROUP_LENGTH && p.bytes().all(|c| ALPHABET.contains(&c)))
    })
}

/// The password exists only here and in the operator's terminal; storage has its digest.
pub struct SetupPassword {
    pub password: Zeroizing<String>,
    pub origin: String,
}

/// Operator command. Replaces any setup password that has not been used yet.
pub async fn issue_setup_password(
    pg: &crate::PgStore,
    cfg: &AccountConfig,
    reset: bool,
) -> Result<SetupPassword> {
    let accounts = Accounts::new(cfg, pg.pool.clone())?;
    Ok(SetupPassword {
        password: accounts.issue_setup(reset).await?,
        origin: accounts.cfg.origin.clone(),
    })
}

pub(crate) struct Claimed {
    pub(crate) session: String,
    /// The generated password. Absent when the person chose their own.
    pub(crate) password: Option<Zeroizing<String>>,
    pub(crate) revision: i64,
    pub(crate) active_keys: i64,
}

pub(crate) struct PasswordStatus {
    pub(crate) signin: String,
    pub(crate) unsaved: bool,
    pub(crate) revision: i64,
}

pub(crate) enum Saved {
    Done,
    /// Another tab replaced the password this page is confirming.
    Stale,
    SignedOut,
}

impl Accounts {
    pub(crate) async fn issue_setup(&self, reset: bool) -> Result<Zeroizing<String>> {
        let mut tx = self.pool.begin().await?;
        self.email_lock(&mut tx, ADMIN_LOGIN).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_credentials WHERE email=$1)")
                .bind(ADMIN_LOGIN)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            reset || !exists,
            "setup is already finished; use --reset to replace the admin password"
        );
        let mut password = Zeroizing::new(SETUP_PREFIX.to_string());
        password.push_str(&generated());
        sqlx::query("INSERT INTO instance_setup(context_sha256,token_sha256,expires_at) VALUES($1,$2,clock_timestamp()+make_interval(mins=>$3)) ON CONFLICT(singleton) DO UPDATE SET context_sha256=EXCLUDED.context_sha256,token_sha256=EXCLUDED.token_sha256,expires_at=EXCLUDED.expires_at,created_at=clock_timestamp()")
            .bind(&self.context).bind(hash_key(&password)).bind(SETUP_TTL_MINUTES as i32)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(password)
    }

    /// (a setup password is waiting to be used, the built-in admin exists)
    pub(crate) async fn first_run_state(&self) -> Result<(bool, bool)> {
        Ok(sqlx::query_as("SELECT EXISTS(SELECT 1 FROM instance_setup WHERE context_sha256=$1 AND expires_at>clock_timestamp()),EXISTS(SELECT 1 FROM native_credentials WHERE email=$2)")
            .bind(&self.context).bind(ADMIN_LOGIN).fetch_one(&self.pool).await?)
    }

    pub(crate) async fn claim_setup(
        &self,
        setup: &str,
        chosen: Option<String>,
    ) -> Result<Option<Claimed>> {
        if !valid_setup(setup) {
            return Ok(None);
        }
        let digest = hash_key(setup);
        // Only a live setup password earns password-hashing work.
        let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_setup WHERE token_sha256=$1 AND context_sha256=$2 AND expires_at>clock_timestamp())")
            .bind(&digest).bind(&self.context).fetch_one(&self.pool).await?;
        if !live {
            return Ok(None);
        }
        let generated_password = chosen.is_none().then(generated);
        let secret_text = match (&chosen, &generated_password) {
            (Some(chosen), _) => chosen.clone(),
            (None, Some(made)) => made.to_string(),
            (None, None) => unreachable!("generated when none was chosen"),
        };
        let phc = self
            .password_work(secret_text, None)
            .await?
            .expect("hash operation");
        let mut tx = self.pool.begin().await?;
        self.email_lock(&mut tx, ADMIN_LOGIN).await?;
        // The single-use decision. A concurrent claim finds the row already gone.
        let claimed = sqlx::query("DELETE FROM instance_setup WHERE token_sha256=$1 AND context_sha256=$2 AND expires_at>clock_timestamp()")
            .bind(&digest).bind(&self.context).execute(&mut *tx).await?.rows_affected() == 1;
        if !claimed {
            return Ok(None);
        }
        let existing: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM native_credentials WHERE email=$1")
                .bind(ADMIN_LOGIN)
                .fetch_optional(&mut *tx)
                .await?;
        let user = match existing {
            Some(user) => {
                sqlx::query("UPDATE native_credentials SET password_phc=$2,credential_epoch=credential_epoch+1,changed_at=clock_timestamp() WHERE user_id=$1")
                    .bind(&user).bind(phc).execute(&mut *tx).await?;
                sqlx::query("DELETE FROM native_sessions WHERE user_id=$1")
                    .bind(&user)
                    .execute(&mut *tx)
                    .await?;
                audit(&mut tx, &user, "instance.admin_password_replaced").await?;
                user
            }
            None => {
                let subject = Subject {
                    id: secret("native_"),
                    org: None,
                    name: Some("Admin".into()),
                    via: "native",
                };
                let membership =
                    crate::tenancy::create_account_project(&mut tx, ISSUER, &subject).await?;
                sqlx::query(
                    "INSERT INTO native_credentials(user_id,email,password_phc) VALUES($1,$2,$3)",
                )
                .bind(&membership.user)
                .bind(ADMIN_LOGIN)
                .bind(phc)
                .execute(&mut *tx)
                .await?;
                audit(&mut tx, &membership.user, "instance.first_run_claimed").await?;
                membership.user
            }
        };
        sqlx::query("INSERT INTO instance_admins(user_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(&user)
            .execute(&mut *tx)
            .await?;
        let epoch: i64 =
            sqlx::query_scalar("SELECT credential_epoch FROM native_credentials WHERE user_id=$1")
                .bind(&user)
                .fetch_one(&mut *tx)
                .await?;
        let session = self.open_session(&mut tx, &user, epoch).await?;
        // A password the person chose needs no "did you save it" step.
        if generated_password.is_some() {
            sqlx::query("UPDATE native_credentials SET unsaved_session_sha256=$2 WHERE user_id=$1")
                .bind(&user)
                .bind(hash_key(&session))
                .execute(&mut *tx)
                .await?;
        }
        // A reset ends sessions, not keys: collectors must keep working after a lost
        // password. The person is told, in case someone else made these.
        let active_keys: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM project_keys WHERE created_by=$1 AND revoked_at IS NULL",
        )
        .bind(&user)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(Claimed {
            session,
            password: generated_password,
            revision: epoch,
            active_keys,
        }))
    }

    pub(crate) async fn password_status(&self, token: &str) -> Result<Option<PasswordStatus>> {
        if !valid_secret(token, SESSION_PREFIX) {
            return Ok(None);
        }
        let row: Option<(String,bool,i64)> = sqlx::query_as("SELECT c.email,c.unsaved_session_sha256 IS NOT DISTINCT FROM s.session_sha256,c.credential_epoch FROM native_sessions s JOIN native_credentials c ON c.user_id=s.user_id AND c.credential_epoch=s.credential_epoch WHERE s.session_sha256=$1 AND s.context_sha256=$2 AND s.expires_at>clock_timestamp()")
            .bind(hash_key(token)).bind(&self.context).fetch_optional(&self.pool).await?;
        Ok(row.map(|(signin, unsaved, revision)| PasswordStatus {
            signin,
            unsaved,
            revision,
        }))
    }

    pub(crate) async fn confirm_saved(&self, token: &str, revision: i64) -> Result<Saved> {
        if !valid_secret(token, SESSION_PREFIX) {
            return Ok(Saved::SignedOut);
        }
        let digest = hash_key(token);
        let mut tx = self.pool.begin().await?;
        let row: Option<(String,i64,bool)> = sqlx::query_as("SELECT c.user_id,c.credential_epoch,c.unsaved_session_sha256 IS NOT DISTINCT FROM s.session_sha256 FROM native_sessions s JOIN native_credentials c ON c.user_id=s.user_id AND c.credential_epoch=s.credential_epoch WHERE s.session_sha256=$1 AND s.context_sha256=$2 AND s.expires_at>clock_timestamp() FOR UPDATE OF c")
            .bind(&digest).bind(&self.context).fetch_optional(&mut *tx).await?;
        let Some((user, current, unsaved)) = row else {
            return Ok(Saved::SignedOut);
        };
        if current != revision {
            return Ok(Saved::Stale);
        }
        if unsaved {
            sqlx::query(
                "UPDATE native_credentials SET unsaved_session_sha256=NULL WHERE user_id=$1",
            )
            .bind(&user)
            .execute(&mut *tx)
            .await?;
            audit(&mut tx, &user, "account.password_saved").await?;
            tx.commit().await?;
        }
        Ok(Saved::Done)
    }

    /// For a page closed before the password was saved: it cannot be shown twice. Only the
    /// session that received it may do this, and signing in with the password ends that
    /// right, so a password someone is already using can never be replaced from here.
    pub(crate) async fn replace_unsaved(
        &self,
        token: &str,
    ) -> Result<Option<(Zeroizing<String>, i64)>> {
        if !self
            .password_status(token)
            .await?
            .is_some_and(|status| status.unsaved)
        {
            return Ok(None);
        }
        let password = generated();
        let phc = self
            .password_work(password.to_string(), None)
            .await?
            .expect("hash operation");
        let digest = hash_key(token);
        let mut tx = self.pool.begin().await?;
        let user: Option<String> = sqlx::query_scalar("SELECT c.user_id FROM native_sessions s JOIN native_credentials c ON c.user_id=s.user_id AND c.credential_epoch=s.credential_epoch WHERE s.session_sha256=$1 AND s.context_sha256=$2 AND s.expires_at>clock_timestamp() AND c.unsaved_session_sha256=s.session_sha256 FOR UPDATE OF c")
            .bind(&digest).bind(&self.context).fetch_optional(&mut *tx).await?;
        let Some(user) = user else {
            return Ok(None);
        };
        // The new epoch ends device grants too. Only this session moves with it.
        let revision: i64 = sqlx::query_scalar("UPDATE native_credentials SET password_phc=$2,credential_epoch=credential_epoch+1,changed_at=clock_timestamp() WHERE user_id=$1 RETURNING credential_epoch")
            .bind(&user).bind(phc).fetch_one(&mut *tx).await?;
        sqlx::query("DELETE FROM native_sessions WHERE user_id=$1 AND session_sha256<>$2")
            .bind(&user)
            .bind(&digest)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE native_sessions SET credential_epoch=$2 WHERE session_sha256=$1")
            .bind(&digest)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx, &user, "account.password_regenerated").await?;
        tx.commit().await?;
        Ok(Some((password, revision)))
    }
}
