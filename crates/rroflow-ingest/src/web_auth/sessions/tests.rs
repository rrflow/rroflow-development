use super::*;
use crate::web_auth::{self, WebAuth};
use crate::{AppState, Config, Store};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tower::ServiceExt;

const ORIGIN: &str = "https://member.example.test";
const USER: &str = "u_00000000000000000000000000000001";
const CONTEXT: &[&str] = &["session-test", ORIGIN, "issuer", "client", "audience"];

fn key_file(dir: &Path, active: &str, entries: &[(&str, u8)]) -> std::path::PathBuf {
    let path = dir.join(format!("session-{}.json", rand::random::<u64>()));
    let keys: BTreeMap<_, _> = entries
        .iter()
        .map(|(id, byte)| (*id, hex::encode([*byte; 32])))
        .collect();
    std::fs::write(&path, json!({"active":active,"keys":keys}).to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    path
}

struct Fixture {
    admin: PgPool,
    pool: PgPool,
    schema: String,
    cfg: Config,
    fake: Arc<crate::tests::FakeNomen>,
    dir: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let Ok(base) = std::env::var("RROFLOW_TEST_DATABASE_URL") else {
            return None;
        };
        let admin = PgPool::connect(&base).await.unwrap();
        let schema = format!("session_fixture_{}", rand::random::<u64>());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
        let mut url = reqwest::Url::parse(&base).unwrap();
        url.query_pairs_mut()
            .append_pair("options", &format!("-csearch_path={schema}"));
        let dir = tempfile::tempdir().unwrap();
        let fake = crate::tests::fake_nomen().await;
        let key = crate::test_keys::rsa();
        let identity = dir.path().join("identity.json");
        std::fs::write(
            &identity,
            json!({"type":"application","keyId":"fixture","key":key.pem,"clientId":"ingest"})
                .to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&identity, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let cfg = Config {
            database_url: Some(url.to_string()),
            db_max_connections: 6,
            nomen: Some(crate::NomenConfig {
                issuer: fake.issuer.clone(),
                audiences: vec![crate::tests::NOMEN_PROJECT.into()],
                clients: vec![crate::tests::NOMEN_CLIENT.into()],
                key_file: identity.to_string_lossy().into(),
            }),
            ..Default::default()
        };
        let Store::Postgres(pg) = cfg.open_store().await.unwrap() else {
            unreachable!()
        };
        sqlx::query("INSERT INTO users(id) VALUES($1)")
            .bind(USER)
            .execute(&pg.pool)
            .await
            .unwrap();
        Some(Self {
            admin,
            pool: pg.pool,
            schema,
            cfg,
            fake,
            dir,
        })
    }
    fn sessions(&self, path: &Path, context: &[&str], capacity: usize) -> DurableSessions {
        DurableSessions::new(self.pool.clone(), path, context, capacity).unwrap()
    }
    async fn app(&self, path: &Path) -> (Router, Arc<WebAuth>) {
        let store = Arc::new(self.cfg.open_store().await.unwrap());
        let Store::Postgres(pg) = store.as_ref() else {
            unreachable!()
        };
        let durable = DurableSessions::new(pg.pool.clone(), path, CONTEXT, 10).unwrap();
        let mut state = AppState::new(&self.cfg, store).unwrap();
        let web = Arc::new(WebAuth {
            origin: ORIGIN.into(),
            issuer: self.fake.issuer.clone(),
            client_id: "dashboard".into(),
            audience: "audience".into(),
            http: reqwest::Client::new(),
            pending: tokio::sync::Mutex::new(Default::default()),
            sessions: tokio::sync::Mutex::new(Default::default()),
            durable: Some(durable),
            capacity: 10,
            session_ttl: 3600,
        });
        state.web = Some(web.clone());
        (crate::router(state, self.cfg.max_body_bytes), web)
    }
    async fn person(&self, app: &Router, subject: &str) -> (String, String) {
        let token = format!("fixture-token-{subject}");
        self.fake.answers.lock().unwrap().insert(
            token.clone(),
            crate::tests::signed_in(&self.fake.issuer, subject),
        );
        let r = app
            .clone()
            .oneshot(
                Request::get("/v1/whoami")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let value: Value =
            serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap();
        (token, value["user"].as_str().unwrap().into())
    }
    async fn close(self) {
        self.pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .unwrap();
        self.admin.close().await;
    }
}

#[tokio::test]
async fn envelope_survives_reconstruction_rotation_and_rejects_tampering() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let key = key_file(f.dir.path(), "old", &[("old", 1)]);
    let id = web_auth::random();
    let token = "fixture-private-bearer";
    let s = f.sessions(&key, CONTEXT, 10);
    s.put(&id, None, USER, token, 60).await.unwrap();
    let row =
        sqlx::query("SELECT session_sha256,nonce,ciphertext,expires_at FROM browser_sessions")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(row.get::<String, _>("session_sha256"), crate::hash_key(&id));
    let original: Vec<u8> = row.get("ciphertext");
    assert!(!original.windows(token.len()).any(|v| v == token.as_bytes()));
    drop(s);
    let reopened = f.sessions(&key, CONTEXT, 10);
    assert_eq!(reopened.get(&id).await.unwrap().as_deref(), Some(token));
    let other = f.sessions(&key, &["other-deployment"], 10);
    assert!(other.get(&id).await.unwrap().is_none());
    sqlx::query("INSERT INTO browser_sessions SELECT $1,session_sha256,user_id,key_id,nonce,ciphertext,expires_at,created_at FROM browser_sessions WHERE context_sha256=$2")
        .bind(&other.context).bind(&reopened.context).execute(&f.pool).await.unwrap();
    assert!(other.get(&id).await.unwrap().is_none());
    let digest = crate::hash_key(&id);
    let expiry: DateTime<Utc> = row.get("expires_at");
    sqlx::query("UPDATE browser_sessions SET expires_at=expires_at+interval '1 second' WHERE context_sha256=$1 AND session_sha256=$2")
        .bind(&reopened.context).bind(&digest).execute(&f.pool).await.unwrap();
    assert!(reopened.get(&id).await.unwrap().is_none());
    sqlx::query("UPDATE browser_sessions SET expires_at=$1,ciphertext=$2 WHERE context_sha256=$3 AND session_sha256=$4")
        .bind(expiry).bind(vec![0u8;original.len()]).bind(&reopened.context).bind(&digest).execute(&f.pool).await.unwrap();
    assert!(reopened.get(&id).await.unwrap().is_none());
    sqlx::query(
        "UPDATE browser_sessions SET ciphertext=$1 WHERE context_sha256=$2 AND session_sha256=$3",
    )
    .bind(original)
    .bind(&reopened.context)
    .bind(&digest)
    .execute(&f.pool)
    .await
    .unwrap();
    let rotated = key_file(f.dir.path(), "new", &[("old", 1), ("new", 2)]);
    let rotated = f.sessions(&rotated, CONTEXT, 10);
    assert_eq!(rotated.get(&id).await.unwrap().as_deref(), Some(token));
    let new_id = web_auth::random();
    rotated
        .put(&new_id, None, USER, "new-token", 60)
        .await
        .unwrap();
    assert!(reopened.get(&new_id).await.unwrap().is_none());
    let retired = key_file(f.dir.path(), "new", &[("new", 2)]);
    let retired = f.sessions(&retired, CONTEXT, 10);
    assert!(retired.get(&id).await.unwrap().is_none());
    assert_eq!(
        retired.get(&new_id).await.unwrap().as_deref(),
        Some("new-token")
    );
    rotated.remove(&new_id).await.unwrap();
    assert!(retired.get(&new_id).await.unwrap().is_none());
    f.close().await;
}

#[tokio::test]
async fn capacity_is_atomic_and_failed_replacement_preserves_old_session() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let key = key_file(f.dir.path(), "one", &[("one", 1)]);
    let s = f.sessions(&key, CONTEXT, 2);
    let a = web_auth::random();
    s.put(&a, None, USER, "a", 60).await.unwrap();
    let b = web_auth::random();
    let c = web_auth::random();
    let (b_result, c_result) = tokio::join!(
        s.put(&b, None, USER, "b", 60),
        s.put(&c, None, USER, "c", 60)
    );
    assert_eq!(
        usize::from(b_result.is_ok()) + usize::from(c_result.is_ok()),
        1
    );
    let replacement = web_auth::random();
    s.put(&replacement, Some(&a), USER, "replacement", 60)
        .await
        .unwrap();
    assert!(s.get(&a).await.unwrap().is_none());
    assert!(
        s.put(
            &web_auth::random(),
            Some(&replacement),
            "u_00000000000000000000000000000099",
            "bad",
            60
        )
        .await
        .is_err()
    );
    assert_eq!(
        s.get(&replacement).await.unwrap().as_deref(),
        Some("replacement")
    );
    sqlx::query("UPDATE browser_sessions SET expires_at=date_trunc('second',clock_timestamp())-interval '1 second' WHERE session_sha256=$1")
        .bind(crate::hash_key(&replacement)).execute(&f.pool).await.unwrap();
    assert!(s.get(&replacement).await.unwrap().is_none());
    s.put(&web_auth::random(), None, USER, "after-expiry", 60)
        .await
        .unwrap();
    f.close().await;
}

