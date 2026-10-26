//! HTTP-level tests. Every scenario runs on the JSONL store, and on Postgres too when
//! `RROFLOW_TEST_DATABASE_URL` is set (CI sets it; locally it is skipped with a note).

use super::*;
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use rroflow_core::{Termination, Usage};
use std::sync::atomic::{AtomicU32, Ordering};
use tower::ServiceExt;

const PG_ENV: &str = "RROFLOW_TEST_DATABASE_URL";

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

/// Unique per test and per run, so Postgres tests never see each other's rows.
fn unique(name: &str) -> String {
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
enum Backend {
    Jsonl,
    Postgres,
}

/// The backends available in this run.
fn backends() -> Vec<Backend> {
    if std::env::var(PG_ENV).is_ok() {
        vec![Backend::Jsonl, Backend::Postgres]
    } else {
        eprintln!("{PG_ENV} not set: Postgres store tests skipped");
        vec![Backend::Jsonl]
    }
}

/// A test service with two devspaces, `<tag>_jesse` and `<tag>_other`.
struct Env {
    cfg: Config,
    jesse: String,
    other: String,
}

impl Env {
    fn new(backend: Backend, tag: &str) -> Self {
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
    async fn app(&self) -> Router {
        let store = Arc::new(self.cfg.open_store().await.unwrap());
        router(
            AppState::new(&self.cfg, store).unwrap(),
            self.cfg.max_body_bytes,
        )
    }
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

fn put(key: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::put(uri)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn key_decides_the_devspace_and_resends_replace() {
    for b in backends() {
        let env = Env::new(b, "devspace");
        let app = env.app().await;
        let (code, body) = call(
            &app,
            post(
                "key-jesse",
                &[rec("a", Some("blocked_handoff")), rec("bb", None)],
            ),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{b:?}: {body}");
        assert_eq!(body["devspace"].as_str(), Some(env.jesse.as_str()), "{b:?}");
        assert_eq!(body["new"], 2, "{b:?}");
        // Re-send with a changed signal: replaced, and the old hit is gone.
        let (_, body) = call(&app, post("key-jesse", &[rec("a", Some("mood"))])).await;
        assert_eq!(
            (body["new"].as_u64(), body["replaced"].as_u64()),
            (Some(0), Some(1)),
            "{b:?}"
        );
        // Another devspace sees nothing of jesse's.
        let (_, body) = call(&app, get("key-other", "/v1/turns")).await;
        assert_eq!(body["devspace"].as_str(), Some(env.other.as_str()));
        assert_eq!(body["turns"].as_array().unwrap().len(), 0, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?signal=mood")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?signal=blocked_handoff")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 0, "{b:?}");
        // Agent filter.
        let mut claude = rec("ccc", None);
        claude.agent = "claude-code".into();
        call(&app, post("key-jesse", &[claude])).await;
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?agent=claude-code")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?agent=codex&signal=mood")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1, "{b:?}");
        // Newest first.
        let (_, body) = call(&app, get("key-jesse", "/v1/turns")).await;
        assert_eq!(body["turns"][0]["event_id"], "ccc", "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        let s = &body["summary"];
        assert_eq!(s["turns"], 3, "{b:?}");
        assert_eq!(s["signals"]["mood"], 1, "{b:?}");
        assert!(s["signals"].get("blocked_handoff").is_none(), "{b:?}");
        assert_eq!(
            (s["input_tokens"].as_u64(), s["cache_read_tokens"].as_u64()),
            (Some(300), Some(150)),
            "{b:?}"
        );
        assert_eq!(s["by_agent"]["codex"], 2, "{b:?}");
        assert_eq!(s["signals_by_agent"]["codex"]["mood"], 1, "{b:?}");
        // Data survives a restart.
        let app2 = env.app().await;
        let (_, body) = call(&app2, get("key-jesse", "/v1/summary")).await;
        assert_eq!(body["summary"]["turns"], 3, "{b:?}");
    }
}

#[tokio::test]
async fn duplicate_event_ids_in_one_batch_keep_the_last() {
    for b in backends() {
        let env = Env::new(b, "dup");
        let app = env.app().await;
        let (code, body) = call(
            &app,
            post(
                "key-jesse",
                &[rec("a", None), rec("a", Some("aborted_turn"))],
            ),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{b:?}: {body}");
        assert_eq!(body["new"], 1, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns")).await;
        let turns = body["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 1, "{b:?}");
        assert_eq!(turns[0]["signals"][0]["signal"], "aborted_turn", "{b:?}");
    }
}

#[tokio::test]
async fn unreported_usage_is_counted_not_priced_as_zero() {
    for b in backends() {
        let env = Env::new(b, "usage");
        let app = env.app().await;
        let mut r = rec("a", None);
        r.usage.input_tokens = None;
        call(&app, post("key-jesse", &[r, rec("bb", None)])).await;
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        let s = &body["summary"];
        assert_eq!(
            (
                s["turns_without_usage"].as_u64(),
                s["input_tokens"].as_u64()
            ),
            (Some(1), Some(100)),
            "{b:?}"
        );
    }
}

#[tokio::test]
async fn signal_settings_round_trip_and_reject_bad_values() {
    for b in backends() {
        let env = Env::new(b, "settings");
        let app = env.app().await;
        let (_, body) = call(&app, get("key-jesse", "/v1/signals/settings")).await;
        assert_eq!(
            body["settings"]["disabled"].as_array().unwrap().len(),
            0,
            "{b:?}"
        );
        let want = serde_json::json!({
            "disabled": ["mood"],
            "options": { "tool_error_streak": { "min_streak": 5.0 } }
        });
        let (code, _) = call(&app, put("key-jesse", "/v1/signals/settings", want.clone())).await;
        assert_eq!(code, StatusCode::OK, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/signals/settings")).await;
        assert_eq!(body["settings"], want, "{b:?}");
        // Settings are per devspace.
        let (_, body) = call(&app, get("key-other", "/v1/signals/settings")).await;
        assert_eq!(
            body["settings"]["disabled"].as_array().unwrap().len(),
            0,
            "{b:?}"
        );
        // The summary carries them.
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        assert_eq!(body["settings"], want, "{b:?}");
        for bad in [
            serde_json::json!({ "disabled": ["nope"] }),
            serde_json::json!({ "options": { "tool_error_streak": { "min_streak": 999 } } }),
        ] {
            let (code, _) = call(&app, put("key-jesse", "/v1/signals/settings", bad)).await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{b:?}");
        }
        let (code, _) = call(
            &app,
            Request::put("/v1/signals/settings")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{b:?}");
    }
}

#[tokio::test]
async fn catalogue_is_public_and_grouped_by_category() {
    let env = Env::new(Backend::Jsonl, "catalogue");
    let app = env.app().await;
    let (code, body) = call(
        &app,
        Request::get("/v1/signals").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["schema"], SIGNALS_SCHEMA);
    let grouped: usize = body["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["signals"].as_array().unwrap().len())
        .sum();
    assert_eq!(grouped, CATALOGUE.len());
    let streak = body["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "tool_error_streak")
        .unwrap();
    assert_eq!(streak["category"], "failure");
    assert_eq!(streak["options"][0]["name"], "min_streak");
}

#[tokio::test]
async fn dashboard_is_served_without_a_key_and_reads_only_with_one() {
    let app = Env::new(Backend::Jsonl, "dash").app().await;
    let resp = app
        .clone()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("connect-src 'self'")
    );
    let html = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(std::str::from_utf8(&html).unwrap().contains("/v1/summary"));
    let (code, _) = call(
        &app,
        Request::get("/v1/summary").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_key_wrong_schema_and_bad_timestamps_are_refused() {
    for b in backends() {
        let app = Env::new(b, "refuse").app().await;
        let (code, _) = call(&app, post("nope", &[rec("a", None)])).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let mut bad = rec("a", None);
        bad.schema = "something/v0".into();
        let (code, body) = call(&app, post("key-jesse", &[bad])).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("schema"));
        let mut bad = rec("a", None);
        bad.started_at = "yesterday".into();
        let (code, body) = call(&app, post("key-jesse", &[bad])).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("started_at"));
    }
}

#[tokio::test]
async fn config_rejects_unsafe_ids_and_shared_keys() {
    let store = Arc::new(
        Env::new(Backend::Jsonl, "cfg")
            .cfg
            .open_store()
            .await
            .unwrap(),
    );
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
