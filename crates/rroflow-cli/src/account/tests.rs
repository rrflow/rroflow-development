//! Against a fake Nomen that plays the device-authorization grant the way Zitadel does.
//! The fake runs on its own Tokio runtime thread; the CLI under test is blocking.

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering::SeqCst},
};

use axum::{
    Form, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

use super::*;

const SUBJECT: &str = "395290028818825218";
const CLIENT: &str = "397185154935750659";
const PROJECT: &str = "397185153912274947";

struct Fake {
    issuer: Mutex<String>,
    /// Token-endpoint answers for the device grant, in order; the last repeats.
    device_answers: Vec<(StatusCode, Value)>,
    polls: AtomicUsize,
    revoked: Mutex<Vec<String>>,
    refreshes: AtomicUsize,
    scopes: Mutex<Vec<String>>,
}

fn fake(device_answers: Vec<(StatusCode, Value)>) -> Arc<Fake> {
    Arc::new(Fake {
        issuer: Mutex::new(String::new()),
        device_answers,
        polls: AtomicUsize::new(0),
        revoked: Mutex::new(Vec::new()),
        refreshes: AtomicUsize::new(0),
        scopes: Mutex::new(Vec::new()),
    })
}

async fn device_authorization(
    State(fake): State<Arc<Fake>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    assert_eq!(form.get("client_id").map(String::as_str), Some(CLIENT));
    fake.scopes
        .lock()
        .unwrap()
        .push(form.get("scope").cloned().unwrap_or_default());
    let issuer = fake.issuer.lock().unwrap().clone();
    axum::Json(json!({
        "device_code": "dc-1",
        "user_code": "ABCD-EFGH",
        "verification_uri": format!("{issuer}/device"),
        "verification_uri_complete": format!("{issuer}/device?user_code=ABCD-EFGH"),
        "expires_in": 600,
        "interval": 0,
    }))
    .into_response()
}

async fn token(
    State(fake): State<Arc<Fake>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let field = |key: &str| form.get(key).map(String::as_str).unwrap_or("");
    assert_eq!(field("client_id"), CLIENT);
    match field("grant_type") {
        DEVICE_GRANT => {
            assert_eq!(field("device_code"), "dc-1");
            let n = fake.polls.fetch_add(1, SeqCst);
            let (status, body) = fake.device_answers[n.min(fake.device_answers.len() - 1)].clone();
            (status, axum::Json(body)).into_response()
        }
        "refresh_token" => {
            fake.refreshes.fetch_add(1, SeqCst);
            if field("refresh_token") != "rt-1" {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error": "invalid_grant"})),
                )
                    .into_response();
            }
            axum::Json(json!({"access_token": "at-2", "refresh_token": "rt-2", "token_type": "Bearer", "expires_in": 7200}))
                .into_response()
        }
        other => panic!("unexpected grant {other}"),
    }
}

async fn userinfo(headers: HeaderMap) -> Response {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if bearer != "Bearer at-1" {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(
        json!({ "sub": SUBJECT, "preferred_username": "hades", "email": "jessay@example.test" }),
    )
    .into_response()
}

async fn revoke(
    State(fake): State<Arc<Fake>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    fake.revoked
        .lock()
        .unwrap()
        .push(form.get("token").cloned().unwrap_or_default());
    StatusCode::OK.into_response()
}

/// Serve the fake on a background runtime; returns its issuer URL.
fn serve(fake: Arc<Fake>) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let issuer = format!("http://{}", listener.local_addr().unwrap());
                *fake.issuer.lock().unwrap() = issuer.clone();
                let app = Router::new()
                    .route("/oauth/v2/device_authorization", post(device_authorization))
                    .route("/oauth/v2/token", post(token))
                    .route("/oauth/v2/revoke", post(revoke))
                    .route("/oidc/v1/userinfo", get(userinfo))
                    .with_state(fake);
                tx.send(issuer).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
    });
    rx.recv().unwrap()
}

fn pending() -> (StatusCode, Value) {
    (
        StatusCode::BAD_REQUEST,
        json!({"error": "authorization_pending"}),
    )
}

fn granted() -> (StatusCode, Value) {
    (
        StatusCode::OK,
        json!({"access_token": "at-1", "refresh_token": "rt-1", "token_type": "Bearer", "expires_in": 3600}),
    )
}