fn request(path: &str, id: &str, method: &str) -> axum::http::request::Builder {
    Request::builder()
        .uri(path)
        .method(method)
        .header("cookie", format!("{}={id}", web_auth::SESSION))
}

#[tokio::test]
async fn fresh_instances_share_logout_and_preserve_csrf_and_bearer_authority() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let key = key_file(f.dir.path(), "one", &[("one", 1)]);
    let (first, web) = f.app(&key).await;
    let response = first
        .clone()
        .oneshot(Request::get("/auth/config").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let public: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        public,
        json!({"enabled":true,"member_origin":ORIGIN,"session_storage":"postgres"})
    );
    let (token, user) = f.person(&first, "300000000000000801").await;
    let (other_token, other_user) = f.person(&first, "300000000000000802").await;
    let id = web_auth::random();
    web.durable
        .as_ref()
        .unwrap()
        .put(&id, None, &user, &token, 60)
        .await
        .unwrap();
    drop(first);
    drop(web);
    let (second, web) = f.app(&key).await;
    let r = second
        .clone()
        .oneshot(
            request("/v1/whoami", &id, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let value: Value =
        serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(value["user"], user);
    let r = second
        .clone()
        .oneshot(
            request("/v1/whoami", &id, "GET")
                .header("authorization", format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let value: Value =
        serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(value["user"], other_user);
    let r = second
        .clone()
        .oneshot(
            request("/v1/organizations", &id, "POST")
                .header("origin", "https://other.example")
                .header("x-rroflow-request", "dashboard")
                .header("content-type", "application/json")
                .body(Body::from("{\"name\":\"must not create\"}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let (third, _) = f.app(&key).await;
    let r = second
        .clone()
        .oneshot(
            request("/auth/logout", &id, "POST")
                .header("origin", ORIGIN)
                .header("x-rroflow-request", "dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let r = third
        .oneshot(
            request("/v1/whoami", &id, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    web.durable.as_ref().unwrap().pool.close().await;
    for (path, method) in [("/v1/whoami", "GET"), ("/auth/logout", "POST")] {
        let r = second
            .clone()
            .oneshot(
                request(path, &id, method)
                    .header("origin", ORIGIN)
                    .header("x-rroflow-request", "dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(r.headers()["cache-control"], "no-store");
        assert!(!r.headers().contains_key("set-cookie"));
    }
    drop(second);
    drop(web);
    f.close().await;
}

#[tokio::test]
async fn key_configuration_is_bounded_and_never_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@localhost/unused")
        .unwrap();
    let path = key_file(dir.path(), "missing", &[("one", 1)]);
    assert!(DurableSessions::new(pool.clone(), &path, CONTEXT, 2).is_err());
    let path = key_file(dir.path(), "one", &[("one", 1)]);
    assert!(DurableSessions::new(pool.clone(), &path, CONTEXT, 2).is_ok());
    let mut cfg = Config {
        browser: Some(web_auth::BrowserConfig {
            origin: ORIGIN.into(),
            client_id: "dashboard".into(),
            session_keys_file: Some(path.clone()),
            session_capacity: 2,
            session_ttl_seconds: 600,
        }),
        nomen: Some(crate::NomenConfig {
            issuer: "https://identity.example.test".into(),
            audiences: vec!["project".into()],
            clients: vec!["dashboard".into()],
            key_file: "not-read-by-browser-config".into(),
        }),
        ..Default::default()
    };
    assert!(WebAuth::new(&cfg, None).is_err());
    assert!(
        WebAuth::new(&cfg, Some(&pool))
            .unwrap()
            .unwrap()
            .durable
            .is_some()
    );
    cfg.browser.as_mut().unwrap().session_capacity = 0;
    assert!(WebAuth::new(&cfg, Some(&pool)).is_err());
    cfg.browser.as_mut().unwrap().session_capacity = 2;
    cfg.browser.as_mut().unwrap().session_ttl_seconds = 86401;
    assert!(WebAuth::new(&cfg, Some(&pool)).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(DurableSessions::new(pool.clone(), &path, CONTEXT, 2).is_err());
    }
    assert!(!valid_cookie("injected"));
    assert!(valid_cookie(&web_auth::random()));
    pool.close().await;
}

#[tokio::test]
async fn oauth_callback_issues_only_an_opaque_cookie_and_persists_verified_member() {
    use axum::{
        extract::{Form, Query, State},
        routing::post,
    };
    use std::collections::HashMap;
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let key = key_file(f.dir.path(), "one", &[("one", 1)]);
    f.cfg.accounts = Some(crate::AccountConfig {
        origin: ORIGIN.into(),
        encryption_keys_file: key.clone(),
        ..Default::default()
    });
    let native =
        crate::accounts::Accounts::new(f.cfg.accounts.as_ref().unwrap(), f.pool.clone()).unwrap();
    use argon2::password_hash::PasswordHasher;
    let phc = argon2::Argon2::default()
        .hash_password(b"a long switching test password")
        .unwrap()
        .to_string();
    sqlx::query("INSERT INTO native_credentials(user_id,email,password_phc) VALUES($1,'switch@example.test',$2)").bind(USER).bind(phc).execute(&f.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO user_identities(issuer,subject,user_id) VALUES($1,'switch-fixture',$2)",
    )
    .bind(crate::accounts::ISSUER)
    .bind(USER)
    .execute(&f.pool)
    .await
    .unwrap();
    let native_token = native
        .login(
            "switch@example.test",
            "a long switching test password".into(),
        )
        .await
        .unwrap()
        .unwrap();
    let (app, _) = f.app(&key).await;
    let (token, user) = f.person(&app, "300000000000000803").await;
    drop(app);
    let response_token = token.clone();
    let provider = Router::new().route(
        "/oauth/v2/token",
        post(move |Form(form): Form<HashMap<String, String>>| {
            let token = response_token.clone();
            async move {
                assert_eq!(form["code_verifier"], "fixture-verifier");
                axum::Json(json!({"access_token":token,"token_type":"Bearer","expires_in":600}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, provider).await.unwrap();
    });
    let store = Arc::new(f.cfg.open_store().await.unwrap());
    let mut state = AppState::new(&f.cfg, store).unwrap();
    let web = Arc::new(WebAuth {
        origin: ORIGIN.into(),
        issuer,
        client_id: "dashboard".into(),
        audience: "audience".into(),
        http: reqwest::Client::new(),
        pending: tokio::sync::Mutex::new(Default::default()),
        sessions: tokio::sync::Mutex::new(Default::default()),
        durable: Some(f.sessions(&key, CONTEXT, 10)),
        capacity: 10,
        session_ttl: 300,
    });
    web.pending.lock().await.insert(
        "fixture-state".into(),
        web_auth::Pending {
            binding: "fixture-browser".into(),
            verifier: "fixture-verifier".into(),
            expires: std::time::Instant::now() + web_auth::LOGIN_TTL,
        },
    );
    state.web = Some(web.clone());
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "cookie",
        format!(
            "{}=fixture-browser; {}={native_token}",
            web_auth::LOGIN,
            crate::accounts::http::COOKIE
        )
        .parse()
        .unwrap(),
    );
    let response = web_auth::callback(
        State(state),
        headers,
        Query(web_auth::Callback {
            state: Some("fixture-state".into()),
            code: Some("fixture-code".into()),
            error: None,
        }),
    )
    .await;
    assert_eq!(response.headers()["location"], "/app");
    assert!(native.subject(&native_token).await.unwrap().is_none());
    assert!(response.headers().get_all("set-cookie").iter().any(|v| {
        v.to_str().unwrap().starts_with("__Host-rroflow-native=;")
            && v.to_str().unwrap().contains("Max-Age=0")
    }));
    drop(native);
    let session_cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|v| v.starts_with(&format!("{}=", web_auth::SESSION)))
        .unwrap();
    assert!(session_cookie.contains("Max-Age=300"));
    assert!(!session_cookie.contains(&token));
    let id = session_cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1;
    assert!(valid_cookie(id));
    assert!(web.sessions.lock().await.is_empty());
    assert_eq!(
        web.durable.as_ref().unwrap().get(id).await.unwrap(),
        Some(token.clone())
    );
    let stored_user: String =
        sqlx::query_scalar("SELECT user_id FROM browser_sessions WHERE session_sha256=$1")
            .bind(crate::hash_key(id))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(stored_user, user);
    // A newly started verifier still rejects a provider-revoked credential.
    f.fake
        .answers
        .lock()
        .unwrap()
        .insert(token, json!({"active":false}));
    let (restarted, _) = f.app(&key).await;
    let r = restarted
        .oneshot(
            request("/v1/whoami", id, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    server.abort();
    drop(web);
    f.close().await;
}
