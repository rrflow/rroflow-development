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

pub mod allowances;
mod analytics;
mod brand;
pub mod datasets;
pub mod sdk_activity;
pub use brand::{BrandConfig, BrandLink};
mod web_auth;
pub use web_auth::BrowserConfig;
mod capture;
pub mod nomen;
pub mod pg;
mod reviews;
mod source_health;
pub mod store;
#[cfg(test)]
mod test_keys;

pub use nomen::{AuthError, NomenVerifier, Subject};
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

/// Embedded at build time, independent of the serving host's checkout or environment.
#[derive(Serialize)]
pub struct BuildInfo {
    pub revision: &'static str,
    pub target: &'static str,
    pub migrations: &'static str,
}

pub fn build_info() -> BuildInfo {
    BuildInfo {
        revision: env!("RROFLOW_RELEASE_REVISION"),
        target: env!("RROFLOW_RELEASE_TARGET"),
        migrations: env!("RROFLOW_MIGRATIONS_SHA256"),
    }
}

async fn ready(State(st): State<AppState>) -> impl IntoResponse {
    match tokio::time::timeout(std::time::Duration::from_secs(2), st.store.ready()).await {
        Ok(Ok(())) => (StatusCode::OK, "ready"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "store unavailable"),
    }
}

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
    /// Public/member presentation, validated and rendered before serving requests.
    pub brand: BrandConfig,
    /// Fail closed unless a database-owned allowance policy has been installed.
    pub require_allowance_policy: bool,
    /// Operator-enabled metadata dataset collection. Training is not enabled by this.
    pub datasets: Option<datasets::DatasetConfig>,
    pub sdk_activity: sdk_activity::ActivityConfig,
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
            brand: BrandConfig::default(),
            require_allowance_policy: false,
            datasets: None,
            sdk_activity: sdk_activity::ActivityConfig::default(),
        }
    }
}

