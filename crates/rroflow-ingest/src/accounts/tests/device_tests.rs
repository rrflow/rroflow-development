use super::*;
use crate::accounts::device::{Authorization, Outcome, Tokens};
const CLIENT: &str = "fixture-collector";

#[test]
fn device_resource_origins_are_explicit_and_bounded() {
    for resource in [
        "https://api.example.test",
        "http://127.0.0.1:1234",
        "http://[::1]:1234",
    ] {
        let cfg = DeviceConfig {
            resource: resource.into(),
            clients: vec![DeviceClient {
                id: CLIENT.into(),
                name: "Fixture".into(),
            }],
            ..Default::default()
        };
        cfg.validate().unwrap();
    }
    for resource in [
        "http://api.example.test",
        "http://127.0.0.1.evil.test",
        "https://api.example.test/path",
        "https://api.example.test?key=value",
        "https://user:secret@api.example.test",
        "https://api.example.test/#code",
        "https://api.example.test/",
    ] {
        let cfg = DeviceConfig {
            resource: resource.into(),
            clients: vec![DeviceClient {
                id: CLIENT.into(),
                name: "Fixture".into(),
            }],
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "{resource}");
    }
}
fn value<T>(result: Result<Outcome<T>>) -> T {
    match result.unwrap() {
        Outcome::Value(v) => v,
        Outcome::Error(e) => panic!("unexpected device refusal: {e}"),
    }
}
fn refused<T>(result: Result<Outcome<T>>, expected: &str) {
    match result.unwrap() {
        Outcome::Error(e) => assert_eq!(e, expected),
        Outcome::Value(_) => panic!("expected refusal {expected}"),
    }
}
async fn fixture() -> Option<Fixture> {
    let mut f = Fixture::new().await?;
    f.cfg.accounts.as_mut().unwrap().device = Some(DeviceConfig {
        resource: ORIGIN.into(),
        clients: vec![DeviceClient {
            id: CLIENT.into(),
            name: "Fixture collector".into(),
        }],
        ..Default::default()
    });
    Some(f)
}
async fn project(f: &Fixture, a: &Accounts, session: &str) -> String {
    let subject = a.subject(session).await.unwrap().unwrap();
    sqlx::query_scalar("SELECT id FROM devspaces WHERE owner_issuer=$1 AND owner_subject=$2")
        .bind(ISSUER)
        .bind(subject.id)
        .fetch_one(&f.pool)
        .await
        .unwrap()
}
async fn due(f: &Fixture, device: &str) {
    sqlx::query("UPDATE native_device_requests SET next_poll_at=clock_timestamp()-interval '1 second' WHERE device_sha256=$1").bind(hash_key(device)).execute(&f.pool).await.unwrap();
}
async fn connect(
    f: &Fixture,
    a: &Accounts,
    session: &str,
    project: &str,
) -> (Authorization, Tokens, String) {
    let start = value(a.device_start(CLIENT, ORIGIN, "Fixture laptop").await);
    value(
        a.device_decide(session, &start.user_code, project, true)
            .await,
    );
    due(f, &start.device_code).await;
    let request = hex::encode(rand::random::<[u8; 32]>());
    let token = value(
        a.device_exchange(CLIENT, &start.device_code, &request)
            .await,
    );
    (start, token, request)
}
async fn bearer(
    app: &Router,
    path: &str,
    method: &str,
    token: &str,
    project: Option<&str>,
) -> StatusCode {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"));
    if let Some(p) = project {
        request = request.header("x-rroflow-project", p);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn device_polling_consent_and_one_project_capability_are_enforced() {
    let Some(f) = fixture().await else {
        return;
    };
    let a = f.accounts();
    refused(
        a.device_start("unregistered", ORIGIN, "Laptop").await,
        "invalid_client",
    );
    refused(
        a.device_start(CLIENT, "https://other.example.test", "Laptop")
            .await,
        "invalid_request",
    );
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let start = value(a.device_start(CLIENT, ORIGIN, "Laptop").await);
    let request = "a".repeat(64);
    refused(
        a.device_exchange(CLIENT, &start.device_code, &request)
            .await,
        "slow_down",
    );
    due(&f, &start.device_code).await;
    refused(
        a.device_exchange(CLIENT, &start.device_code, &request)
            .await,
        "authorization_pending",
    );
    let info = value(
        a.device_inspect(&session, &start.user_code.to_ascii_lowercase())
            .await,
    );
    assert_eq!(info.client_name, "Fixture collector");
    assert_eq!(info.account_email, EMAIL);
    assert_eq!(info.scope, "capture");
    refused(
        a.device_decide(&session, &start.user_code, "missing", true)
            .await,
        "access_denied",
    );
    value(
        a.device_decide(&session, &start.user_code, &project, true)
            .await,
    );
    due(&f, &start.device_code).await;
    let tokens = value(
        a.device_exchange(CLIENT, &start.device_code, &request)
            .await,
    );
    assert_eq!(tokens.project_id, project);
    let retry = value(
        a.device_exchange(CLIENT, &start.device_code, &request)
            .await,
    );
    assert_eq!(retry.access_token, tokens.access_token);
    refused(
        a.device_exchange(CLIENT, &start.device_code, &"b".repeat(64))
            .await,
        "invalid_grant",
    );
    let app = f.app().await;
    assert_eq!(
        bearer(&app, "/v1/whoami", "GET", &tokens.access_token, None).await,
        StatusCode::OK
    );
    assert_eq!(
        bearer(
            &app,
            "/v1/signals/settings",
            "GET",
            &tokens.access_token,
            None
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        bearer(
            &app,
            "/v1/whoami",
            "GET",
            &tokens.access_token,
            Some("missing")
        )
        .await,
        StatusCode::NOT_FOUND
    );
    for (method, path) in [
        ("GET", "/v1/turns"),
        ("POST", "/v1/organizations"),
        ("PUT", "/v1/datasets/policy"),
        ("GET", "/v1/me"),
        ("PUT", "/v1/signals/settings"),
    ] {
        assert_eq!(
            bearer(&app, path, method, &tokens.access_token, None).await,
            StatusCode::FORBIDDEN,
            "{method} {path}"
        );
    }
    // Forging a browser cookie from a valid collector token must not bypass scope.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/me")
                .header(
                    "cookie",
                    format!("{}={}", http::COOKIE, tokens.access_token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    f.close().await;
}

#[tokio::test]
async fn refresh_response_recovery_is_durable_and_different_reuse_revokes_the_grant() {
    let Some(f) = fixture().await else {
        return;
    };
    let a = f.accounts();
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let (start, initial, exchange) = connect(&f, &a, &session, &project).await;
    let request = "c".repeat(64);
    let first = value(
        a.device_refresh(CLIENT, &initial.refresh_token, &request)
            .await,
    );
    assert!(
        a.device_subject(&initial.access_token)
            .await
            .unwrap()
            .is_none()
    );
    let reconstructed = f.accounts();
    let recovered = value(
        reconstructed
            .device_refresh(CLIENT, &initial.refresh_token, &request)
            .await,
    );
    assert_eq!(first.refresh_token, recovered.refresh_token);
    assert_eq!(first.access_token, recovered.access_token);
    assert_eq!(first.expires_at, recovered.expires_at);
    assert!(
        reconstructed
            .device_subject(&first.access_token)
            .await
            .unwrap()
            .is_some()
    );
    refused(
        a.device_exchange(CLIENT, &start.device_code, &exchange)
            .await,
        "invalid_grant",
    );
    let second = value(
        a.device_refresh(CLIENT, &first.refresh_token, &"d".repeat(64))
            .await,
    );
    let retained:i64=sqlx::query_scalar("SELECT count(*) FROM native_device_rotations WHERE grant_id=$1 AND response_key IS NOT NULL").bind(&second.grant_id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        retained, 1,
        "only the latest receipt keeps encrypted credentials"
    );
    refused(
        a.device_refresh(CLIENT, &initial.refresh_token, &request)
            .await,
        "invalid_grant",
    );
    assert!(
        a.device_subject(&second.access_token)
            .await
            .unwrap()
            .is_some(),
        "a superseded identical retry does not revoke the newer grant"
    );
    refused(
        a.device_refresh(CLIENT, &initial.refresh_token, &"e".repeat(64))
            .await,
        "invalid_grant",
    );
    assert!(
        a.device_subject(&second.access_token)
            .await
            .unwrap()
            .is_none()
    );
    refused(
        a.device_refresh(CLIENT, &second.refresh_token, &"f".repeat(64))
            .await,
        "invalid_grant",
    );
    f.close().await;
}

#[tokio::test]
async fn concurrent_refresh_retries_mint_only_one_rotation() {
    let Some(f) = fixture().await else {
        return;
    };
    let a = f.accounts();
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let (_, initial, _) = connect(&f, &a, &session, &project).await;
    let id = "1".repeat(64);
    let (one, two) = tokio::join!(
        a.device_refresh(CLIENT, &initial.refresh_token, &id),
        a.device_refresh(CLIENT, &initial.refresh_token, &id)
    );
    let one = value(one);
    let two = value(two);
    assert_eq!(one.access_token, two.access_token);
    assert_eq!(one.refresh_token, two.refresh_token);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM native_device_rotations WHERE grant_id=$1")
            .bind(one.grant_id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    f.close().await;
}

#[tokio::test]
async fn reset_membership_and_device_revocation_take_effect_without_auth_cache() {
    let Some(f) = fixture().await else {
        return;
    };
    let a = f.accounts();
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let (_, token, _) = connect(&f, &a, &session, &project).await;
    let other = f.register(&a, "other@example.test").await;
    refused(
        a.device_disconnect(&other, &token.grant_id).await,
        "invalid_request",
    );
    assert!(
        a.device_subject(&token.access_token)
            .await
            .unwrap()
            .is_some()
    );
    a.device_revoke("wrong-client", &token.refresh_token)
        .await
        .unwrap();
    assert!(
        a.device_subject(&token.access_token)
            .await
            .unwrap()
            .is_some()
    );
    value(a.device_disconnect(&session, &token.grant_id).await);
    assert!(
        a.device_subject(&token.access_token)
            .await
            .unwrap()
            .is_none()
    );
    let (_, token, _) = connect(&f, &a, &session, &project).await;
    let reset = f.token(&a, EMAIL, Purpose::Reset).await;
    assert!(a.complete(&reset, NEW_PASSWORD.into()).await.unwrap());
    assert!(
        a.device_subject(&token.access_token)
            .await
            .unwrap()
            .is_none()
    );
    refused(
        a.device_refresh(CLIENT, &token.refresh_token, &"2".repeat(64))
            .await,
        "invalid_grant",
    );
    let session = a.login(EMAIL, NEW_PASSWORD.into()).await.unwrap().unwrap();
    let (_, token, _) = connect(&f, &a, &session, &project).await;
    sqlx::query("UPDATE organization_members SET role='viewer' WHERE organization_id=(SELECT organization_id FROM devspaces WHERE id=$1)").bind(&project).execute(&f.pool).await.unwrap();
    assert!(
        a.device_subject(&token.access_token)
            .await
            .unwrap()
            .is_none()
    );
    refused(
        a.device_refresh(CLIENT, &token.refresh_token, &"3".repeat(64))
            .await,
        "invalid_grant",
    );
    f.close().await;
}

#[tokio::test]
async fn device_capacity_deny_expiry_and_encrypted_receipt_tampering_fail_closed() {
    let Some(mut f) = fixture().await else {
        return;
    };
    f.cfg
        .accounts
        .as_mut()
        .unwrap()
        .device
        .as_mut()
        .unwrap()
        .max_grants_per_user = 1;
    let a = f.accounts();
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let denied = value(a.device_start(CLIENT, ORIGIN, "Denied").await);
    value(
        a.device_decide(&session, &denied.user_code, "", false)
            .await,
    );
    refused(
        a.device_exchange(CLIENT, &denied.device_code, &"4".repeat(64))
            .await,
        "access_denied",
    );
    let expired = value(a.device_start(CLIENT, ORIGIN, "Expired").await);
    sqlx::query("UPDATE native_device_requests SET expires_at=clock_timestamp()-interval '1 second' WHERE device_sha256=$1").bind(hash_key(&expired.device_code)).execute(&f.pool).await.unwrap();
    refused(
        a.device_exchange(CLIENT, &expired.device_code, &"5".repeat(64))
            .await,
        "expired_token",
    );
    let (_, first, _) = connect(&f, &a, &session, &project).await;
    let second = value(a.device_start(CLIENT, ORIGIN, "Over capacity").await);
    value(
        a.device_decide(&session, &second.user_code, &project, true)
            .await,
    );
    due(&f, &second.device_code).await;
    refused(
        a.device_exchange(CLIENT, &second.device_code, &"6".repeat(64))
            .await,
        "access_denied",
    );
    let request = "7".repeat(64);
    let refreshed = value(
        a.device_refresh(CLIENT, &first.refresh_token, &request)
            .await,
    );
    sqlx::query("UPDATE native_device_rotations SET response_ciphertext=set_byte(response_ciphertext,0,(get_byte(response_ciphertext,0)+1)%256) WHERE grant_id=$1").bind(&first.grant_id).execute(&f.pool).await.unwrap();
    assert!(
        a.device_refresh(CLIENT, &first.refresh_token, &request)
            .await
            .is_err()
    );
    assert!(
        a.device_subject(&refreshed.access_token)
            .await
            .unwrap()
            .is_some(),
        "tampering does not mint or replace a credential"
    );
    a.device_revoke(CLIENT, &first.refresh_token).await.unwrap();
    assert!(
        a.device_subject(&refreshed.access_token)
            .await
            .unwrap()
            .is_none(),
        "a spent refresh token can still revoke its own family"
    );
    f.close().await;
}

#[tokio::test]
async fn device_browser_approval_requires_same_origin_cookie_and_rejects_bearers() {
    let Some(f) = fixture().await else {
        return;
    };
    let a = f.accounts();
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let app = f.app().await;
    let start = value(a.device_start(CLIENT, ORIGIN, "Browser proof").await);
    let body = json!({"user_code":start.user_code,"project_id":project,"approve":true});
    let cookie = format!("{}={session}", http::COOKIE);
    assert_eq!(
        rpc(
            &app,
            "/auth/native/device/decide",
            Some(body.clone()),
            Some(&cookie),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rpc(
            &app,
            "/auth/native/device/decide",
            Some(body.clone()),
            None,
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        rpc(
            &app,
            "/auth/native/device/decide",
            Some(body),
            Some(&cookie),
            Some(ORIGIN)
        )
        .await
        .0,
        StatusCode::OK
    );
    let request = Request::builder()
        .method("POST")
        .uri("/auth/native/device/inspect")
        .header("content-type", "application/json")
        .header("origin", ORIGIN)
        .header("x-rroflow-request", "dashboard")
        .header("cookie", cookie)
        .header("authorization", format!("Bearer {session}"))
        .body(Body::from(json!({"user_code":start.user_code}).to_string()))
        .unwrap();
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    f.close().await;
}

#[tokio::test]
async fn device_expiry_is_rechecked_after_credential_lock_waits() {
    let Some(f) = fixture().await else {
        return;
    };
    let a = f.accounts();
    let session = f.register(&a, EMAIL).await;
    let project = project(&f, &a, &session).await;
    let start = value(a.device_start(CLIENT, ORIGIN, "Expiry proof").await);
    value(
        a.device_decide(&session, &start.user_code, &project, true)
            .await,
    );
    due(&f, &start.device_code).await;
    let mut hold = f.pool.begin().await.unwrap();
    sqlx::query("SELECT user_id FROM native_credentials WHERE email=$1 FOR UPDATE")
        .bind(EMAIL)
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("UPDATE native_device_requests SET expires_at=clock_timestamp()+interval '3 seconds' WHERE device_sha256=$1").bind(hash_key(&start.device_code)).execute(&f.pool).await.unwrap();
    let worker = f.accounts();
    let code = start.device_code.clone();
    let task =
        tokio::spawn(async move { worker.device_exchange(CLIENT, &code, &"8".repeat(64)).await });
    wait_for_lock(&f).await;
    tokio::time::sleep(std::time::Duration::from_millis(3200)).await;
    hold.commit().await.unwrap();
    refused(task.await.unwrap(), "expired_token");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM native_device_grants")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    let (_, token, _) = connect(&f, &a, &session, &project).await;
    let mut hold = f.pool.begin().await.unwrap();
    sqlx::query("SELECT user_id FROM native_credentials WHERE email=$1 FOR UPDATE")
        .bind(EMAIL)
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("UPDATE native_device_grants SET expires_at=clock_timestamp()+interval '3 seconds' WHERE id=$1").bind(&token.grant_id).execute(&f.pool).await.unwrap();
    let worker = f.accounts();
    let task = tokio::spawn(async move {
        worker
            .device_refresh(CLIENT, &token.refresh_token, &"9".repeat(64))
            .await
    });
    wait_for_lock(&f).await;
    tokio::time::sleep(std::time::Duration::from_millis(3200)).await;
    hold.commit().await.unwrap();
    refused(task.await.unwrap(), "invalid_grant");
    f.close().await;
}
