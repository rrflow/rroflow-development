use super::*;
use crate::{Config, Devspace, hash_key, router};
use axum::{Router, body::Body, extract::Form, http::Request, routing::post};
use http_body_util::BodyExt;
use tower::ServiceExt;

const ORIGIN: &str = "https://app.example.test";

async fn app() -> (Router, Arc<WebAuth>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config {
        data_dir: dir.path().into(),
        devspace: vec![
            Devspace {
                id: "alice".into(),
                key_sha256: vec![hash_key("alice-key")],
            },
            Devspace {
                id: "bob".into(),
                key_sha256: vec![hash_key("bob-key")],
            },
        ],
        ..Default::default()
    };
    let store = Arc::new(cfg.open_store().await.unwrap());
    let mut st = AppState::new(&cfg, store).unwrap();
    let web = Arc::new(WebAuth {
        origin: ORIGIN.into(),
        issuer: "https://identity.example.test".into(),
        client_id: "dashboard".into(),
        audience: "rroflow".into(),
        http: reqwest::Client::new(),
        pending: Mutex::new(HashMap::new()),
        sessions: Mutex::new(HashMap::new()),
    });
    st.web = Some(web.clone());
    (router(st, cfg.max_body_bytes), web, dir)
}

async fn body(r: Response) -> serde_json::Value {
    serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap()
}
fn request(method: &str, path: &str, cookie_value: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header("cookie", cookie_value)
}

