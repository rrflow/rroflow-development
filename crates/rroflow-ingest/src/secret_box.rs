//! Authenticated credential envelopes; callers define and bind their own context.
use anyhow::{Context, Result, ensure};
use aws_lc_rs::aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey};
use serde::Deserialize;
use std::{collections::BTreeMap, io::Read, path::Path};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    active: String,
    keys: BTreeMap<String, String>,
}

pub(crate) struct SecretBox {
    active: String,
    keys: BTreeMap<String, RandomizedNonceKey>,
}
impl SecretBox {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).context("cannot open private encryption key file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                file.metadata()?.permissions().mode() & 0o077 == 0,
                "encryption key file must be private (mode 0600)"
            );
        }
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(16385).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16384, "encryption key file exceeds limit");
        let cfg: KeyFile = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid encryption key file"))?;
        ensure!(
            !cfg.keys.is_empty() && cfg.keys.len() <= 3 && cfg.keys.contains_key(&cfg.active),
            "key ring needs an active key and at most three keys"
        );
        let mut keys = BTreeMap::new();
        for (id, value) in cfg.keys {
            let value = Zeroizing::new(value);
            ensure!(
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
                "invalid encryption key id"
            );
            ensure!(value.len() == 64, "encryption keys must be 32 bytes of hex");
            let raw = Zeroizing::new(
                hex::decode(value.as_bytes())
                    .map_err(|_| anyhow::anyhow!("invalid encryption key"))?,
            );
            let key = RandomizedNonceKey::new(&AES_256_GCM, &raw)
                .map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
            keys.insert(id, key);
        }
        Ok(Self {
            active: cfg.active,
            keys,
        })
    }
    pub(crate) fn active(&self) -> &str {
        &self.active
    }
    pub(crate) fn seal(&self, plain: &[u8], aad: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut ciphertext = plain.to_vec();
        let nonce = self.keys[&self.active]
            .seal_in_place_append_tag(Aad::from(aad), &mut ciphertext)
            .map_err(|_| anyhow::anyhow!("credential encryption failed"))?;
        Ok((nonce.as_ref().to_vec(), ciphertext))
    }
    pub(crate) fn open(
        &self,
        id: &str,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Option<Vec<u8>> {
        let key = self.keys.get(id)?;
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut ciphertext = ciphertext.to_vec();
        key.open_in_place(nonce, Aad::from(aad), &mut ciphertext)
            .ok()
            .map(|p| p.to_vec())
    }
}
