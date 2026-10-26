//! rroflow-ingest: the rroflow.ai service that receives `rroflow.turn/v1` records from
//! collectors and files them into the sender's devspace.
//!
//! - **Tenancy:** a devspace is one person's space on rroflow.ai, owned by their Nomen
//!   account and attachable to their Shippin.cloud tenant. The credential decides the
//!   devspace; a client cannot name another one. Ingest keys are stored only as SHA-256
//!   hashes.
//! - **Idempotence:** records upsert by `event_id`, so re-sending replaces, never duplicates.
//! - **Privacy:** the service never logs record content; content arrives only when the
//!   user opted in on the collector.
//! - **Store:** [`Store`]: Postgres when `database_url` is set, else JSONL files.
//! - **Signals:** the catalogue is public at `GET /v1/signals`; each devspace keeps its own
//!   choices (on/off, thresholds) at `/v1/signals/settings`, which collectors read before
//!   evaluating.

pub mod pg;
pub mod store;

pub use pg::PgStore;
pub use store::{JsonlStore, Store, Summary, TurnFilter, Upserted};

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rroflow_core::signals::{CATALOGUE, Category, SignalConfig};
use rroflow_core::{SCHEMA, TurnRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
        }
    }
}

impl Config {
    /// Open the store this config names.
    pub async fn open_store(&self) -> anyhow::Result<Store> {
        let store = match &self.database_url {
            Some(url) => Store::Postgres(PgStore::connect(url, self.db_max_connections).await?),
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

/// Shared state: key-hash → devspace, the store, limits.
#[derive(Clone)]
pub struct AppState {
    keys: Arc<HashMap<String, String>>,
    store: Arc<Store>,
    max_batch: usize,
}

impl AppState {
    pub fn new(cfg: &Config, store: Arc<Store>) -> anyhow::Result<Self> {
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
        Ok(Self {
            keys: Arc::new(keys),
            store,
            max_batch: cfg.max_batch,
        })
    }

    fn devspace(&self, headers: &HeaderMap) -> Result<String, ApiError> {
        let key = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;
        self.keys
            .get(&hash_key(key.trim()))
            .cloned()
            .ok_or(ApiError::Unauthorized)
    }
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    BadRequest(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "missing or unknown ingest key".to_string(),
            ),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "store error".into()),
        };
        (code, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

/// Log a store failure (never record content) and answer 500.
fn internal(e: anyhow::Error) -> ApiError {
    eprintln!("store error: {e:#}");
    ApiError::Internal
}

#[derive(Deserialize)]
pub struct TurnsBody {
    pub turns: Vec<TurnRecord>,
}

/// Check one record before it is stored. Returns the problem in plain words.
fn check_record(r: &TurnRecord) -> Result<(), String> {
    if r.schema != SCHEMA {
        return Err(format!("schema {:?} is not {SCHEMA}", r.schema));
    }
    if r.event_id.is_empty() {
        return Err("event_id is empty".into());
    }
    for (field, v) in [("started_at", &r.started_at), ("ended_at", &r.ended_at)] {
        if !v.is_empty() && pg::timestamp(v).is_none() {
            return Err(format!("{field} {v:?} is not an RFC 3339 timestamp"));
        }
    }
    Ok(())
}

async fn post_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TurnsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.devspace(&headers)?;
    if body.turns.len() > st.max_batch {
        return Err(ApiError::BadRequest(format!(
            "batch of {} exceeds the limit of {}",
            body.turns.len(),
            st.max_batch
        )));
    }
    for (i, r) in body.turns.iter().enumerate() {
        check_record(r).map_err(|m| ApiError::BadRequest(format!("turns[{i}]: {m}")))?;
    }
    let up = st
        .store
        .upsert(&devspace, &body.turns)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "accepted": body.turns.len(),
        "new": up.new,
        "replaced": up.replaced,
    })))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub agent: Option<String>,
    pub signal: Option<String>,
}

/// Most records one read returns.
const LIST_MAX: usize = 1000;

async fn get_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.devspace(&headers)?;
    let limit = q.limit.unwrap_or(50).min(LIST_MAX);
    let turns = st
        .store
        .list(
            &devspace,
            limit,
            &TurnFilter {
                agent: q.agent.as_deref().filter(|a| !a.is_empty()),
                signal: q.signal.as_deref().filter(|s| !s.is_empty()),
            },
        )
        .await
        .map_err(internal)?;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "turns": turns }),
    ))
}

/// Schema id of the signal catalogue document.
pub const SIGNALS_SCHEMA: &str = "rroflow.signals/v1";

/// The signal catalogue, grouped: every category with its signals in catalogue order.
fn catalogue() -> serde_json::Value {
    serde_json::json!({
        "schema": SIGNALS_SCHEMA,
        "categories": Category::ALL
            .iter()
            .map(|c| serde_json::json!({
                "id": c,
                "label": c.label(),
                "meaning": c.meaning(),
                "signals": CATALOGUE.iter().filter(|s| s.category == *c).map(|s| s.id).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "signals": CATALOGUE,
    })
}

async fn get_signals() -> Json<serde_json::Value> {
    Json(catalogue())
}

async fn get_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.devspace(&headers)?;
    let settings = st.store.signal_config(&devspace).await.map_err(internal)?;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "settings": settings }),
    ))
}

async fn put_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(cfg): Json<SignalConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.devspace(&headers)?;
    cfg.validate().map_err(ApiError::BadRequest)?;
    st.store
        .set_signal_config(&devspace, &cfg)
        .await
        .map_err(internal)?;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "settings": cfg }),
    ))
}

async fn get_summary(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.devspace(&headers)?;
    let (summary, settings) = tokio::try_join!(
        st.store.summary(&devspace),
        st.store.signal_config(&devspace)
    )
    .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "summary": summary,
        "catalogue": catalogue(),
        "settings": settings,
    })))
}

/// The devspace dashboard: one self-contained page (no external scripts) that asks for the
/// ingest key, keeps it in the tab's session storage, and reads the API.
async fn dashboard() -> impl IntoResponse {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            (
                "content-security-policy",
                "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
            ),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "no-referrer"),
        ],
        include_str!("dashboard.html"),
    )
}

/// The HTTP surface: `GET /` (dashboard), `POST|GET /v1/turns`, `GET /v1/summary`,
/// `GET /v1/signals`, `GET|PUT /v1/signals/settings`, `GET /healthz`.
pub fn router(st: AppState, max_body_bytes: usize) -> Router {
    Router::new()
        .route("/v1/turns", post(post_turns).get(get_turns))
        .route("/v1/summary", get(get_summary))
        .route("/v1/signals", get(get_signals))
        .route(
            "/v1/signals/settings",
            get(get_signal_settings).put(put_signal_settings),
        )
        .route("/healthz", get(|| async { "ok" }))
        .route("/", get(dashboard))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(st)
}

#[cfg(test)]
mod tests;
