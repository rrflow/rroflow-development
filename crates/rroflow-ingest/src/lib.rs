//! rroflow-ingest: the rroflow.ai service that receives `rroflow.turn/v1` records from
//! collectors and files them into the sender's devspace.
//!
//! - **Tenancy:** a devspace is the user's Shippin.cloud space. The ingest key decides the
//!   devspace; a client cannot name another one. Keys are stored only as SHA-256 hashes.
//! - **Idempotence:** records upsert by `event_id`, so re-sending replaces, never duplicates.
//! - **Privacy:** the service never logs record content; content arrives only when the
//!   user opted in on the collector.
//! - **Store:** [`TurnStore`] is the seam. [`JsonlStore`] (append-only JSONL per devspace,
//!   indexed in memory) is the interim implementation until decision D-4 picks the store.

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rroflow_core::{SCHEMA, TurnRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Service configuration (TOML). Every value is configuration; defaults are in [`Config::default`].
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Address to listen on.
    pub listen: String,
    /// Root directory for devspace data.
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
            data_dir: PathBuf::from("data"),
            max_batch: 500,
            max_body_bytes: 16 * 1024 * 1024,
            devspace: vec![],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Devspace {
    /// The Shippin.cloud devspace id this data belongs to.
    pub id: String,
    /// SHA-256 (hex) of each ingest key that writes to this devspace.
    pub key_sha256: Vec<String>,
}

/// SHA-256 of a key, hex-encoded: the only form in which keys are stored.
pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// Characters allowed in a devspace id (it becomes a directory name).
fn valid_devspace_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Counts returned by an upsert.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Upserted {
    pub new: usize,
    pub replaced: usize,
}

/// Where turns live. One implementation per storage decision (D-4).
pub trait TurnStore: Send + Sync {
    fn upsert(&self, devspace: &str, recs: &[TurnRecord]) -> anyhow::Result<Upserted>;
    /// Newest first.
    fn list(&self, devspace: &str, limit: usize, signal: Option<&str>) -> Vec<TurnRecord>;
    fn summary(&self, devspace: &str) -> Summary;
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub turns: usize,
    pub by_agent: BTreeMap<String, usize>,
    pub by_project: BTreeMap<String, usize>,
    pub signals: BTreeMap<String, usize>,
    pub tool_calls: usize,
    pub tool_errors: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    /// Turns whose usage the provider did not report (counted, never priced as zero).
    pub turns_without_usage: usize,
}

/// Append-only JSONL per devspace (`<data_dir>/<devspace>/turns.jsonl`), indexed in memory.
/// The last line for an `event_id` wins, so a re-send replaces the earlier record.
pub struct JsonlStore {
    root: PathBuf,
    index: Mutex<HashMap<String, HashMap<String, TurnRecord>>>,
}

impl JsonlStore {
    /// Open the store and load every devspace's records.
    pub fn open(root: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&root)?;
        let mut index: HashMap<String, HashMap<String, TurnRecord>> = HashMap::new();
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let file = entry.path().join("turns.jsonl");
            if !file.is_file() {
                continue;
            }
            let ds = entry.file_name().to_string_lossy().to_string();
            let map = index.entry(ds).or_default();
            for line in std::io::BufReader::new(std::fs::File::open(&file)?).lines() {
                if let Ok(rec) = serde_json::from_str::<TurnRecord>(&line?) {
                    map.insert(rec.event_id.clone(), rec);
                }
            }
        }
        Ok(Self {
            root,
            index: Mutex::new(index),
        })
    }
}