impl Config {
    pub fn validate_authentication(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.nomen.is_some() || self.devspace.iter().any(|d| !d.key_sha256.is_empty()),
            "configure Nomen sign-in or at least one devspace key"
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

/// Who is calling, as their credential proved it.
#[derive(Clone, Debug, Serialize)]
pub struct Caller {
    /// The devspace every read and write of this request is confined to.
    pub devspace: String,
    /// `key` (ingest key) or `nomen` (Nomen sign-in).
    pub via: &'static str,
    /// Nomen account ID, for `nomen`.
    pub subject: Option<String>,
    /// Nomen organization, when the token carried it.
    pub org: Option<String>,
    /// Login name Nomen reported, for display only.
    pub name: Option<String>,
}

/// Shared state: key-hash → devspace, the Nomen verifier, the store, limits.
#[derive(Clone)]
pub struct AppState {
    datasets: Option<datasets::DatasetConfig>,
    keys: Arc<HashMap<String, String>>,
    brand: Arc<brand::RenderedBrand>,
    web: Option<Arc<web_auth::WebAuth>>,
    nomen: Option<Arc<NomenVerifier>>,
    store: Arc<Store>,
    max_batch: usize,
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
        Ok(Self {
            datasets: cfg.datasets.clone(),
            keys: Arc::new(keys),
            brand: Arc::new(brand::RenderedBrand::new(&cfg.brand)?),
            web: web_auth::WebAuth::new(cfg)?,
            nomen,
            store,
            max_batch: cfg.max_batch,
        })
    }

    /// Identify the caller from `Authorization: Bearer <credential>`: an ingest key first
    /// (a hash lookup), then a Nomen access token when sign-in is configured.
    pub async fn caller(&self, headers: &HeaderMap) -> Result<Caller, ApiError> {
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or(ApiError::Unauthorized)?;
        if let Some(devspace) = self.keys.get(&hash_key(token)) {
            return Ok(Caller {
                devspace: devspace.clone(),
                via: "key",
                subject: None,
                org: None,
                name: None,
            });
        }
        let Some(nomen) = &self.nomen else {
            return Err(ApiError::Unauthorized);
        };
        let subject = match nomen.verify(token).await {
            Ok(s) => s,
            Err(AuthError::Invalid(why)) => {
                eprintln!("sign-in refused: {why}");
                return Err(ApiError::Unauthorized);
            }
            Err(AuthError::Unavailable(why)) => {
                eprintln!("issuer_unavailable: {why}");
                return Err(ApiError::IssuerUnavailable);
            }
        };
        let devspace = self
            .store
            .devspace_for_owner(nomen.issuer(), &subject)
            .await
            .map_err(internal)?;
        Ok(Caller {
            devspace,
            via: subject.via,
            subject: Some(subject.id),
            org: subject.org,
            name: subject.name,
        })
    }
}

#[derive(Debug)]
pub enum ApiError {
    DatasetPolicyConflict,
    MemberRequired,
    NotFound,
    Quota(allowances::QuotaExceeded),
    AllowanceUnavailable,
    Unauthorized,
    Conflict,
    SettingsConflict,
    SourceConflict,
    /// Nomen could not be asked about a token; retry. Never a refusal.
    IssuerUnavailable,
    BadRequest(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::Quota(quota) = self {
            return (StatusCode::TOO_MANY_REQUESTS, [("Retry-After", "60")], Json(serde_json::json!({
                "error":"allowance_exceeded", "message":"This workspace has reached its monthly allowance. Keep pending captures locally and retry later.",
                "quota": quota
            }))).into_response();
        }
        let (code, msg) = match self {
            ApiError::DatasetPolicyConflict => (
                StatusCode::CONFLICT,
                "Dataset permission changed. Refresh before saving.".into(),
            ),
            ApiError::MemberRequired => (
                StatusCode::FORBIDDEN,
                "Sign in with your member account to manage dataset consent or exports.".into(),
            ),
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                "Dataset unavailable, expired or withdrawn.".into(),
            ),
            ApiError::Quota(_) => unreachable!(),
            ApiError::AllowanceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "allowance_policy_unavailable: keep pending captures and retry".into(),
            ),
            ApiError::Conflict => (StatusCode::CONFLICT, "capture identity conflict".into()),
            ApiError::SourceConflict => (
                StatusCode::CONFLICT,
                "source report sequence is stale or conflicting".into(),
            ),
            ApiError::SettingsConflict => (
                StatusCode::CONFLICT,
                "Signal settings changed in another tab. Refresh before saving.".into(),
            ),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "missing or unknown credential: an ingest key or a Nomen sign-in".to_string(),
            ),
            ApiError::IssuerUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "issuer_unavailable: Nomen could not be reached to check the sign-in; retry".into(),
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
    Json(mut body): Json<TurnsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.caller(&headers).await?.devspace;
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
    let settings = st.store.signal_config(&devspace).await.map_err(internal)?;
    for turn in &mut body.turns {
        // This namespace belongs to the receiving devspace, never the submitting client.
        turn.signals
            .retain(|hit| !hit.signal.starts_with("custom_"));
        for signal in settings.custom.iter().filter(|s| s.enabled) {
            if let rroflow_core::custom::Evaluation::Match(hit) = signal.evaluate(turn) {
                turn.signals.push(hit);
            }
        }
    }
    let up = st
        .store
        .upsert(&devspace, &body.turns)
        .await
        .map_err(allowances::ingest_error)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "accepted": body.turns.len(),
        "new": up.new,
        "replaced": up.replaced,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub agent: Option<String>,
    pub signal: Option<String>,
    pub project: Option<String>,
    pub session: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub cursor: Option<String>,
    pub event: Option<String>,
}

/// Most records one read returns.
const LIST_MAX: usize = 1000;