#[test]
fn pkce_matches_rfc7636_vector_and_config_rejects_insecure_origins() {
    assert_eq!(
        challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    for bad in [
        "http://app.example.test",
        "https://user:pass@example.test",
        "https://example.test/path",
        "https://example.test/?query",
        "https://example.test/#fragment",
    ] {
        assert!(!https_origin(bad), "{bad}");
    }
    assert!(https_origin(ORIGIN));
    let mut cfg = Config {
        browser: Some(BrowserConfig {
            origin: ORIGIN.into(),
            client_id: "dashboard".into(),
        }),
        ..Default::default()
    };
    assert!(WebAuth::new(&cfg).is_err());
    cfg.nomen = Some(crate::NomenConfig {
        issuer: "https://identity.example.test".into(),
        audiences: vec!["project".into()],
        clients: vec!["cli".into()],
        key_file: "not-read-here".into(),
    });
    assert!(WebAuth::new(&cfg).is_err());
    cfg.nomen.as_mut().unwrap().clients.push("dashboard".into());
    assert!(WebAuth::new(&cfg).unwrap().is_some());
}

#[tokio::test]
async fn login_binds_state_and_callback_to_the_browser() {
    let (app, web, _dir) = app().await;
    let r = app
        .clone()
        .oneshot(Request::get("/auth/login").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::SEE_OTHER);
    assert_eq!(r.headers()["cache-control"], "no-store");
    let c = r.headers()["set-cookie"].to_str().unwrap();
    assert!(c.contains("Secure; HttpOnly; SameSite=Lax"));
    assert!(!c.contains("Domain="));
    let cookie = c.split(';').next().unwrap().to_string();
    let url = reqwest::Url::parse(r.headers()["location"].to_str().unwrap()).unwrap();
    let q: HashMap<_, _> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(q["redirect_uri"], format!("{ORIGIN}/auth/callback"));
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        q["scope"],
        "openid profile urn:zitadel:iam:org:project:id:rroflow:aud"
    );
    let state = &q["state"];
    assert_eq!(
        q["code_challenge"],
        challenge(&web.pending.lock().await[state].verifier)
    );
    let path = format!("/auth/callback?state={state}&error=access_denied");
    // The wrong browser cannot consume the transaction.
    let r = app
        .clone()
        .oneshot(Request::get(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.headers()["location"], "/?signin=failed");
    assert!(web.pending.lock().await.contains_key(state));
    // A provider refusal consumes the correctly bound transaction exactly once.
    let r = app
        .oneshot(request("GET", &path, &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.headers()["location"], "/?signin=failed");
    assert!(!web.pending.lock().await.contains_key(state));
    assert!(web.sessions.lock().await.is_empty());
}

#[tokio::test]
async fn sessions_isolate_accounts_block_csrf_expire_and_logout() {
    let (app, web, _dir) = app().await;
    for (id, token) in [("a", "alice-key"), ("b", "bob-key")] {
        web.sessions.lock().await.insert(
            id.into(),
            Session {
                token: token.into(),
                expires: Instant::now() + Duration::from_secs(60),
            },
        );
    }
    let alice = format!("{SESSION}=a");
    for (id, expected) in [("a", "alice"), ("b", "bob")] {
        let r = app
            .clone()
            .oneshot(
                request("GET", "/v1/whoami", &format!("{SESSION}={id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(body(r).await["devspace"], expected);
    }
    for origin in ["https://evil.example", "null", ""] {
        let r = app
            .clone()
            .oneshot(
                request("PUT", "/v1/signals/settings", &alice)
                    .header("origin", origin)
                    .header("x-rroflow-request", "dashboard")
                    .header("content-type", "application/json")
                    .body(Body::from("{\"disabled\":[],\"options\":{}}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }
    let r = app
        .clone()
        .oneshot(
            request("PUT", "/v1/signals/settings", &alice)
                .header("origin", ORIGIN)
                .header("x-rroflow-request", "dashboard")
                .header("content-type", "application/json")
                .body(Body::from("{\"disabled\":[],\"options\":{}}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = app
        .clone()
        .oneshot(
            request("POST", "/auth/logout", &alice)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let r = app
        .clone()
        .oneshot(
            request("POST", "/auth/logout", &alice)
                .header("origin", ORIGIN)
                .header("x-rroflow-request", "dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    assert!(!web.sessions.lock().await.contains_key("a"));
    let r = app
        .clone()
        .oneshot(
            request("GET", "/v1/whoami", &alice)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    web.sessions.lock().await.get_mut("b").unwrap().expires = Instant::now();
    let r = app
        .clone()
        .oneshot(
            request("GET", "/v1/whoami", &format!("{SESSION}=b"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    // A collector's explicit bearer is not overridden by a browser cookie.
    let r = app
        .oneshot(
            request("GET", "/v1/whoami", &alice)
                .header("authorization", "Bearer bob-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body(r).await["devspace"], "bob");
}

#[tokio::test]
async fn code_exchange_is_single_use_and_never_returns_tokens_to_browser() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config {
        data_dir: dir.path().into(),
        devspace: vec![Devspace {
            id: "alice".into(),
            key_sha256: vec![hash_key("fixture-access-token")],
        }],
        ..Default::default()
    };
    let mut st = AppState::new(&cfg, Arc::new(cfg.open_store().await.unwrap())).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let issuer_app = Router::new().route("/oauth/v2/token", post(|Form(f): Form<HashMap<String,String>>| async move {
        assert_eq!(f["grant_type"], "authorization_code");
        assert_eq!(f["code_verifier"], "fixture-verifier");
        assert_eq!(f["redirect_uri"], format!("{ORIGIN}/auth/callback"));
        assert_eq!(f["client_id"], "dashboard");
        Json(serde_json::json!({"access_token": "fixture-access-token", "token_type": "Bearer", "expires_in": 600}))
    }));
    let server = tokio::spawn(async move { axum::serve(listener, issuer_app).await.unwrap() });
    // The fixture uses a known ingest credential to isolate OAuth transport from
    // Nomen introspection (the latter has its own HTTP and Postgres tests).
    let web = Arc::new(WebAuth {
        origin: ORIGIN.into(),
        issuer,
        client_id: "dashboard".into(),
        audience: "project".into(),
        http: reqwest::Client::new(),
        pending: Mutex::new(HashMap::new()),
        sessions: Mutex::new(HashMap::new()),
    });
    web.pending.lock().await.insert(
        "state".into(),
        Pending {
            binding: "browser".into(),
            verifier: "fixture-verifier".into(),
            expires: Instant::now() + LOGIN_TTL,
        },
    );
    st.web = Some(web.clone());
    let app = router(st, cfg.max_body_bytes);
    let path = "/auth/callback?code=fixture-code&state=state";
    let r = app
        .clone()
        .oneshot(
            request("GET", path, &format!("{LOGIN}=browser"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.headers()["location"], "/");
    let cookies: Vec<_> = r
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(cookies.iter().all(|v| !v.contains("fixture-access-token")));
    let session = cookies
        .iter()
        .find(|v| v.starts_with(SESSION))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let who = app
        .clone()
        .oneshot(
            request("GET", "/v1/whoami", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body(who).await["devspace"], "alice");
    let replay = app
        .oneshot(
            request("GET", path, &format!("{LOGIN}=browser"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.headers()["location"], "/?signin=failed");
    assert_eq!(web.sessions.lock().await.len(), 1);
    server.abort();
}
