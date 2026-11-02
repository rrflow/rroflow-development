//! Durable credential envelopes. PostgreSQL sees cookie digests and AEAD ciphertext,
//! never an authenticating cookie or a plaintext federation access token.
use anyhow::{Context, Result, ensure};
use aws_lc_rs::aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    active: String,
    keys: BTreeMap<String, String>,
}

pub(super) struct DurableSessions {
    pool: PgPool,
    context: String,
    active: String,
    keys: BTreeMap<String, RandomizedNonceKey>,
    capacity: i64,
}

impl DurableSessions {
    pub(super) fn new(
        pool: PgPool,
        path: &Path,
        context: &[&str],
        capacity: usize,
    ) -> Result<Self> {
        let file = std::fs::File::open(path).context("cannot open browser session key file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                file.metadata()?.permissions().mode() & 0o077 == 0,
                "browser session key file must be private (mode 0600)"
            );
        }
        let mut bytes = Vec::new();
        file.take(16385).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 16384,
            "browser session key file exceeds limit"
        );
        let cfg: KeyFile = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid browser session key file"))?;
        ensure!(
            !cfg.keys.is_empty() && cfg.keys.len() <= 3 && cfg.keys.contains_key(&cfg.active),
            "session key ring needs an active key and at most three keys"
        );
        let mut keys = BTreeMap::new();
        for (id, value) in cfg.keys {
            ensure!(
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
                "invalid session key id"
            );
            ensure!(
                value.len() == 64,
                "session encryption keys must be 32 bytes of hex"
            );
            let raw = hex::decode(value)
                .map_err(|_| anyhow::anyhow!("invalid session encryption key"))?;
            let key = RandomizedNonceKey::new(&AES_256_GCM, &raw)
                .map_err(|_| anyhow::anyhow!("invalid session encryption key"))?;
            keys.insert(id, key);
        }
        Ok(Self {
            pool,
            context: hex::encode(Sha256::digest(serde_json::to_vec(context)?)),
            active: cfg.active,
            keys,
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
        let Some(key) = self.keys.get(&key_id) else {
            return Ok(None);
        };
        let user: String = row.try_get("user_id")?;
        let expiry: DateTime<Utc> = row.try_get("expires_at")?;
        let nonce: Vec<u8> = row.try_get("nonce")?;
        let Ok(nonce) = Nonce::try_assume_unique_for_key(&nonce) else {
            return Ok(None);
        };
        let mut ciphertext: Vec<u8> = row.try_get("ciphertext")?;
        let aad = self.aad(&digest, &user, &key_id, expiry.timestamp());
        let Ok(plain) = key.open_in_place(nonce, Aad::from(aad), &mut ciphertext) else {
            return Ok(None);
        };
        Ok(std::str::from_utf8(plain).ok().map(str::to_owned))
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
        let aad = self.aad(&digest, user, &self.active, expiry);
        let mut ciphertext = token.as_bytes().to_vec();
        let nonce = self.keys[&self.active]
            .seal_in_place_append_tag(Aad::from(aad), &mut ciphertext)
            .map_err(|_| anyhow::anyhow!("session encryption failed"))?;
        sqlx::query("INSERT INTO browser_sessions(context_sha256,session_sha256,user_id,key_id,nonce,ciphertext,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&self.context).bind(digest).bind(user).bind(&self.active).bind(nonce.as_ref())
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