#[test]
fn login_shows_the_code_waits_and_lands_the_account_owner_only() {
    let fake = fake(vec![pending(), granted()]);
    let issuer = serve(fake.clone());
    let nomen = Issuer::new(&issuer, CLIENT, Some(PROJECT)).unwrap();
    let mut told = Vec::new();
    let account = nomen.login("https://ingest.example/", &mut told).unwrap();
    let told = String::from_utf8(told).unwrap();
    assert!(told.contains("ABCD-EFGH"), "{told}");
    assert!(told.contains(&format!("{issuer}/device")), "{told}");
    assert_eq!(fake.polls.load(SeqCst), 2, "pending once, then granted");
    assert!(
        fake.scopes.lock().unwrap()[0]
            .ends_with(&format!("urn:zitadel:iam:org:project:id:{PROJECT}:aud")),
        "the rroflow project is asked for in the audience"
    );
    assert_eq!(account.subject, SUBJECT);
    assert_eq!(account.username.as_deref(), Some("hades"));
    assert_eq!(account.ingest_url, "https://ingest.example");
    assert_eq!(account.access_token(), "at-1");
    assert_eq!(account.standing(unix_now()), Standing::Fresh);
    assert!(
        !format!("{account:?}").contains("at-1"),
        "Debug never shows a token"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join(FILE_NAME);
    write(&path, &account).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{mode:o}");
    }
    assert_eq!(read(&path).unwrap(), Some(account.clone()));
    assert!(!path.with_extension("json.tmp").exists());

    // Refresh keeps the identity and takes the new tokens; revoke names both tokens.
    let refreshed = nomen.refresh(&account).unwrap();
    assert_eq!(refreshed.subject, SUBJECT);
    assert_eq!(refreshed.access_token(), "at-2");
    assert_eq!(refreshed.refresh_token.as_deref(), Some("rt-2"));
    assert!(refreshed.expires_at > account.expires_at);
    nomen.revoke(&account).unwrap();
    assert_eq!(*fake.revoked.lock().unwrap(), vec!["at-1", "rt-1"]);
    assert!(remove(&path).unwrap());
    assert!(!remove(&path).unwrap());
    assert_eq!(read(&path).unwrap(), None);
}

#[test]
fn a_declined_or_expired_code_is_a_refusal_in_plain_words() {
    for (answer, expect) in [
        (json!({"error": "access_denied"}), "declined"),
        (json!({"error": "expired_token"}), "expired"),
        (
            json!({"error": "invalid_client", "error_description": "no such app"}),
            "invalid_client (no such app)",
        ),
    ] {
        let issuer = serve(fake(vec![(StatusCode::BAD_REQUEST, answer)]));
        let nomen = Issuer::new(&issuer, CLIENT, None).unwrap();
        match nomen
            .login(DEFAULT_INGEST_URL, &mut Vec::new())
            .err()
            .unwrap()
        {
            Refusal::Refused(text) => assert!(text.contains(expect), "{text}"),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn an_unreachable_issuer_is_unavailable_not_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let nomen = Issuer::new(&issuer, CLIENT, None).unwrap();
    assert!(matches!(
        nomen.login(DEFAULT_INGEST_URL, &mut Vec::new()),
        Err(Refusal::Unavailable(_))
    ));
    assert!(
        Issuer::new("http://nomen.example", CLIENT, None).is_err(),
        "https only"
    );
    assert!(Issuer::new(DEFAULT_ISSUER, " ", None).is_err());
}

#[test]
fn current_refreshes_when_due_and_keeps_the_file_otherwise() {
    let fake = fake(vec![granted()]);
    let issuer = serve(fake.clone());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    assert_eq!(current(&path).unwrap(), None);

    let now = unix_now();
    let account = Account {
        issuer: issuer.clone(),
        client_id: CLIENT.into(),
        project_id: Some(PROJECT.into()),
        ingest_url: DEFAULT_INGEST_URL.into(),
        subject: SUBJECT.into(),
        username: Some("hades".into()),
        email: None,
        access_token: "at-1".into(),
        refresh_token: Some("rt-1".into()),
        expires_at: now + 60, // within REFRESH_AHEAD
        signed_in_at: now - 3600,
    };
    assert_eq!(account.standing(now), Standing::Refresh);
    write(&path, &account).unwrap();
    let after = current(&path).unwrap().unwrap();
    assert_eq!(after.access_token(), "at-2");
    assert_eq!(fake.refreshes.load(SeqCst), 1);
    assert_eq!(read(&path).unwrap(), Some(after.clone()), "written back");
    assert_eq!(after.standing(unix_now()), Standing::Fresh);
    current(&path).unwrap();
    assert_eq!(fake.refreshes.load(SeqCst), 1, "fresh: no second refresh");

    // A refresh Nomen refuses leaves the file as it was, and the standing says so.
    let stale = Account {
        refresh_token: Some("rt-gone".into()),
        expires_at: now - 10,
        ..account.clone()
    };
    write(&path, &stale).unwrap();
    assert_eq!(current(&path).unwrap(), Some(stale.clone()));
    assert!(matches!(stale.standing(now), Standing::Offline { .. }));
    assert_eq!(
        stale.standing(now + OFFLINE_GRACE.as_secs() + 11),
        Standing::Expired
    );

    std::fs::write(&path, "not json").unwrap();
    assert!(read(&path).unwrap_err().contains("logout"));
}