async fn get_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.caller(&headers).await?.devspace;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let limit = q.limit.unwrap_or(50).clamp(1, LIST_MAX);
    let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if search.is_some_and(|s| s.chars().count() > 200) {
        return Err(ApiError::BadRequest(
            "Search must be 200 characters or fewer".into(),
        ));
    }
    let since = q
        .since
        .as_deref()
        .map(|value| {
            pg::timestamp(value).ok_or_else(|| ApiError::BadRequest("Invalid start date".into()))
        })
        .transpose()?;
    let until = q
        .until
        .as_deref()
        .map(|value| {
            pg::timestamp(value).ok_or_else(|| ApiError::BadRequest("Invalid end date".into()))
        })
        .transpose()?;
    if since.zip(until).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::BadRequest("Start must be before end".into()));
    }
    let cursor: Option<(String, String)> = q
        .cursor
        .as_deref()
        .map(|value| {
            if value.len() > 4096 {
                return Err(ApiError::BadRequest("Invalid page cursor".into()));
            }
            URL_SAFE_NO_PAD
                .decode(value)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .ok_or_else(|| ApiError::BadRequest("Invalid page cursor".into()))
        })
        .transpose()?;
    let before = cursor
        .as_ref()
        .map(|(at, id)| {
            if id.is_empty() || (!at.is_empty() && pg::timestamp(at).is_none()) {
                return Err(ApiError::BadRequest("Invalid page cursor".into()));
            }
            Ok((pg::timestamp(at), id.as_str()))
        })
        .transpose()?;
    let mut turns = st
        .store
        .list(
            &devspace,
            limit + 1,
            &TurnFilter {
                agent: q.agent.as_deref().filter(|a| !a.is_empty()),
                signal: q.signal.as_deref().filter(|s| !s.is_empty()),
                project: q.project.as_deref().filter(|s| !s.is_empty()),
                session: q.session.as_deref().filter(|s| !s.is_empty()),
                search,
                since,
                until,
                before,
                event: q.event.as_deref().filter(|s| !s.is_empty()),
            },
        )
        .await
        .map_err(internal)?;
    let has_more = turns.len() > limit;
    turns.truncate(limit);
    let next_cursor = if has_more {
        turns.last().map(|turn| {
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&(
                    pg::timestamp(&turn.started_at)
                        .map(|at| at.to_rfc3339())
                        .unwrap_or_default(),
                    &turn.event_id,
                ))
                .expect("string tuple serializes"),
            )
        })
    } else {
        None
    };
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "next_cursor": next_cursor, "revisions":turns.iter().map(|t| (t.event_id.clone(), reviews::digest(t))).collect::<std::collections::BTreeMap<_,_>>(), "turns": turns }),
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
    let devspace = st.caller(&headers).await?.devspace;
    let settings = st.store.signal_config(&devspace).await.map_err(internal)?;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "settings": settings }),
    ))
}

async fn put_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(mut cfg): Json<SignalConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.caller(&headers).await?.devspace;
    cfg.validate().map_err(ApiError::BadRequest)?;
    let old = st.store.signal_config(&devspace).await.map_err(internal)?;
    if old.revision != cfg.revision {
        return Err(ApiError::SettingsConflict);
    }
    cfg.validate_update(&old).map_err(ApiError::BadRequest)?;
    let saved = st
        .store
        .set_signal_config(&devspace, &cfg)
        .await
        .map_err(internal)?;
    if !saved {
        return Err(ApiError::SettingsConflict);
    }
    cfg.revision += 1;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "settings": cfg }),
    ))
}

async fn preview_signal(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(rule): Json<rroflow_core::custom::CustomSignal>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.caller(&headers).await?.devspace;
    rule.validate().map_err(ApiError::BadRequest)?;
    let turns = st
        .store
        .list(
            &devspace,
            101,
            &store::TurnFilter {
                agent: rule.agent.as_deref(),
                project: rule.project.as_deref(),
                ..Default::default()
            },
        )
        .await
        .map_err(internal)?;
    let mut matches = vec![];
    let mut evaluated = 0;
    let mut unavailable = 0;
    for turn in turns.iter().take(100) {
        match rule.evaluate(turn) {
            rroflow_core::custom::Evaluation::Match(hit) => {
                evaluated += 1;
                matches.push(serde_json::json!({"event_id":turn.event_id,"hit":hit}));
            }
            rroflow_core::custom::Evaluation::NoMatch => evaluated += 1,
            rroflow_core::custom::Evaluation::Unavailable => unavailable += 1,
            rroflow_core::custom::Evaluation::OutOfScope => {}
        }
    }
    Ok(Json(
        serde_json::json!({"scanned":turns.len().min(100),"evaluated":evaluated,"unavailable":unavailable,"more_history":turns.len()>100,"matches":matches}),
    ))
}

