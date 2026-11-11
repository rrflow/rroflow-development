use super::*;
use crate::{AppState, Config, Store};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

mod device_tests;
mod first_run_tests;

const ORIGIN: &str = "https://accounts.example.test";
const PASSWORD: &str = "a long private test phrase";
const NEW_PASSWORD: &str = "a different long test phrase";
const EMAIL: &str = "member@example.test";
struct Fixture {
    admin: PgPool,
    pool: PgPool,
    schema: String,
    cfg: Config,
    _dir: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let Ok(base) = std::env::var("RROFLOW_TEST_DATABASE_URL") else {
            return None;
        };
        let admin = PgPool::connect(&base).await.unwrap();
        let schema = format!("native_fixture_{}", rand::random::<u64>());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
        let mut url = reqwest::Url::parse(&base).unwrap();
        url.query_pairs_mut()
            .append_pair("options", &format!("-csearch_path={schema}"));
        url.query_pairs_mut()
            .append_pair("application_name", &schema);
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("keys.json");
        std::fs::write(
            &key,
            json!({"active":"first","keys":{"first":hex::encode([7u8;32])}}).to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let cfg = Config {
            database_url: Some(url.to_string()),
            accounts: Some(AccountConfig {
                origin: ORIGIN.into(),
                encryption_keys_file: key,
                signup_enabled: true,
                smtp: Some(SmtpConfig {
                    host: "smtp.example.test".into(),
                    port: 587,
                    from: "accounts@example.test".into(),
                    credentials_file: dir.path().join("not-used-by-fixture.json"),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let Store::Postgres(pg) = cfg.open_store().await.unwrap() else {
            unreachable!()
        };
        Some(Self {
            admin,
            pool: pg.pool,
            schema,
            cfg,
            _dir: dir,
        })
    }
    fn accounts(&self) -> Accounts {
        Accounts::new(self.cfg.accounts.as_ref().unwrap(), self.pool.clone()).unwrap()
    }
    async fn app(&self) -> Router {
        let store = Arc::new(self.cfg.open_store().await.unwrap());
        crate::router(
            AppState::new(&self.cfg, store).unwrap(),
            self.cfg.max_body_bytes,
        )
    }
    async fn token(&self, a: &Accounts, email: &str, purpose: Purpose) -> String {
        a.request(email, purpose).await.unwrap();
        self.take_mail(a).await
    }
    async fn take_mail(&self, a: &Accounts) -> String {
        let received = Arc::new(std::sync::Mutex::new(String::new()));
        let copy = received.clone();
        let status = mail::deliver_fixture(a, move |_, _, body| async move {
            *copy.lock().unwrap() = body;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(status.sent, 1);
        let body = received.lock().unwrap().clone();
        let link = body.lines().find(|l| l.starts_with(ORIGIN)).unwrap();
        link.split_once('#').unwrap().1.into()
    }
    async fn register(&self, a: &Accounts, email: &str) -> String {
        let token = self.token(a, email, Purpose::Register).await;
        assert!(a.complete(&token, PASSWORD.into()).await.unwrap());
        a.login(email, PASSWORD.into()).await.unwrap().unwrap()
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
async fn rpc(
    app: &Router,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> (StatusCode, Value, Option<String>) {
    let mut request =
        Request::builder()
            .uri(path)
            .method(if body.is_some() { "POST" } else { "GET" });
    if let Some(c) = cookie {
        request = request.header("cookie", c);
    }
    if let Some(o) = origin {
        request = request
            .header("origin", o)
            .header("x-rroflow-request", "dashboard");
    }
    let request = request
        .header("content-type", "application/json")
        .body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find(|s| s.starts_with(http::COOKIE))
        .map(str::to_string);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cookie,
    )
}

#[tokio::test]
async fn native_http_verifies_mail_before_creation_and_uses_private_restart_surviving_cookies() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let request = json!({"email":EMAIL,"purpose":"register"});
    assert_eq!(
        rpc(
            &app,
            "/auth/native/request",
            Some(request.clone()),
            None,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(
            &app,
            "/auth/native/request",
            Some(request),
            None,
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let token = f.take_mail(&a).await;
    let complete = json!({"token":token,"password":PASSWORD});
    let (first, second) = tokio::join!(
        rpc(
            &app,
            "/auth/native/complete",
            Some(complete.clone()),
            None,
            Some(ORIGIN)
        ),
        rpc(
            &app,
            "/auth/native/complete",
            Some(complete),
            None,
            Some(ORIGIN)
        )
    );
    assert_eq!(
        [first.0, second.0]
            .iter()
            .filter(|&&s| s == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM organizations")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM devspaces")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert!(first.2.is_none() && second.2.is_none());
    let login = rpc(
        &app,
        "/auth/native/login",
        Some(json!({"email":EMAIL,"password":PASSWORD})),
        None,
        Some(ORIGIN),
    )
    .await;
    assert_eq!(login.0, StatusCode::OK);
    assert_eq!(login.1, json!({"message":"Signed in."}));
    let cookie = login.2.unwrap();
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("Secure") && cookie.contains("SameSite=Lax")
    );
    let cookie = cookie.split(';').next().unwrap();
    let restarted = f.app().await;
    assert_eq!(
        rpc(&restarted, "/v1/me", None, Some(cookie), None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        rpc(
            &restarted,
            "/v1/organizations",
            Some(json!({"name":"Test team"})),
            Some(cookie),
            Some("https://wrong.test")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(
            &restarted,
            "/v1/organizations",
            Some(json!({"name":"Test team"})),
            Some(cookie),
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::OK
    );
    let explicit = app
        .clone()
        .oneshot(
            Request::get("/v1/me")
                .header("cookie", cookie)
                .header("authorization", "Bearer rrna_invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(explicit.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        rpc(
            &restarted,
            "/auth/logout",
            Some(json!({})),
            Some(cookie),
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        rpc(&app, "/v1/me", None, Some(cookie), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    drop((a, app, restarted));
    f.close().await;
}

#[tokio::test]
async fn recovery_invalidates_every_old_session_and_link_without_linking_federation() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    // An existing federated identity with the same display string remains a distinct person.
    let Store::Postgres(pg) = f.cfg.open_store().await.unwrap() else {
        unreachable!()
    };
    let other = pg
        .account_project(
            "https://identity.example.test",
            &Subject {
                id: "federated-person".into(),
                name: Some(EMAIL.into()),
                org: None,
                via: "nomen",
            },
        )
        .await
        .unwrap();
    let old = f.register(&a, EMAIL).await;
    let second = a.login(EMAIL, PASSWORD.into()).await.unwrap().unwrap();
    let native = a.subject(&old).await.unwrap().unwrap();
    let member = pg.account_project(ISSUER, &native).await.unwrap();
    assert_ne!(other.user, member.user);
    assert!(
        pg.member_project(ISSUER, &native.id, &other.devspace)
            .await
            .unwrap()
            .is_none()
    );
    let reset = f.token(&a, EMAIL, Purpose::Reset).await;
    let stale = f.token(&a, EMAIL, Purpose::Reset).await;
    assert!(a.complete(&reset, NEW_PASSWORD.into()).await.unwrap());
    assert!(a.subject(&old).await.unwrap().is_none());
    assert!(a.subject(&second).await.unwrap().is_none());
    assert!(!a.complete(&reset, PASSWORD.into()).await.unwrap());
    assert!(!a.complete(&stale, PASSWORD.into()).await.unwrap());
    assert!(a.login(EMAIL, PASSWORD.into()).await.unwrap().is_none());
    assert!(a.login(EMAIL, NEW_PASSWORD.into()).await.unwrap().is_some());
    assert!(
        a.login("unknown@example.test", NEW_PASSWORD.into())
            .await
            .unwrap()
            .is_none()
    );
    drop((a, pg));
    f.close().await;
}

#[tokio::test]
async fn mail_failure_retries_ciphertext_and_tampering_never_delivers_a_link() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    a.request(EMAIL, Purpose::Register).await.unwrap();
    let ciphertext: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM native_mail_outbox")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&ciphertext).contains(ACTION_PREFIX));
    let status = mail::deliver_fixture(&a, |_, _, _| async {
        anyhow::bail!("simulated delivery outage")
    })
    .await
    .unwrap();
    assert_eq!(status.retried, 1);
    let status = mail::deliver_fixture(&a, |_, _, _| async {
        panic!("backoff must prevent delivery")
    })
    .await
    .unwrap();
    assert_eq!(status.sent, 0);
    sqlx::query(
        "UPDATE native_mail_outbox SET next_attempt_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let token = f.take_mail(&a).await;
    assert!(a.complete(&token, PASSWORD.into()).await.unwrap());
    a.request("second@example.test", Purpose::Register)
        .await
        .unwrap();
    sqlx::query("UPDATE native_actions SET email='altered@example.test'")
        .execute(&f.pool)
        .await
        .unwrap();
    let status = mail::deliver_fixture(&a, |_, _, _| async {
        panic!("altered destination must fail authentication")
    })
    .await
    .unwrap();
    assert_eq!(status.retried, 1);
    sqlx::query("UPDATE native_actions SET expires_at=date_trunc('second',clock_timestamp())-interval '1 second'")
        .execute(&f.pool)
        .await
        .unwrap();
    let status =
        mail::deliver_fixture(&a, |_, _, _| async { panic!("expired mail must not send") })
            .await
            .unwrap();
    assert_eq!(status.sent, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM native_mail_outbox")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    drop(a);
    f.close().await;
}

#[tokio::test]
async fn durable_limits_expiry_origin_binding_and_storage_failure_are_enforced() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let cfg = f.cfg.accounts.as_mut().unwrap();
    cfg.email_attempts = 2;
    cfg.sessions_per_user = 1;
    cfg.max_pending_emails = 1;
    let a = f.accounts();
    let b = f.accounts();
    let (x, y, z) = tokio::join!(
        a.allow("login", EMAIL),
        b.allow("login", EMAIL),
        a.allow("login", EMAIL)
    );
    assert_eq!(
        [x.unwrap(), y.unwrap(), z.unwrap()]
            .into_iter()
            .filter(|x| *x)
            .count(),
        2
    );
    let token = f.token(&a, EMAIL, Purpose::Register).await;
    assert!(
        a.request("another@example.test", Purpose::Register)
            .await
            .is_err()
    );
    assert!(a.complete(&token, PASSWORD.into()).await.unwrap());
    let first = a.login(EMAIL, PASSWORD.into()).await.unwrap().unwrap();
    let second = b.login(EMAIL, PASSWORD.into()).await.unwrap().unwrap();
    assert!(a.subject(&first).await.unwrap().is_none());
    assert!(a.subject(&second).await.unwrap().is_some());
    let mut moved = a.cfg.clone();
    moved.origin = "https://different.example.test".into();
    assert!(
        Accounts::new(&moved, f.pool.clone())
            .unwrap()
            .subject(&second)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE native_sessions SET expires_at=clock_timestamp()-interval '1 second'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(a.subject(&second).await.unwrap().is_none());
    let app = f.app().await;
    let token = a.login(EMAIL, PASSWORD.into()).await.unwrap().unwrap();
    // The app owns an independent pool; explicitly close that pool to prove a storage outage isn't 401.
    let store = Arc::new(f.cfg.open_store().await.unwrap());
    let st = AppState::new(&f.cfg, store).unwrap();
    st.accounts.as_ref().unwrap().pool.close().await;
    let down = crate::router(st, f.cfg.max_body_bytes);
    let cookie = format!("{}={token}", http::COOKIE);
    assert_eq!(
        rpc(&down, "/v1/me", None, Some(&cookie), None).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        rpc(
            &down,
            "/auth/logout",
            Some(json!({})),
            Some(&cookie),
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        rpc(&app, "/v1/me", None, Some(&cookie), None).await.0,
        StatusCode::OK
    );
    drop((a, b, app, down));
    f.close().await;
}

#[test]
fn native_inputs_preserve_email_identity_and_reject_unbounded_passwords() {
    assert_eq!(
        normalize_email(" Member+tag@Example.TEST ").as_deref(),
        Some("member+tag@example.test")
    );
    for invalid in [
        "a\r\nb@example.test",
        "Somebody <a@example.test>",
        "a@localhost",
        "ü@example.test",
    ] {
        assert!(normalize_email(invalid).is_none());
    }
    let policy = PasswordPolicy {
        min: 8,
        max: 128,
        recommended_min: 15,
        recommended_max: 18,
    };
    assert!(!policy.accepts("seven77"));
    assert!(policy.accepts("Welcome1"));
    assert!(!policy.accepts(&"a".repeat(129)));
    let strict = PasswordPolicy { min: 15, ..policy };
    assert!(!strict.accepts("Welcome1"));
    assert!(strict.accepts(PASSWORD));
    assert!(within_limits("x"), "signing in never applies the minimum");
    assert!(!AccountConfig::default().signup_enabled);
}

#[tokio::test]
async fn configuration_refuses_unsafe_origins_and_password_work_is_bounded() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    assert!(f.cfg.validate_authentication().is_ok());
    let a = f.accounts();
    let mut cfg = a.cfg.clone();
    cfg.origin = "http://example.test".into();
    assert!(Accounts::new(&cfg, f.pool.clone()).is_err());
    cfg = a.cfg.clone();
    cfg.smtp = None;
    assert!(Accounts::new(&cfg, f.pool.clone()).is_err());
    cfg.signup_enabled = false;
    assert!(Accounts::new(&cfg, f.pool.clone()).is_ok());
    cfg = a.cfg.clone();
    cfg.password_concurrency = 0;
    assert!(Accounts::new(&cfg, f.pool.clone()).is_err());
    let permits = a
        .hashing
        .clone()
        .acquire_many_owned(a.cfg.password_concurrency as u32)
        .await
        .unwrap();
    assert!(a.password_work(PASSWORD.into(), None).await.is_err());
    drop(permits);
    assert!(
        a.password_work(
            PASSWORD.into(),
            Some(a.dummy.replace("m=19456", "m=999999999"))
        )
        .await
        .is_err()
    );
    let app = f.app().await;
    let oversized = app
        .clone()
        .oneshot(
            Request::post("/auth/native/login")
                .header("origin", ORIGIN)
                .header("x-rroflow-request", "dashboard")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"email":EMAIL,"password":"x".repeat(5000)}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    drop((a, app));
    f.close().await;
}

async fn wait_for_lock(f: &Fixture) {
    for _ in 0..200 {
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock')")
            .bind(&f.schema).fetch_one(&f.admin).await.unwrap();
        if blocked {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("expected account operation to wait on the held database lock");
}

#[tokio::test]
async fn expiry_and_password_epoch_are_rechecked_after_lock_waits() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = Arc::new(f.accounts());
    let token = f.token(&a, EMAIL, Purpose::Register).await;
    let mut hold = f.pool.begin().await.unwrap();
    a.email_lock(&mut hold, EMAIL).await.unwrap();
    let task = {
        let a = a.clone();
        let token = token.clone();
        tokio::spawn(async move { a.complete(&token, PASSWORD.into()).await })
    };
    wait_for_lock(&f).await;
    sqlx::query("UPDATE native_actions SET expires_at=date_trunc('second',clock_timestamp())-interval '1 second'")
        .execute(&mut *hold)
        .await
        .unwrap();
    hold.commit().await.unwrap();
    assert!(!task.await.unwrap().unwrap());
    f.register(&a, EMAIL).await;
    let phc = a
        .password_work(NEW_PASSWORD.into(), None)
        .await
        .unwrap()
        .unwrap();
    let mut hold = f.pool.begin().await.unwrap();
    sqlx::query("SELECT user_id FROM native_credentials WHERE email=$1 FOR UPDATE")
        .bind(EMAIL)
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    let task = {
        let a = a.clone();
        tokio::spawn(async move { a.login(EMAIL, PASSWORD.into()).await })
    };
    wait_for_lock(&f).await;
    sqlx::query("UPDATE native_credentials SET password_phc=$2,credential_epoch=credential_epoch+1 WHERE email=$1").bind(EMAIL).bind(phc).execute(&mut *hold).await.unwrap();
    hold.commit().await.unwrap();
    assert!(task.await.unwrap().unwrap().is_none());
    assert!(a.login(EMAIL, NEW_PASSWORD.into()).await.unwrap().is_some());
    drop(a);
    f.close().await;
}

#[tokio::test]
async fn requesting_mail_does_not_hold_expired_rows_while_waiting_for_completion() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = Arc::new(f.accounts());
    a.request(EMAIL, Purpose::Register).await.unwrap();
    sqlx::query("UPDATE native_actions SET expires_at=date_trunc('second',clock_timestamp())-interval '1 second'")
        .execute(&f.pool).await.unwrap();
    // A completion owns the email lock. A simultaneous resend must not hold
    // expired action rows while waiting for that lock: completion deletes them too.
    let mut completion = f.pool.begin().await.unwrap();
    a.email_lock(&mut completion, EMAIL).await.unwrap();
    let resend = {
        let a = a.clone();
        tokio::spawn(async move { a.request(EMAIL, Purpose::Register).await })
    };
    wait_for_lock(&f).await;
    let removed = sqlx::query("DELETE FROM native_actions WHERE email=$1")
        .bind(EMAIL)
        .execute(&mut *completion)
        .await;
    assert!(removed.is_ok(), "completion and resend formed a lock cycle");
    completion.commit().await.unwrap();
    assert!(
        resend.await.unwrap().is_ok(),
        "resend was aborted by a lock cycle"
    );
    drop(a);
    f.close().await;
}