impl TurnStore for JsonlStore {
    fn upsert(&self, devspace: &str, recs: &[TurnRecord]) -> anyhow::Result<Upserted> {
        let dir = self.root.join(devspace);
        std::fs::create_dir_all(&dir)?;
        let mut index = self.index.lock().expect("store lock");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("turns.jsonl"))?;
        let map = index.entry(devspace.to_string()).or_default();
        let mut out = Upserted::default();
        for rec in recs {
            writeln!(f, "{}", serde_json::to_string(rec)?)?;
            if map.insert(rec.event_id.clone(), rec.clone()).is_some() {
                out.replaced += 1;
            } else {
                out.new += 1;
            }
        }
        f.flush()?;
        Ok(out)
    }

    fn list(&self, devspace: &str, limit: usize, signal: Option<&str>) -> Vec<TurnRecord> {
        let index = self.index.lock().expect("store lock");
        let mut v: Vec<TurnRecord> = index
            .get(devspace)
            .map(|m| {
                m.values()
                    .filter(|r| signal.is_none_or(|s| r.signals.iter().any(|h| h.signal == s)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v.truncate(limit);
        v
    }

    fn summary(&self, devspace: &str) -> Summary {
        let index = self.index.lock().expect("store lock");
        let mut s = Summary::default();
        for r in index.get(devspace).into_iter().flat_map(|m| m.values()) {
            s.turns += 1;
            *s.by_agent.entry(r.agent.clone()).or_default() += 1;
            *s.by_project
                .entry(r.project.clone().unwrap_or_else(|| "(none)".into()))
                .or_default() += 1;
            for h in &r.signals {
                *s.signals.entry(h.signal.clone()).or_default() += 1;
            }
            s.tool_calls += r.tools.len();
            s.tool_errors += r
                .tools
                .iter()
                .filter(|t| t.status == rroflow_core::ToolStatus::Error)
                .count();
            match (r.usage.input_tokens, r.usage.output_tokens) {
                (Some(i), Some(o)) => {
                    s.input_tokens += i;
                    s.output_tokens += o;
                    s.cache_read_tokens += r.usage.cache_read_tokens.unwrap_or(0);
                }
                _ => s.turns_without_usage += 1,
            }
        }
        s
    }
}

/// Shared state: key-hash → devspace, the store, limits.
#[derive(Clone)]
pub struct AppState {
    keys: Arc<HashMap<String, String>>,
    store: Arc<dyn TurnStore>,
    max_batch: usize,
}

impl AppState {
    pub fn new(cfg: &Config, store: Arc<dyn TurnStore>) -> anyhow::Result<Self> {
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

#[derive(Deserialize)]
pub struct TurnsBody {
    pub turns: Vec<TurnRecord>,
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
        if r.schema != SCHEMA {
            return Err(ApiError::BadRequest(format!(
                "turns[{i}]: schema {:?} is not {SCHEMA}",
                r.schema
            )));
        }
        if r.event_id.is_empty() {
            return Err(ApiError::BadRequest(format!(
                "turns[{i}]: event_id is empty"
            )));
        }
    }
    let up = st
        .store
        .upsert(&devspace, &body.turns)
        .map_err(|_| ApiError::Internal)?;
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
    let turns = st.store.list(&devspace, limit, q.signal.as_deref());
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "turns": turns }),
    ))
}

async fn get_summary(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st.devspace(&headers)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "summary": st.store.summary(&devspace),
        "catalogue": rroflow_core::signals::CATALOGUE
            .iter()
            .map(|(id, v, meaning)| serde_json::json!({"signal": id, "rule_version": v, "meaning": meaning}))
            .collect::<Vec<_>>(),
    })))
}

