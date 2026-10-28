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

#[tokio::test]
async fn build_identity_and_store_readiness_are_distinct_from_liveness() {
    for backend in backends() {
        let env = Env::new(backend, "readiness");
        let store = Arc::new(env.cfg.open_store().await.unwrap());
        let app = router(
            AppState::new(&env.cfg, store.clone()).unwrap(),
            env.cfg.max_body_bytes,
        );
        assert_eq!(call(&app, get("", "/readyz")).await.0, StatusCode::OK);
        let (status, info) = call(&app, get("", "/build-info")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(info["revision"], build_info().revision);
        assert_eq!(info["migrations"].as_str().unwrap().len(), 64);
        if let Store::Postgres(pg) = &*store {
            pg.pool.close().await;
            assert_eq!(
                call(&app, get("", "/readyz")).await.0,
                StatusCode::SERVICE_UNAVAILABLE
            );
            assert_eq!(call(&app, get("", "/healthz")).await.0, StatusCode::OK);
            assert_eq!(call(&app, get("", "/build-info")).await.0, StatusCode::OK);
        }
    }
}

#[tokio::test]
async fn project_and_session_filters_apply_before_limit_and_remain_tenant_scoped() {
    for backend in backends() {
        let env = Env::new(backend, "navigation");
        let app = env.app().await;
        let mut old = rec("old", None);
        old.project = None;
        old.cwd = "/project-a".into();
        old.session_id = "session-a".into();
        let mut newer = rec("newer", None);
        newer.project = Some("project-b".into());
        newer.session_id = "session-b".into();
        assert_eq!(
            call(&app, post("key-jesse", &[old.clone(), newer])).await.0,
            StatusCode::OK
        );
        let (status, body) = call(
            &app,
            get(
                "key-jesse",
                "/v1/turns?project=%2Fproject-a&session=session-a&limit=1",
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["turns"].as_array().unwrap().len(), 1);
        assert_eq!(body["turns"][0]["event_id"], old.event_id);
        let (_, empty) = call(
            &app,
            get("key-jesse", "/v1/turns?project=project-b&session=session-a"),
        )
        .await;
        assert_eq!(empty["turns"], serde_json::json!([]));
        let (_, other) = call(&app, get("key-other", "/v1/turns?project=%2Fproject-a")).await;
        assert_eq!(other["turns"], serde_json::json!([]));
        let (_, sum) = call(&app, get("key-jesse", "/v1/summary")).await;
        assert_eq!(sum["summary"]["by_project"]["/project-a"], 1);
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
        let mut want = serde_json::json!({
            "disabled": ["mood"],
            "options": { "tool_error_streak": { "min_streak": 5.0 } }
        });
        let (code, _) = call(&app, put("key-jesse", "/v1/signals/settings", want.clone())).await;
        assert_eq!(code, StatusCode::OK, "{b:?}");
        want["revision"] = 1.into();
        want["custom"] = serde_json::json!([]);
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
async fn public_product_entry_is_separate_from_member_data() {
    let app = Env::new(Backend::Jsonl, "public").app().await;
    for (path, needle) in [
        ("/", "See the whole story"),
        ("/assets/public.js", "/auth/config"),
        ("/assets/public.css", ".public-hero"),
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains(needle));
        assert!(!text.contains("/assets/dashboard.js"));
    }
    let (code, config) = call(
        &app,
        Request::get("/auth/config").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        config,
        serde_json::json!({"enabled":false,"member_origin":null})
    );
    let (code, _) = call(
        &app,
        Request::get("/v1/whoami").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_is_served_without_a_key_and_reads_only_with_one() {
    let app = Env::new(Backend::Jsonl, "dash").app().await;
    let resp = app
        .clone()
        .oneshot(Request::get("/app").body(Body::empty()).unwrap())
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
    assert!(
        std::str::from_utf8(&html)
            .unwrap()
            .contains("/assets/dashboard.js")
    );
    for (path, kind, needle) in [
        ("/assets/dashboard.js", "text/javascript", "/v1/summary"),
        ("/assets/dashboard.css", "text/css", "@media"),
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with(kind)
        );
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&bytes).unwrap().contains(needle));
    }
    let (code, _) = call(
        &app,
        Request::get("/v1/summary").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, _) = call(
        &app,
        Request::get("/assets/research.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, notes) = call(&app, get("key-jesse", "/assets/research.json")).await;
    assert_eq!(code, StatusCode::OK);
    assert!(notes.as_array().unwrap().iter().any(|note| note["path"]
        == "docs/research/raindrop/ux-settings.md"
        && !note["text"].as_str().unwrap().is_empty()));
}

#[tokio::test]
async fn turn_pages_keep_timestamp_ties_and_undated_records_without_duplicates() {
    for backend in backends() {
        let app = Env::new(backend, "pages").app().await;
        let mut turns: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| rec(id, None))
            .collect();
        turns[0].started_at = "2026-10-27T00:00:00Z".into();
        turns[1].started_at = "2026-10-26T20:00:00-04:00".into();
        turns[2].started_at = "2026-10-26T20:00:00Z".into();
        turns[3].started_at.clear();
        turns[4].started_at.clear();
        assert_eq!(
            call(&app, post("key-jesse", &turns)).await.0,
            StatusCode::OK
        );
        let mut url = "/v1/turns?limit=1".to_string();
        let mut found = Vec::new();
        for _ in 0..6 {
            let (status, body) = call(&app, get("key-jesse", &url)).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["turns"].as_array().unwrap().len(), 1);
            found.push(body["turns"][0]["event_id"].as_str().unwrap().to_string());
            if let Some(cursor) = body["next_cursor"].as_str() {
                url = format!("/v1/turns?limit=1&cursor={cursor}");
                let (_, other) = call(&app, get("key-other", &url)).await;
                assert_eq!(other["turns"], serde_json::json!([]));
            } else {
                break;
            }
        }
        assert_eq!(found, ["a", "b", "c", "d", "e"]);
    }
}

#[tokio::test]
async fn turn_search_is_literal_date_filtered_and_exact_lookup_is_tenant_scoped() {
    for backend in backends() {
        let app = Env::new(backend, "search").app().await;
        let mut a = rec("a", Some("blocked_handoff"));
        a.model = Some("Model-%_One".into());
        a.started_at = "2026-10-27T00:00:00Z".into();
        let mut b = rec("b", None);
        b.model = Some("other".into());
        b.started_at = "2026-10-20T00:00:00Z".into();
        call(&app, post("key-jesse", &[a, b])).await;
        for query in [
            "search=%25_",
            "search=MODEL-%25_ONE",
            "since=2026-10-26T00%3A00%3A00Z",
            "event=a",
        ] {
            let (status, body) = call(
                &app,
                get("key-jesse", &format!("/v1/turns?{query}&limit=1")),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["turns"][0]["event_id"], "a");
            assert!(body["next_cursor"].is_null());
            let (_, other) = call(&app, get("key-other", &format!("/v1/turns?{query}"))).await;
            assert_eq!(other["turns"], serde_json::json!([]));
        }
        let (_, body) = call(
            &app,
            get("key-jesse", "/v1/turns?search=other&signal=blocked_handoff"),
        )
        .await;
        assert_eq!(body["turns"], serde_json::json!([]));
    }
}

#[tokio::test]
async fn invalid_turn_navigation_is_rejected_instead_of_silently_widening_the_view() {
    let app = Env::new(Backend::Jsonl, "bad-nav").app().await;
    for query in ["cursor=bad", "since=yesterday", "seach=typo"] {
        assert_eq!(
            call(&app, get("key-jesse", &format!("/v1/turns?{query}")))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let long = "x".repeat(201);
    assert_eq!(
        call(&app, get("key-jesse", &format!("/v1/turns?search={long}")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
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

// ---- Nomen sign-in (Postgres only: devspace owners live there) ----

/// A stand-in for Nomen's introspection endpoint: answers per token, or a fixed status.
/// The client assertion itself is checked in `nomen::tests`.
struct FakeNomen {
    issuer: String,
    answers: std::sync::Mutex<HashMap<String, serde_json::Value>>,
    status: std::sync::atomic::AtomicU16,
}

async fn fake_nomen() -> Arc<FakeNomen> {
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

const NOMEN_PROJECT: &str = "rroflow-project";
const NOMEN_CLIENT: &str = "rroflow-cli";

fn signed_in(issuer: &str, sub: &str) -> serde_json::Value {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    serde_json::json!({"active": true, "iss": issuer, "aud": [NOMEN_PROJECT], "client_id": NOMEN_CLIENT,
        "sub": sub, "exp": exp, "username": "jesse@example.test",
        "urn:zitadel:iam:user:resourceowner:id": "300000000000000900"})
}

/// A Postgres-backed service with Nomen sign-in pointed at `fake`.
async fn nomen_app(env: &mut Env, fake: &FakeNomen, dir: &std::path::Path) -> Router {
    let key = crate::test_keys::rsa();
    let path = dir.join("nomen-key.json");
    std::fs::write(
        &path,
        serde_json::json!({"type": "application", "keyId": "k1", "key": key.pem, "clientId": "rroflow-ingest@shippin"})
            .to_string(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    env.cfg.nomen = Some(NomenConfig {
        issuer: fake.issuer.clone(),
        audiences: vec![NOMEN_PROJECT.into()],
        clients: vec![NOMEN_CLIENT.into()],
        key_file: path.to_string_lossy().into(),
    });
    env.app().await
}

/// A Nomen-style numeric account ID unique to this run.
fn sub_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{}",
        300_000_000_000_000_000u128 + (nanos % 99_999_999_999_999_999)
    )
}

#[tokio::test]
async fn nomen_sign_in_opens_the_accounts_own_devspace() {
    if std::env::var(PG_ENV).is_err() {
        eprintln!("{PG_ENV} not set: Nomen sign-in tests skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_nomen().await;
    let (alice, bob) = (sub_id(), sub_id());
    {
        let mut a = fake.answers.lock().unwrap();
        a.insert("tok-alice".into(), signed_in(&fake.issuer, &alice));
        a.insert("tok-bob".into(), signed_in(&fake.issuer, &bob));
        let mut other_client = signed_in(&fake.issuer, &alice);
        other_client["client_id"] = "some-tenant-app".into();
        a.insert("tok-other-client".into(), other_client);
    }
    let mut env = Env::new(Backend::Postgres, "nomen");
    let app = nomen_app(&mut env, &fake, dir.path()).await;

    // First sign-in creates the devspace, named by the account ID.
    let (code, who) = call(&app, get("tok-alice", "/v1/whoami")).await;
    assert_eq!(code, StatusCode::OK, "{who}");
    assert_eq!(who["devspace"].as_str(), Some(alice.as_str()));
    assert_eq!(who["via"], "nomen");
    assert_eq!(who["org"], "300000000000000900");
    let (code, body) = call(&app, post("tok-alice", &[rec("a", Some("mood"))])).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["devspace"].as_str(), Some(alice.as_str()));

    // Each account sees only its own devspace; keys keep working beside sign-in.
    let (_, body) = call(&app, get("tok-bob", "/v1/turns")).await;
    assert_eq!(body["turns"].as_array().unwrap().len(), 0);
    let (_, who) = call(&app, get("key-jesse", "/v1/whoami")).await;
    assert_eq!(
        (who["devspace"].as_str(), who["via"].as_str()),
        (Some(env.jesse.as_str()), Some("key"))
    );

    // Refusals are 401; a token from an app rroflow does not accept is refused.
    for bad in ["tok-unknown", "tok-other-client"] {
        let (code, _) = call(&app, get(bad, "/v1/whoami")).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{bad}");
    }

    // Claim: a key-registered devspace becomes bob's, and bob's sign-in opens it. Bob's
    // devspace from signing in above is empty, so it gives way; alice's holds a turn.
    let Store::Postgres(pg) = env.cfg.open_store().await.unwrap() else {
        unreachable!()
    };
    let bob_space = env.other.clone();
    pg.claim_devspace(&bob_space, &fake.issuer, &bob)
        .await
        .unwrap();
    pg.claim_devspace(&bob_space, &fake.issuer, &bob)
        .await
        .unwrap(); // idempotent
    assert!(
        pg.claim_devspace(&bob_space, &fake.issuer, &alice)
            .await
            .is_err(),
        "already owned"
    );
    assert!(
        pg.claim_devspace(&env.jesse, &fake.issuer, &alice)
            .await
            .is_err(),
        "alice already owns a devspace with turns"
    );
    let fresh = nomen_app(&mut env, &fake, dir.path()).await; // new verifier cache
    let (_, who) = call(&fresh, get("tok-bob", "/v1/whoami")).await;
    assert_eq!(who["devspace"].as_str(), Some(bob_space.as_str()));

    // Nomen down: 503 issuer_unavailable, never 401; keys still work.
    fake.status.store(503, Ordering::SeqCst);
    let down = nomen_app(&mut env, &fake, dir.path()).await;
    let (code, body) = call(&down, get("tok-alice", "/v1/whoami")).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("issuer_unavailable")
    );
    let (code, _) = call(&down, get("key-jesse", "/v1/whoami")).await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn nomen_needs_postgres() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_nomen().await;
    let mut env = Env::new(Backend::Jsonl, "nomen-jsonl");
    env.cfg.nomen = Some(NomenConfig {
        issuer: fake.issuer.clone(),
        audiences: vec![NOMEN_PROJECT.into()],
        clients: vec![NOMEN_CLIENT.into()],
        key_file: dir.path().join("missing.json").to_string_lossy().into(),
    });
    let store = Arc::new(env.cfg.open_store().await.unwrap());
    let err = AppState::new(&env.cfg, store).err().unwrap().to_string();
    assert!(err.contains("database_url"), "{err}");
}

#[test]
fn account_ids_that_are_not_valid_devspace_ids_get_a_digest() {
    assert_eq!(
        crate::pg::owner_devspace_id("https://id", "300000000000000001"),
        "300000000000000001"
    );
    let odd = crate::pg::owner_devspace_id("https://id", "user:with/odd.chars");
    assert!(
        odd.starts_with("n-") && crate::valid_devspace_id(&odd),
        "{odd}"
    );
    assert_ne!(
        odd,
        crate::pg::owner_devspace_id("https://other", "user:with/odd.chars")
    );
}

#[tokio::test]
async fn capture_events_are_immutable_isolated_and_survive_restart() {
    for backend in backends() {
        let env = Env::new(backend, "capture");
        let app = env.app().await;
        let e = serde_json::json!({"schema":"rroflow.capture/v1","event_id":"11111111-1111-4111-8111-111111111111","capture_id":"22222222-2222-4222-8222-222222222222","sequence":0,"agent":"grok","session_id":"s","operation_id":"op","parent_operation_id":null,"observed_at":"2026-10-26T00:00:00Z","kind":"stream.event","native_type":"response.created","payload":null});
        let send = |events: Vec<serde_json::Value>| {
            Request::post("/v1/events")
                .header("authorization", "Bearer key-jesse")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"events":events}).to_string()))
                .unwrap()
        };
        for _ in 0..2 {
            let (code, receipt) = call(&app, send(vec![e.clone()])).await;
            assert_eq!(code, StatusCode::OK, "{receipt}");
            assert_eq!(receipt["accepted_ids"], serde_json::json!([e["event_id"]]));
        }
        let mut conflict = e.clone();
        conflict["kind"] = "call.failed".into();
        assert_eq!(
            call(&app, send(vec![conflict])).await.0,
            StatusCode::CONFLICT
        );
        let mut slot = e.clone();
        slot["event_id"] = "33333333-3333-4333-8333-333333333333".into();
        assert_eq!(call(&app, send(vec![slot])).await.0, StatusCode::CONFLICT);
        let (_, other) = call(&app, get("key-other", "/v1/events")).await;
        assert_eq!(other["events"], serde_json::json!([]));
        drop(app);
        let app = env.app().await;
        let (_, rows) = call(&app, get("key-jesse", "/v1/events?agent=grok")).await;
        assert_eq!(rows["events"].as_array().unwrap().len(), 1);
        assert_eq!(rows["events"][0]["kind"], "stream.event");
        if let Store::Postgres(pg) = env.cfg.open_store().await.unwrap() {
            pg.claim_devspace(&env.jesse, "https://issuer.example.test", "capture-owner")
                .await
                .unwrap();
            let err = pg
                .claim_devspace(&env.other, "https://issuer.example.test", "capture-owner")
                .await
                .unwrap_err();
            assert!(err.to_string().contains("one devspace per account"));
        }
        let mut invalid = e.clone();
        invalid["event_id"] = "../escape".into();
        assert_eq!(
            call(&app, send(vec![invalid])).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn reviews_pin_evidence_reject_stale_and_isolate_owners() {
    for b in backends() {
        let env = Env::new(b, "reviews");
        let app = env.app().await;
        let turn = rec("review-turn", Some("blocked_handoff"));
        call(&app, post("key-jesse", std::slice::from_ref(&turn))).await;
        let (_, readback) = call(&app, get("key-jesse", "/v1/turns")).await;
        let mut review = serde_json::json!({"review_id":"11111111-1111-4111-8111-111111111111","event_id":turn.event_id,"turn_digest":readback["revisions"][&turn.event_id],"signal":"blocked_handoff","rule_version":1,"verdict":"not_match"});
        let request = |key: &str, body: &serde_json::Value| {
            Request::post("/v1/reviews")
                .header("authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap()
        };
        assert_eq!(
            call(&app, request("key-other", &review)).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::OK
        );
        let mut changed = turn.clone();
        changed.tools_unfinished = 1;
        call(&app, post("key-jesse", &[changed])).await;
        // Same receipt is idempotent even after the turn changes.
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::OK
        );
        review["verdict"] = serde_json::json!("match");
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::CONFLICT
        );
        review["review_id"] = serde_json::json!("22222222-2222-4222-8222-222222222222");
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::BAD_REQUEST
        );
        review["review_id"] = serde_json::json!("../../outside");
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::BAD_REQUEST
        );
        drop(app);
        let app = env.app().await;
        let (_, rows) = call(&app, get("key-jesse", "/v1/reviews")).await;
        assert_eq!(rows["reviews"].as_array().unwrap().len(), 1);
        assert_eq!(rows["reviews"][0]["request"]["verdict"], "not_match");
        let (_, other) = call(&app, get("key-other", "/v1/reviews")).await;
        assert!(other["reviews"].as_array().unwrap().is_empty());
    }
}

fn json_post(key: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::post(uri)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn custom_signals_preview_ingest_pause_and_persist_per_owner() {
    for b in backends() {
        let env = Env::new(b, "custom");
        let app = env.app().await;
        let rule = serde_json::json!({"id":"custom_test_phrase","name":"Completion phrase","group":"Quality","enabled":true,"agent":"codex","project":"shippinai","condition":{"kind":"response_contains","text":"all done"}});
        let mut t = rec("first", None);
        t.content = Some(rroflow_core::Content {
            input: "Check this".into(),
            output: "It is ALL DONE now.".into(),
        });
        let missing = rec("missing", None);
        let mut other_agent = t.clone();
        other_agent.event_id = "claude".into();
        other_agent.agent = "claude-code".into();
        assert_eq!(
            call(&app, post("key-jesse", &[t.clone(), missing, other_agent]))
                .await
                .0,
            StatusCode::OK
        );
        let (status, preview) = call(
            &app,
            json_post("key-jesse", "/v1/signals/preview", rule.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(preview["evaluated"], 1);
        assert_eq!(preview["unavailable"], 1);
        assert_eq!(preview["matches"].as_array().unwrap().len(), 1);
        assert_eq!(
            call(&app, json_post("", "/v1/signals/preview", rule.clone()))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(
                &app,
                json_post("key-other", "/v1/signals/preview", rule.clone())
            )
            .await
            .1["scanned"],
            0
        );
        let config = serde_json::json!({"revision":0,"custom":[rule.clone()]});
        let (status, saved) = call(
            &app,
            put("key-jesse", "/v1/signals/settings", config.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["settings"]["revision"], 1);
        // Saving does not silently rewrite the already captured turn.
        assert_eq!(
            call(
                &app,
                get("key-jesse", "/v1/turns?signal=custom_test_phrase")
            )
            .await
            .1["turns"],
            serde_json::json!([])
        );
        t.event_id = "new-arrival".into();
        call(&app, post("key-jesse", &[t.clone()])).await;
        let (_, hits) = call(
            &app,
            get("key-jesse", "/v1/turns?signal=custom_test_phrase"),
        )
        .await;
        assert_eq!(hits["turns"].as_array().unwrap().len(), 1);
        assert_eq!(hits["turns"][0]["signals"][0]["rule_version"], 1);
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", config))
                .await
                .0,
            StatusCode::CONFLICT
        );
        let mut changed = saved["settings"].clone();
        changed["custom"][0]["condition"]["text"] = "different".into();
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", changed))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let mut paused = saved["settings"].clone();
        paused["custom"][0]["enabled"] = false.into();
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", paused))
                .await
                .0,
            StatusCode::OK
        );
        let restarted = env.app().await;
        assert_eq!(
            call(&restarted, get("key-jesse", "/v1/signals/settings"))
                .await
                .1["settings"]["custom"][0]["enabled"],
            false
        );
        t.event_id = "after-pause".into();
        call(&restarted, post("key-jesse", &[t])).await;
        assert_eq!(
            call(
                &restarted,
                get("key-jesse", "/v1/turns?signal=custom_test_phrase")
            )
            .await
            .1["turns"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // Clients cannot inject another owner's/custom namespace findings.
        call(
            &restarted,
            post("key-other", &[rec("forged", Some("custom_test_phrase"))]),
        )
        .await;
        assert_eq!(
            call(
                &restarted,
                get("key-other", "/v1/turns?signal=custom_test_phrase")
            )
            .await
            .1["turns"],
            serde_json::json!([])
        );
        assert_eq!(
            call(&restarted, get("key-other", "/v1/signals/settings"))
                .await
                .1["settings"]["custom"],
            serde_json::json!([])
        );
    }
}

#[tokio::test]
async fn custom_rule_bounds_metadata_and_concurrent_settings_are_checked() {
    use rroflow_core::custom::{CustomSignal, Evaluation};
    for b in backends() {
        let env = Env::new(b, "custom-bounds");
        let app = env.app().await;
        let rule = serde_json::json!({"id":"custom_test_errors","name":"Tool failures","group":"Quality","enabled":true,"condition":{"kind":"tool_errors","at_least":2}});
        let mut t = rec("tools", None);
        t.tools = (0..3)
            .map(|i| rroflow_core::ToolRecord {
                id: format!("tool-{i}"),
                name: "TestRunner".into(),
                status: if i < 2 {
                    rroflow_core::ToolStatus::Error
                } else {
                    rroflow_core::ToolStatus::Ok
                },
                started_at: t.started_at.clone(),
                ended_at: t.ended_at.clone(),
                input: None,
                output: None,
            })
            .collect();
        let parsed: CustomSignal = serde_json::from_value(rule.clone()).unwrap();
        assert!(matches!(parsed.evaluate(&t), Evaluation::Match(_)));
        let mut named = rule.clone();
        named["condition"] = serde_json::json!({"kind":"tool_named","name":"testrunner"});
        assert!(matches!(
            serde_json::from_value::<CustomSignal>(named)
                .unwrap()
                .evaluate(&t),
            Evaluation::Match(_)
        ));
        let mut scoped = parsed.clone();
        scoped.project = Some("different".into());
        assert!(matches!(scoped.evaluate(&t), Evaluation::OutOfScope));
        let mut cfg = serde_json::json!({"revision":0,"custom":[rule.clone()]});
        let (a, b) = tokio::join!(
            call(&app, put("key-jesse", "/v1/signals/settings", cfg.clone())),
            call(&app, put("key-jesse", "/v1/signals/settings", cfg.clone()))
        );
        assert!(
            (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
                || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
        );
        cfg["revision"] = 1.into();
        cfg["custom"] = serde_json::json!([]);
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", cfg))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        for condition in [
            serde_json::json!({"kind":"tool_errors","at_least":0}),
            serde_json::json!({"kind":"prompt_contains","text":""}),
            serde_json::json!({"kind":"response_contains","text":"x".repeat(161)}),
        ] {
            let mut invalid = rule.clone();
            invalid["condition"] = condition;
            assert_eq!(
                call(&app, json_post("key-jesse", "/v1/signals/preview", invalid))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
    }
}

#[tokio::test]
async fn settings_migration_preserves_old_choices_and_refuses_old_writers() {
    if std::env::var(PG_ENV).is_err() {
        return;
    }
    let env = Env::new(Backend::Postgres, "settings-migration");
    let Store::Postgres(pg) = env.cfg.open_store().await.unwrap() else {
        unreachable!()
    };
    let mut tx = pg.pool.begin().await.unwrap();
    // pg_temp shadows the real table on this transaction's connection only.
    sqlx::query("CREATE TEMP TABLE signal_settings (config JSONB NOT NULL) ON COMMIT DROP")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO signal_settings VALUES ($1)")
        .bind(sqlx::types::Json(
            serde_json::json!({"disabled":["mood"],"options":{}}),
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0004_signal_settings_revisions.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let normalized: sqlx::types::Json<serde_json::Value> =
        sqlx::query_scalar("SELECT config FROM signal_settings")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(normalized.0["revision"], 0);
    assert_eq!(normalized.0["custom"], serde_json::json!([]));
    assert_eq!(normalized.0["disabled"], serde_json::json!(["mood"]));
    let error = sqlx::query(
        "INSERT INTO signal_settings VALUES ('{\"disabled\":[],\"options\":{}}'::jsonb)",
    )
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    tx.rollback().await.unwrap();
}

fn source_health_report(sequence: u64) -> serde_json::Value {
    serde_json::json!({"schema":"rroflow.source-health/v1","source_id":"00000000-0000-4000-8000-000000000001","sequence":sequence,"observed_at":"2026-10-27T00:00:00Z","collector_version":"0.3.0","pending_revisions":2,"pending_bytes":128,"journal_bytes":256,"oldest_pending_at":"2026-10-26T23:00:00Z","capture_errors":1,"delivery_ok":false,"signal_settings_revision":2})
}

#[tokio::test]
async fn source_reports_are_owner_scoped_durable_and_replay_never_refreshes_receipt() {
    for backend in backends() {
        let env = Env::new(backend, "source-health");
        let app = env.app().await;
        let uri = "/v1/sources/health";
        let report = source_health_report(1);
        assert_eq!(
            call(&app, json_post("wrong", uri, report.clone())).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (status, receipt) = call(&app, json_post("key-jesse", uri, report.clone())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(receipt["devspace"], env.jesse);
        assert_eq!(receipt["observation"]["report"], report);
        assert_eq!(
            call(&app, json_post("key-jesse", uri, report.clone()))
                .await
                .1,
            receipt
        );
        assert_eq!(
            call(&app, get("key-other", uri)).await.1["sources"],
            serde_json::json!([])
        );
        let restored = env.app().await;
        let listed = call(&restored, get("key-jesse", uri)).await.1;
        assert_eq!(listed["sources"][0], receipt["observation"]);
        assert_eq!(listed["recent_for_seconds"], 120);
        let mut conflict = report.clone();
        conflict["capture_errors"] = 9.into();
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, conflict))
                .await
                .0,
            StatusCode::CONFLICT
        );
        let newer = source_health_report(3);
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, newer.clone()))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, report.clone()))
                .await
                .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(&restored, get("key-jesse", uri)).await.1["sources"][0]["report"],
            newer
        );
        // The same source identity cannot route data across an authenticated owner boundary.
        assert_eq!(
            call(&restored, json_post("key-other", uri, report.clone()))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&restored, get("key-other", uri)).await.1["sources"][0]["report"]["sequence"],
            1
        );
        let (a, b) = tokio::join!(
            call(
                &restored,
                json_post("key-jesse", uri, source_health_report(4))
            ),
            call(
                &restored,
                json_post("key-jesse", uri, source_health_report(5))
            )
        );
        assert!([StatusCode::OK, StatusCode::CONFLICT].contains(&a.0));
        assert_eq!(b.0, StatusCode::OK);
        assert_eq!(
            call(&restored, get("key-jesse", uri)).await.1["sources"][0]["report"]["sequence"],
            5
        );
        for (field, value) in [
            ("source_id", serde_json::json!("../../other")),
            ("sequence", serde_json::json!(0)),
            ("observed_at", serde_json::json!("no time")),
            ("pending_bytes", serde_json::json!(999)),
            ("collector_version", serde_json::json!("unsafe/path")),
        ] {
            let mut invalid = report.clone();
            invalid[field] = value;
            assert_eq!(
                call(&restored, json_post("key-jesse", uri, invalid))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        let mut unknown = report.clone();
        unknown["path"] = serde_json::json!("/private/session");
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, unknown))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[tokio::test]
async fn source_report_storage_is_bounded_without_blocking_existing_sources() {
    for backend in backends() {
        let env = Env::new(backend, "source-capacity");
        let app = env.app().await;
        for n in 0..128 {
            let mut report = source_health_report(1);
            report["source_id"] = format!("00000000-0000-4000-8000-{n:012x}").into();
            assert_eq!(
                call(&app, json_post("key-jesse", "/v1/sources/health", report))
                    .await
                    .0,
                StatusCode::OK
            );
        }
        let mut report = source_health_report(1);
        report["source_id"] = "00000000-0000-4000-8000-ffffffffffff".into();
        assert_eq!(
            call(&app, json_post("key-jesse", "/v1/sources/health", report))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(
                &app,
                json_post("key-jesse", "/v1/sources/health", source_health_report(2))
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&app, get("key-jesse", "/v1/sources/health")).await.1["sources"]
                .as_array()
                .unwrap()
                .len(),
            128
        );
    }
}
