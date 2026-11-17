//! Service configuration (TOML) and the rules keys and devspace ids are checked against.
use crate::{
    AccountConfig, BrandConfig, BrowserConfig, JsonlStore, PgStore, Store, TenancyConfig, datasets,
    sdk_activity,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Service configuration (TOML). Every value is configuration; defaults are in [`Config::default`].
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Address to listen on.
    pub listen: String,
    /// Postgres connection URL. When set, data lives in Postgres; when not, in JSONL files
    /// under `data_dir`. `RROFLOW_DATABASE_URL` overrides it, so a password never has to sit
    /// in the file (on one host, prefer a Unix socket with peer authentication).
    pub database_url: Option<String>,
    /// Largest Postgres connection pool.
    pub db_max_connections: u32,
    /// Root directory for JSONL devspace data (used only without `database_url`).
    pub data_dir: PathBuf,
    /// Most records accepted per request.
    pub max_batch: usize,
    /// Largest request body in bytes.
    pub max_body_bytes: usize,
    /// Devspaces and the hashes of their ingest keys.
    pub devspace: Vec<Devspace>,
    /// Sign-in with Nomen, the identity shared by all Shippin services. When set, a Nomen
    /// access token opens the devspace its account owns (created on first sign-in).
    /// Requires `database_url`.
    pub nomen: Option<NomenConfig>,
    /// Optional HTTPS dashboard sign-in through Nomen.
    pub browser: Option<BrowserConfig>,
    pub accounts: Option<AccountConfig>,
    /// Public/member presentation, validated and rendered before serving requests.
    pub brand: BrandConfig,
    /// Fail closed unless a database-owned allowance policy has been installed.
    pub require_allowance_policy: bool,
    /// Operator-enabled metadata dataset collection. Training is not enabled by this.
    pub datasets: Option<datasets::DatasetConfig>,
    pub sdk_activity: sdk_activity::ActivityConfig,
    /// Bounds on the organizations and projects one person can create.
    pub tenancy: TenancyConfig,
}

/// How the ingest checks Nomen tokens (RFC 7662 introspection, private-key JWT).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NomenConfig {
    /// Nomen issuer URL, for example `https://id.shippin.cloud`.
    pub issuer: String,
    /// Nomen project ids a token's audience must name (the rroflow project).
    pub audiences: Vec<String>,
    /// Client ids allowed to have obtained the token (the rroflow CLI and web clients).
    pub clients: Vec<String>,
    /// Key file of the ingest's API application in Nomen (JSON, mode 0600).
    pub key_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8787".into(),
            database_url: None,
            db_max_connections: 10,
            data_dir: PathBuf::from("data"),
            max_batch: 500,
            max_body_bytes: 16 * 1024 * 1024,
            devspace: vec![],
            nomen: None,
            browser: None,
            accounts: None,
            brand: BrandConfig::default(),
            require_allowance_policy: false,
            datasets: None,
            sdk_activity: sdk_activity::ActivityConfig::default(),
            tenancy: TenancyConfig::default(),
        }
    }
}

impl Config {
    pub fn validate_authentication(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.nomen.is_some()
                || self.accounts.is_some()
                || self.devspace.iter().any(|d| !d.key_sha256.is_empty()),
            "configure native accounts, Nomen sign-in or at least one devspace key"
        );
        Ok(())
    }
    /// Open the store this config names.
    pub async fn open_store(&self) -> anyhow::Result<Store> {
        self.sdk_activity.validate()?;
        if let Some(cfg) = &self.datasets {
            cfg.validate()?;
            anyhow::ensure!(
                self.database_url.is_some(),
                "dataset collection needs Postgres"
            );
        }
        anyhow::ensure!(
            !self.require_allowance_policy || self.database_url.is_some(),
            "required allowance policy needs Postgres"
        );
        let store = match &self.database_url {
            Some(url) => {
                let mut pg = PgStore::connect(url, self.db_max_connections).await?;
                pg.require_allowance_policy = self.require_allowance_policy;
                if self.require_allowance_policy {
                    let exists: bool =
                        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM allowance_policy)")
                            .fetch_one(&pg.pool)
                            .await?;
                    anyhow::ensure!(
                        exists,
                        "install an allowance policy before serving with require_allowance_policy=true"
                    );
                }
                Store::Postgres(pg)
            }
            None => Store::Jsonl(JsonlStore::open(self.data_dir.clone())?),
        };
        let ids: Vec<String> = self.devspace.iter().map(|d| d.id.clone()).collect();
        store.ensure_devspaces(&ids).await?;
        Ok(store)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Devspace {
    /// The devspace id this data belongs to.
    pub id: String,
    /// SHA-256 (hex) of each ingest key that writes to this devspace.
    pub key_sha256: Vec<String>,
}

/// SHA-256 of a key, hex-encoded: the only form in which keys are stored.
pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// Characters allowed in a devspace id (it becomes a directory name and a database key).
pub fn valid_devspace_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