/// The HTTP surface: `POST /v1/turns`, `GET /v1/turns`, `GET /v1/summary`, `GET /healthz`.
pub fn router(st: AppState, max_body_bytes: usize) -> Router {
    Router::new()
        .route("/v1/turns", post(post_turns).get(get_turns))
        .route("/v1/summary", get(get_summary))
        .route("/healthz", get(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use rroflow_core::{Termination, Usage};
    use tower::ServiceExt;

    fn rec(id: &str, signal: Option<&str>) -> TurnRecord {
        TurnRecord {
            schema: SCHEMA.into(),
            event_id: id.into(),
            agent: "codex".into(),
            agent_version: "0.164.0".into(),
            provider: "openai".into(),
            model: Some("m".into()),
            session_id: "s".into(),
            turn_key: id.into(),
            subagent: None,
            prompt_author: None,
            started_at: format!("2026-10-26T00:00:0{}Z", id.len() % 10),
            ended_at: "2026-10-26T00:01:00Z".into(),
            cwd: "/w".into(),
            git_branch: "main".into(),
            project: Some("shippinai".into()),
            usage: Usage {
                input_tokens: Some(100),
                output_tokens: Some(10),
                cache_read_tokens: Some(50),
                cache_write_tokens: None,
                input_includes_cache: true,
            },
            termination: Termination::Completed,
            compacted: false,
            tools: vec![],
            tools_unfinished: 0,
            content: None,
            signals: signal
                .map(|s| {
                    vec![rroflow_core::SignalHit {
                        signal: s.into(),
                        rule_version: 1,
                        severity: rroflow_core::Severity::Warn,
                        summary: "x".into(),
                        score: None,
                        evidence: vec![],
                    }]
                })
                .unwrap_or_default(),
            collector: "test".into(),
        }
    }

    fn app(dir: &std::path::Path) -> Router {
        let cfg = Config {
            devspace: vec![
                Devspace {
                    id: "jesse".into(),
                    key_sha256: vec![hash_key("key-jesse")],
                },
                Devspace {
                    id: "other".into(),
                    key_sha256: vec![hash_key("key-other")],
                },
            ],
            ..Config::default()
        };
        let store = Arc::new(JsonlStore::open(dir.to_path_buf()).unwrap());
        router(AppState::new(&cfg, store).unwrap(), cfg.max_body_bytes)
    }

    async fn call(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let resp = app.clone().oneshot(req).await.unwrap();
        let code = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (code, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    fn post(key: &str, turns: &[TurnRecord]) -> Request<Body> {
        Request::post("/v1/turns")
            .header("authorization", format!("Bearer {key}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({ "turns": turns })).unwrap(),
            ))
            .unwrap()
    }

    fn get(key: &str, uri: &str) -> Request<Body> {
        Request::get(uri)
            .header("authorization", format!("Bearer {key}"))
            .body(Body::empty())
            .unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rroflow-ingest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[tokio::test]
    async fn key_decides_the_devspace_and_resends_replace() {
        let dir = tmp("devspace");
        let app = app(&dir);
        let (code, body) = call(
            &app,
            post(
                "key-jesse",
                &[rec("a", Some("blocked_handoff")), rec("bb", None)],
            ),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            (body["devspace"].as_str(), body["new"].as_u64()),
            (Some("jesse"), Some(2))
        );
        let (_, body) = call(
            &app,
            post("key-jesse", &[rec("a", Some("blocked_handoff"))]),
        )
        .await;
        assert_eq!(body["replaced"], 1);
        // Another devspace sees nothing of jesse's.
        let (_, body) = call(&app, get("key-other", "/v1/turns")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 0);
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?signal=blocked_handoff")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1);
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        assert_eq!(body["summary"]["turns"], 2);
        assert_eq!(body["summary"]["signals"]["blocked_handoff"], 1);
        // Data survives a restart (JSONL is the record; the index is rebuilt).
        let app2 = self::app(&dir);
        let (_, body) = call(&app2, get("key-jesse", "/v1/summary")).await;
        assert_eq!(body["summary"]["turns"], 2);
    }

    #[tokio::test]
    async fn unknown_key_and_wrong_schema_are_refused() {
        let dir = tmp("refuse");
        let app = app(&dir);
        let (code, _) = call(&app, post("nope", &[rec("a", None)])).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let mut bad = rec("a", None);
        bad.schema = "something/v0".into();
        let (code, body) = call(&app, post("key-jesse", &[bad])).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("schema"));
    }

    #[test]
    fn config_rejects_unsafe_ids_and_shared_keys() {
        let store: Arc<dyn TurnStore> = Arc::new(JsonlStore::open(tmp("cfg")).unwrap());
        let bad = Config {
            devspace: vec![Devspace {
                id: "../etc".into(),
                key_sha256: vec![],
            }],
            ..Config::default()
        };
        assert!(AppState::new(&bad, store.clone()).is_err());
        let h = hash_key("k");
        let shared = Config {
            devspace: vec![
                Devspace {
                    id: "a".into(),
                    key_sha256: vec![h.clone()],
                },
                Devspace {
                    id: "b".into(),
                    key_sha256: vec![h],
                },
            ],
            ..Config::default()
        };
        assert!(AppState::new(&shared, store).is_err());
    }
}
