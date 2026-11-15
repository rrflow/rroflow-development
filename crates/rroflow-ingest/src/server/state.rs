//! The state every handler receives.
use super::{Config, valid_devspace_id};
use crate::{NomenVerifier, Store, TenancyConfig, accounts, brand, datasets, web_auth};
use std::collections::HashMap;
use std::sync::Arc;

/// Shared state: key-hash → devspace, the Nomen verifier, the store, limits.
#[derive(Clone)]
pub struct AppState {
    pub(crate) datasets: Option<datasets::DatasetConfig>,
    pub(crate) keys: Arc<HashMap<String, String>>,
    pub(crate) brand: Arc<brand::RenderedBrand>,
    pub(crate) web: Option<Arc<web_auth::WebAuth>>,
    pub(crate) accounts: Option<Arc<accounts::Accounts>>,
    pub(crate) nomen: Option<Arc<NomenVerifier>>,
    pub(crate) store: Arc<Store>,
    pub(crate) max_batch: usize,
    pub(crate) tenancy: TenancyConfig,
}

impl AppState {
    pub fn new(cfg: &Config, store: Arc<Store>) -> anyhow::Result<Self> {
        cfg.sdk_activity.validate()?;
        if let Some(datasets) = &cfg.datasets {
            datasets.validate()?;
            anyhow::ensure!(
                matches!(*store, Store::Postgres(_)),
                "dataset collection needs Postgres"
            );
        }
        let mut keys = HashMap::new();
        for d in &cfg.devspace {
            anyhow::ensure!(
                valid_devspace_id(&d.id),
                "devspace id {:?} must be 1-64 of [A-Za-z0-9_-]",
                d.id
            );
            for h in &d.key_sha256 {
                anyhow::ensure!(
                    h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()),
                    "devspace {}: key_sha256 entries must be 64 hex characters",
                    d.id
                );
                if let Some(prev) = keys.insert(h.to_ascii_lowercase(), d.id.clone()) {
                    anyhow::bail!(
                        "one key hash is assigned to two devspaces: {prev} and {}",
                        d.id
                    );
                }
            }
        }
        let nomen = match &cfg.nomen {
            None => None,
            Some(n) => {
                anyhow::ensure!(
                    matches!(*store, Store::Postgres(_)),
                    "[nomen] needs database_url: devspace owners are kept in Postgres"
                );
                anyhow::ensure!(
                    !n.audiences.is_empty() && !n.clients.is_empty(),
                    "[nomen] needs at least one audience and one client"
                );
                Some(Arc::new(NomenVerifier::new(
                    &n.issuer,
                    &n.audiences,
                    &n.clients,
                    &n.key_file,
                )?))
            }
        };
        let accounts = cfg
            .accounts
            .as_ref()
            .map(|a| {
                let Store::Postgres(pg) = store.as_ref() else {
                    anyhow::bail!("native accounts require PostgreSQL");
                };
                if let Some(browser) = &cfg.browser {
                    anyhow::ensure!(
                        browser.origin.trim_end_matches('/') == a.origin.trim_end_matches('/'),
                        "native and federated browser origins must match"
                    );
                }
                accounts::Accounts::new(a, pg.pool.clone()).map(Arc::new)
            })
            .transpose()?;
        Ok(Self {
            datasets: cfg.datasets.clone(),
            keys: Arc::new(keys),
            brand: Arc::new(brand::RenderedBrand::new(&cfg.brand)?),
            web: web_auth::WebAuth::new(
                cfg,
                match store.as_ref() {
                    Store::Postgres(pg) => Some(&pg.pool),
                    Store::Jsonl(_) => None,
                },
            )?,
            nomen,
            accounts,
            store,
            max_batch: cfg.max_batch,
            tenancy: cfg.tenancy.clone(),
        })
    }
}
