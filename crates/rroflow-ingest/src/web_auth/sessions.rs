//! Durable credential envelopes. PostgreSQL sees cookie digests and AEAD ciphertext,
//! never an authenticating cookie or a plaintext federation access token.
use crate::secret_box::SecretBox;
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::path::Path;

pub(super) struct DurableSessions {
    pool: PgPool,
    context: String,
    cipher: SecretBox,
    capacity: i64,
}

impl DurableSessions {
    pub(super) fn new(
        pool: PgPool,
        path: &Path,
        context: &[&str],
        capacity: usize,
    ) -> Result<Self> {
        let cipher = SecretBox::load(path)?;
        Ok(Self {
            pool,
            context: hex::encode(Sha256::digest(serde_json::to_vec(context)?)),
            cipher,
            capacity: i64::try_from(capacity)?,
        })
    }

    fn aad(&self, digest: &str, user: &str, key: &str, expires: i64) -> Vec<u8> {
        serde_json::to_vec(&(
            "rroflow.browser-session/v1",
            &self.context,
            digest,
            user,
            key,
            expires,
        ))
        .expect("fixed tuple serialization")
    }

    pub(super) async fn get(&self, id: &str) -> Result<Option<String>> {
        if !valid_cookie(id) {
            return Ok(None);
        }
        let digest = crate::hash_key(id);
        let row = sqlx::query("SELECT user_id,key_id,nonce,ciphertext,expires_at FROM browser_sessions WHERE context_sha256=$1 AND session_sha256=$2 AND expires_at>clock_timestamp()")
            .bind(&self.context).bind(&digest).fetch_optional(&self.pool).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let key_id: String = row.try_get("key_id")?;
        let user: String = row.try_get("user_id")?;
        let expiry: DateTime<Utc> = row.try_get("expires_at")?;
        let nonce: Vec<u8> = row.try_get("nonce")?;
        let ciphertext: Vec<u8> = row.try_get("ciphertext")?;
        let aad = self.aad(&digest, &user, &key_id, expiry.timestamp());
        Ok(self
            .cipher
            .open(&key_id, &nonce, &ciphertext, &aad)
            .and_then(|plain| String::from_utf8(plain).ok()))
    }

    pub(super) async fn put(
        &self,
        id: &str,
        old: Option<&str>,
        user: &str,
        token: &str,
        ttl: u64,
    ) -> Result<()> {
        ensure!(
            valid_cookie(id)
                && !token.is_empty()
                && token.len() <= 65536
                && (1..=86400).contains(&ttl),
            "invalid browser session"
        );
        let mut tx = self.pool.begin().await?;
        // Serialize the capacity/replacement decision across serving instances.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("rroflow.browser-session/{}", self.context))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM browser_sessions WHERE context_sha256=$1 AND expires_at<=clock_timestamp()")
            .bind(&self.context).execute(&mut *tx).await?;
        if let Some(old) = old.filter(|id| valid_cookie(id)) {
            sqlx::query(
                "DELETE FROM browser_sessions WHERE context_sha256=$1 AND session_sha256=$2",
            )
            .bind(&self.context)
            .bind(crate::hash_key(old))
            .execute(&mut *tx)
            .await?;
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM browser_sessions WHERE context_sha256=$1")
                .bind(&self.context)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(count < self.capacity, "browser session capacity reached");
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()))::bigint")
                .fetch_one(&mut *tx)
                .await?;
        let expiry = now + i64::try_from(ttl)?;
        let digest = crate::hash_key(id);
        let aad = self.aad(&digest, user, self.cipher.active(), expiry);
        let (nonce, ciphertext) = self.cipher.seal(token.as_bytes(), &aad)?;
        sqlx::query("INSERT INTO browser_sessions(context_sha256,session_sha256,user_id,key_id,nonce,ciphertext,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&self.context).bind(digest).bind(user).bind(self.cipher.active()).bind(nonce.as_slice())
            .bind(ciphertext).bind(DateTime::<Utc>::from_timestamp(expiry,0).context("invalid session expiry")?)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn remove(&self, id: &str) -> Result<()> {
        if valid_cookie(id) {
            sqlx::query(
                "DELETE FROM browser_sessions WHERE context_sha256=$1 AND session_sha256=$2",
            )
            .bind(&self.context)
            .bind(crate::hash_key(id))
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}

fn valid_cookie(id: &str) -> bool {
    id.len() == 43
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

#[cfg(test)]
mod tests;