async fn get_summary(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.caller(&headers).await?.devspace;
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

/// Static public product page; no third-party scripts or client-side provider credentials.
async fn public_site(State(st): State<AppState>) -> impl IntoResponse {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            (
                "content-security-policy",
                "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "no-referrer"),
            ("cache-control", "no-store"),
        ],
        st.brand.public.clone(),
    )
}

/// Same-origin dashboard assets; no third-party scripts or client-side provider credentials.
async fn dashboard(State(st): State<AppState>) -> impl IntoResponse {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            (
                "content-security-policy",
                "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "no-referrer"),
            ("cache-control", "no-store"),
        ],
        st.brand.dashboard.clone(),
    )
}

/// Source notes are visible only after workspace authentication.
async fn research_notes(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    st.caller(&headers).await?;
    Ok((
        [
            ("content-type", "application/json; charset=utf-8"),
            ("cache-control", "no-store"),
            ("x-content-type-options", "nosniff"),
        ],
        include_str!("research.json"),
    ))
}

/// Who the credential belongs to and which devspace it opens.
async fn get_whoami(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Caller>, ApiError> {
    Ok(Json(st.caller(&headers).await?))
}

/// The HTTP surface: `GET /` (public), `GET /app` (dashboard), `POST|GET /v1/turns`, `GET /v1/summary`,
/// `GET /v1/signals`, `GET|PUT /v1/signals/settings`, `GET /v1/whoami`, `GET /healthz`.
pub fn router(st: AppState, max_body_bytes: usize) -> Router {
    Router::new()
        .route("/auth/config", get(web_auth::config))
        .route("/auth/login", get(web_auth::login))
        .route("/auth/callback", get(web_auth::callback))
        .route("/auth/logout", post(web_auth::logout))
        .route("/v1/events", post(capture::ingest).get(capture::list))
        .route("/v1/reviews", post(reviews::save).get(reviews::list))
        .route("/v1/turns", post(post_turns).get(get_turns))
        .route("/v1/summary", get(get_summary))
        .route("/v1/whoami", get(get_whoami))
        .route("/v1/allowances", get(allowances::status))
        .route("/v1/sdk/activity", get(sdk_activity::list))
        .route(
            "/v1/sdk/reviews",
            get(sdk_activity::reviews).post(sdk_activity::save_review),
        )
        .route("/v1/datasets", get(datasets::status))
        .route("/v1/datasets/policy", axum::routing::put(datasets::save))
        .route("/v1/datasets/jobs/{id}/artifact", get(datasets::artifact))
        .route(
            "/assets/dashboard-datasets.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard-datasets.js"),
                )
            }),
        )
        .route(
            "/assets/dashboard-sdk.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard-sdk.js"),
                )
            }),
        )
        .route("/v1/signals", get(get_signals))
        .route("/v1/signals/stats", get(analytics::get))
        .route("/v1/signals/preview", post(preview_signal))
        .route(
            "/v1/sources/health",
            get(source_health::list).post(source_health::ingest),
        )
        .route(
            "/v1/signals/settings",
            get(get_signal_settings).put(put_signal_settings),
        )
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/build-info", get(|| async { Json(build_info()) }))
        .route("/", get(public_site))
        .route("/app", get(dashboard))
        .route("/app/", get(dashboard))
        .route(
            "/assets/brand.css",
            get(|State(st): State<AppState>| async move {
                (
                    [
                        ("content-type", "text/css; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                        ("cache-control", "no-store"),
                    ],
                    st.brand.css.clone(),
                )
            }),
        )
        .route(
            "/assets/public.css",
            get(|| async {
                (
                    [
                        ("content-type", "text/css; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("public.css"),
                )
            }),
        )
        .route(
            "/assets/public.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("public.js"),
                )
            }),
        )
        .route(
            "/assets/dashboard.css",
            get(|| async {
                (
                    [
                        ("content-type", "text/css; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard.css"),
                )
            }),
        )
        .route(
            "/assets/dashboard-a2ui.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard-a2ui.js"),
                )
            }),
        )
        .route(
            "/assets/dashboard.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard.js"),
                )
            }),
        )
        .route("/assets/research.json", get(research_notes))
        .layer(axum::middleware::from_fn_with_state(
            st.clone(),
            web_auth::browser_session,
        ))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(st)
}

#[cfg(test)]
mod tests;
