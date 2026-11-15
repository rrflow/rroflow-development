//! Helpers shared by every feature's HTTP-level tests. Every scenario runs on the JSONL store,
//! and on Postgres too when `RROFLOW_TEST_DATABASE_URL` is set (CI sets it; locally it is
//! skipped with a note). A feature's tests start with `use crate::tests::*;`, which brings in
//! these helpers, the imports below and the crate root.

pub(crate) use super::*;
pub(crate) use axum::Json;
pub(crate) use axum::body::Body;
pub(crate) use axum::extract::State;
pub(crate) use axum::http::{Request, StatusCode};
pub(crate) use axum::response::IntoResponse;
pub(crate) use http_body_util::BodyExt;
pub(crate) use rroflow_core::signals::CATALOGUE;
pub(crate) use rroflow_core::{SCHEMA, Termination, TurnRecord, Usage};
pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::Arc;
pub(crate) use std::sync::atomic::{AtomicU32, Ordering};
pub(crate) use tower::ServiceExt;

pub(crate) const PG_ENV: &str = "RROFLOW_TEST_DATABASE_URL";

pub(crate) fn rec(id: &str, signal: Option<&str>) -> TurnRecord {
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

/// Unique per test and per run, so Postgres tests never see each other's rows.
pub(crate) fn unique(name: &str) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!(
        "{name}-{}-{nanos}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Backend {
    Jsonl,
    Postgres,
}

/// The backends available in this run.
pub(crate) fn backends() -> Vec<Backend> {
    if std::env::var(PG_ENV).is_ok() {
        vec![Backend::Jsonl, Backend::Postgres]
    } else {
        eprintln!("{PG_ENV} not set: Postgres store tests skipped");
        vec![Backend::Jsonl]
    }
}

/// A test service with two devspaces, `<tag>_jesse` and `<tag>_other`.
pub(crate) struct Env {
    pub(crate) cfg: Config,
    pub(crate) jesse: String,
    pub(crate) other: String,
}

impl Env {
    pub(crate) fn new(backend: Backend, tag: &str) -> Self {
        let id = unique(tag);
        let (jesse, other) = (
            format!("{}_jesse", &id[..id.len().min(40)]),
            format!("{}_other", &id[..id.len().min(40)]),
        );
        let cfg = Config {
            database_url: match backend {
                Backend::Jsonl => None,
                Backend::Postgres => Some(std::env::var(PG_ENV).unwrap()),
            },
            db_max_connections: 4,
            data_dir: std::env::temp_dir().join(format!("rroflow-ingest-{id}")),
            devspace: vec![
                Devspace {
                    id: jesse.clone(),
                    key_sha256: vec![hash_key("key-jesse")],
                },
                Devspace {
                    id: other.clone(),
                    key_sha256: vec![hash_key("key-other")],
                },
            ],
            ..Config::default()
        };
        Self { cfg, jesse, other }
    }

    /// A fresh service over the same data (also how a restart is simulated).
    pub(crate) async fn app(&self) -> Router {
        let store = Arc::new(self.cfg.open_store().await.unwrap());
        router(
            AppState::new(&self.cfg, store).unwrap(),
            self.cfg.max_body_bytes,
        )
    }
}

pub(crate) async fn call(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let code = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (code, serde_json::from_slice(&bytes).unwrap_or_default())
}

pub(crate) fn post(key: &str, turns: &[TurnRecord]) -> Request<Body> {
    Request::post("/v1/turns")
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "turns": turns })).unwrap(),
        ))
        .unwrap()
}

pub(crate) fn get(key: &str, uri: &str) -> Request<Body> {
    Request::get(uri)
        .header("authorization", format!("Bearer {key}"))
        .body(Body::empty())
        .unwrap()
}

pub(crate) fn put(key: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::put(uri)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

// ---- Nomen sign-in (Postgres only: devspace owners live there) ----

/// A stand-in for Nomen's introspection endpoint: answers per token, or a fixed status.
/// The client assertion itself is checked in `nomen::tests`.
pub(crate) struct FakeNomen {
    pub(crate) issuer: String,
    pub(crate) answers: std::sync::Mutex<HashMap<String, serde_json::Value>>,
    pub(crate) status: std::sync::atomic::AtomicU16,
}

pub(crate) async fn fake_nomen() -> Arc<FakeNomen> {
    use axum::extract::Form;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fake = Arc::new(FakeNomen {
        issuer: format!("http://{}", listener.local_addr().unwrap()),
        answers: std::sync::Mutex::new(HashMap::new()),
        status: std::sync::atomic::AtomicU16::new(200),
    });
    let app = Router::new()
        .route(
            "/oauth/v2/introspect",
            axum::routing::post(
                |State(f): State<Arc<FakeNomen>>,
                 Form(form): Form<HashMap<String, String>>| async move {
                    let status = f.status.load(Ordering::SeqCst);
                    if status != 200 {
                        return StatusCode::from_u16(status).unwrap().into_response();
                    }
                    let token = form.get("token").cloned().unwrap_or_default();
                    let answer = f.answers.lock().unwrap().get(&token).cloned();
                    Json(answer.unwrap_or(serde_json::json!({"active": false}))).into_response()
                },
            ),
        )
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    fake
}

pub(crate) const NOMEN_PROJECT: &str = "rroflow-project";
pub(crate) const NOMEN_CLIENT: &str = "rroflow-cli";

pub(crate) fn signed_in(issuer: &str, sub: &str) -> serde_json::Value {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    serde_json::json!({"active": true, "iss": issuer, "aud": [NOMEN_PROJECT], "client_id": NOMEN_CLIENT,
        "sub": sub, "exp": exp, "username": "jesse@example.test",
        "urn:zitadel:iam:user:resourceowner:id": "300000000000000900"})
}

pub(crate) fn json_post(key: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::post(uri)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

pub(crate) fn source_health_report(sequence: u64) -> serde_json::Value {
    serde_json::json!({"schema":"rroflow.source-health/v1","source_id":"00000000-0000-4000-8000-000000000001","sequence":sequence,"observed_at":"2026-10-27T00:00:00Z","collector_version":"0.3.0","pending_revisions":2,"pending_bytes":128,"journal_bytes":256,"oldest_pending_at":"2026-10-26T23:00:00Z","capture_errors":1,"delivery_ok":false,"signal_settings_revision":2})
}
