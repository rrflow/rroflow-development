use super::*;

const CLAIM: &str = "/auth/native/first-run";
const CONFIG: &str = "/auth/native/config";
const LOGIN: &str = "/auth/native/login";
const STATUS: &str = "/auth/native/password/status";
const SAVED: &str = "/auth/native/password/saved";
const REPLACE: &str = "/auth/native/password/replace";

fn session(set_cookie: &Option<String>) -> String {
    set_cookie
        .as_deref()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}
async fn claim(app: &Router, setup: &str) -> (StatusCode, Value, Option<String>) {
    rpc(
        app,
        CLAIM,
        Some(json!({"password":setup})),
        None,
        Some(ORIGIN),
    )
    .await
}
async fn sign_in(app: &Router, password: &str) -> (StatusCode, Value, Option<String>) {
    rpc(
        app,
        LOGIN,
        Some(json!({"email":"Admin","password":password})),
        None,
        Some(ORIGIN),
    )
    .await
}

#[tokio::test]
async fn first_run_replaces_an_exposed_password_once_and_leaves_a_working_admin() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let before = rpc(&app, CONFIG, None, None, None).await.1;
    assert_eq!(before["first_run"], false);
    assert_eq!(before["builtin_admin"], false);

    let setup = a.issue_setup(false).await.unwrap();
    assert!(first_run::valid_setup(&setup));
    let stored: String = sqlx::query_scalar("SELECT token_sha256 FROM instance_setup")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(stored, hash_key(&setup), "only a digest is stored");
    assert_eq!(
        rpc(&app, CONFIG, None, None, None).await.1["first_run"],
        true
    );

    let wrong = claim(&app, "rrsetup-22222-22222-22222-22222-22222-22222").await;
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert!(wrong.2.is_none());
    assert_eq!(claim(&app, "guess").await.0, StatusCode::UNAUTHORIZED);

    let (status, body, cookie) = claim(&app, &setup).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["signin"], "admin");
    let password = body["password"].as_str().unwrap().to_string();
    let revision = body["revision"].as_i64().unwrap();
    assert_eq!(password.len(), 35);
    assert_ne!(password, setup.as_str());
    assert!(cookie.is_some());

    // The exposed password is dead, as a claim and as a login.
    assert_eq!(claim(&app, &setup).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(sign_in(&app, &setup).await.0, StatusCode::UNAUTHORIZED);
    let after = rpc(&app, CONFIG, None, None, None).await.1;
    assert_eq!(after["first_run"], false);
    assert_eq!(after["builtin_admin"], true);

    let cookie = session(&cookie);
    let status = rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(status.1["signin"], "admin");
    assert_eq!(status.1["unsaved"], true);
    assert_eq!(status.1["revision"], revision);
    assert_eq!(saved(&app, &cookie, revision).await, StatusCode::OK);
    let status = rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(status.1["unsaved"], false);

    let (status, _, fresh) = sign_in(&app, &password).await;
    assert_eq!(status, StatusCode::OK);
    assert!(fresh.is_some());

    // A real account: one admin with an owned personal organization and default project.
    let owned: i64 = sqlx::query_scalar("SELECT count(*) FROM instance_admins a JOIN native_credentials c USING(user_id) JOIN organization_members m ON m.user_id=a.user_id AND m.role='owner' JOIN organizations o ON o.id=m.organization_id AND o.personal JOIN devspaces d ON d.organization_id=o.id WHERE c.email='admin'")
        .fetch_one(&f.pool).await.unwrap();
    assert_eq!(owned, 1);
    let trail: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_events WHERE action LIKE 'instance.%' OR action='account.password_saved'",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert!(trail.contains(&"instance.first_run_claimed".to_string()));
    assert!(trail.contains(&"account.password_saved".to_string()));
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn only_one_concurrent_claim_wins_and_an_expired_password_is_refused() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let setup = a.issue_setup(false).await.unwrap();
    let (first, second) = tokio::join!(a.claim_setup(&setup, None), a.claim_setup(&setup, None));
    let wins = [first.unwrap(), second.unwrap()]
        .iter()
        .filter(|c| c.is_some())
        .count();
    assert_eq!(wins, 1);
    let admins: i64 = sqlx::query_scalar("SELECT count(*) FROM instance_admins")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(admins, 1);

    let setup = a.issue_setup(true).await.unwrap();
    sqlx::query("UPDATE instance_setup SET expires_at=clock_timestamp()-interval '1 second'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(a.claim_setup(&setup, None).await.unwrap().is_none());
    assert!(!a.first_run_state().await.unwrap().0);
    drop(a);
    f.close().await;
}

#[tokio::test]
async fn issuing_replaces_an_unused_password_and_reset_recovers_a_lost_admin() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let stale = a.issue_setup(false).await.unwrap();
    let setup = a.issue_setup(false).await.unwrap();
    assert!(a.claim_setup(&stale, None).await.unwrap().is_none());
    let (_, body, cookie) = claim(&app, &setup).await;
    let lost = body["password"].as_str().unwrap().to_string();
    let cookie = session(&cookie);
    let user: String = sqlx::query_scalar("SELECT user_id FROM instance_admins")
        .fetch_one(&f.pool)
        .await
        .unwrap();

    assert!(a.issue_setup(false).await.is_err(), "already claimed");
    let reset = a.issue_setup(true).await.unwrap();
    let (status, body, _) = claim(&app, &reset).await;
    assert_eq!(status, StatusCode::OK);
    let replaced = body["password"].as_str().unwrap();

    assert_eq!(sign_in(&app, &lost).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(sign_in(&app, replaced).await.0, StatusCode::OK);
    let old = rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(old.0, StatusCode::UNAUTHORIZED, "old session is signed out");
    let same: String = sqlx::query_scalar("SELECT user_id FROM instance_admins")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(same, user, "the account and its data are kept");
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn a_new_password_is_available_only_until_the_person_saves_one() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    let (_, body, cookie) = claim(&app, &setup).await;
    let first = body["password"].as_str().unwrap().to_string();
    let cookie = session(&cookie);

    let anonymous = rpc(&app, REPLACE, Some(json!({})), None, Some(ORIGIN)).await;
    assert_eq!(anonymous.0, StatusCode::UNAUTHORIZED);
    let (status, body, _) = rpc(&app, REPLACE, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(status, StatusCode::OK);
    let second = body["password"].as_str().unwrap().to_string();
    let revision = body["revision"].as_i64().unwrap();
    assert_ne!(first, second);
    assert_eq!(sign_in(&app, &first).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN))
            .await
            .0,
        StatusCode::OK,
        "replacing keeps this browser signed in"
    );

    assert_eq!(saved(&app, &cookie, revision).await, StatusCode::OK);
    let refused = rpc(&app, REPLACE, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(refused.0, StatusCode::CONFLICT);
    assert_eq!(sign_in(&app, &second).await.0, StatusCode::OK);
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn secrets_are_never_cached_and_other_sites_are_refused() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    let foreign = rpc(
        &app,
        CLAIM,
        Some(json!({"password":setup.as_str()})),
        None,
        Some("https://evil.example.test"),
    )
    .await;
    assert_eq!(foreign.0, StatusCode::FORBIDDEN);
    let bare = rpc(
        &app,
        CLAIM,
        Some(json!({"password":setup.as_str()})),
        None,
        None,
    )
    .await;
    assert_eq!(bare.0, StatusCode::FORBIDDEN);
    assert!(
        a.first_run_state().await.unwrap().0,
        "refusals burn nothing"
    );

    let response = app
        .clone()
        .oneshot(
            Request::post(CLAIM)
                .header("origin", ORIGIN)
                .header("x-rroflow-request", "dashboard")
                .header("content-type", "application/json")
                .body(Body::from(json!({"password":setup.as_str()}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn a_local_install_may_use_localhost_but_nothing_else_without_https() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    for (origin, allowed) in [
        ("http://localhost:8787", true),
        ("http://localhost", true),
        ("https://rroflow.example.test", true),
        ("http://127.0.0.1:8787", false),
        ("http://192.168.1.20:8787", false),
        ("http://localhost.evil.test", false),
        ("http://localhost:8787/app", false),
        ("http://user@localhost:8787", false),
    ] {
        let mut cfg = a.cfg.clone();
        cfg.origin = origin.into();
        assert_eq!(
            Accounts::new(&cfg, f.pool.clone()).is_ok(),
            allowed,
            "{origin}"
        );
    }
    drop(a);
    f.close().await;
}

async fn saved(app: &Router, cookie: &str, revision: i64) -> StatusCode {
    rpc(
        app,
        SAVED,
        Some(json!({"revision":revision})),
        Some(cookie),
        Some(ORIGIN),
    )
    .await
    .0
}

#[tokio::test]
async fn signing_in_with_the_generated_password_ends_any_chance_of_replacing_it() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    let (_, body, cookie) = claim(&app, &setup).await;
    let password = body["password"].as_str().unwrap().to_string();
    let cookie = session(&cookie);
    assert_eq!(sign_in(&app, &password).await.0, StatusCode::OK);
    let status = rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(status.1["unsaved"], false, "using it proves it was saved");
    let replaced = rpc(&app, REPLACE, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(replaced.0, StatusCode::CONFLICT);
    assert_eq!(sign_in(&app, &password).await.0, StatusCode::OK);
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn only_the_browser_given_a_password_can_replace_it_and_that_evicts_everyone_else() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    let (_, _, cookie) = claim(&app, &setup).await;
    let cookie = session(&cookie);
    let (user, epoch): (String, i64) =
        sqlx::query_as("SELECT user_id,credential_epoch FROM native_credentials")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    let other = a.open_session(&mut tx, &user, epoch).await.unwrap();
    tx.commit().await.unwrap();
    let other = format!("{}={other}", http::COOKIE);

    let status = rpc(&app, STATUS, Some(json!({})), Some(&other), Some(ORIGIN)).await;
    assert_eq!(status.1["unsaved"], false);
    let refused = rpc(&app, REPLACE, Some(json!({})), Some(&other), Some(ORIGIN)).await;
    assert_eq!(refused.0, StatusCode::CONFLICT);

    let replaced = rpc(&app, REPLACE, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(replaced.0, StatusCode::OK);
    let evicted = rpc(&app, STATUS, Some(json!({})), Some(&other), Some(ORIGIN)).await;
    assert_eq!(evicted.0, StatusCode::UNAUTHORIZED);
    let kept = rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(kept.0, StatusCode::OK);
    let now: i64 = sqlx::query_scalar("SELECT credential_epoch FROM native_credentials")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(now > epoch, "device grants end with the old epoch");
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn confirming_a_password_that_was_since_replaced_is_refused() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    let (_, first, cookie) = claim(&app, &setup).await;
    let cookie = session(&cookie);
    let (_, second, _) = rpc(&app, REPLACE, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    let (stale, current) = (
        first["revision"].as_i64().unwrap_or(1),
        second["revision"].as_i64().unwrap_or(2),
    );
    assert_ne!(stale, current);
    assert_eq!(saved(&app, &cookie, stale).await, StatusCode::CONFLICT);
    let status = rpc(&app, STATUS, Some(json!({})), Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(status.1["unsaved"], true);
    assert_eq!(saved(&app, &cookie, current).await, StatusCode::OK);
    assert_eq!(saved(&app, &cookie, current).await, StatusCode::OK);
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn reset_reports_connection_keys_that_still_work() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    assert_eq!(claim(&app, &setup).await.1["active_keys"], 0);
    sqlx::query("INSERT INTO project_keys(id,devspace_id,label,key_sha256,prefix,created_by) SELECT 'k_'||repeat('a',32),d.id,'kept',repeat('b',64),'rrop_',a.user_id FROM instance_admins a JOIN organization_members m ON m.user_id=a.user_id JOIN devspaces d ON d.organization_id=m.organization_id")
        .execute(&f.pool).await.unwrap();
    let reset = a.issue_setup(true).await.unwrap();
    assert_eq!(claim(&app, &reset).await.1["active_keys"], 1);
    drop((a, app));
    f.close().await;
}

#[tokio::test]
async fn anonymous_guesses_cannot_use_up_first_run_sign_in() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    f.cfg.accounts.as_mut().unwrap().global_attempts = 2;
    let a = f.accounts();
    let app = f.app().await;
    let setup = a.issue_setup(false).await.unwrap();
    for group in ["22222", "33333", "44444", "55555", "66666"] {
        let guess = format!("rrsetup-{group}-{group}-{group}-{group}-{group}-{group}");
        assert_eq!(claim(&app, &guess).await.0, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(claim(&app, &setup).await.0, StatusCode::OK);
    drop((a, app));
    f.close().await;
}

const SETTINGS: &str = "/v1/instance/settings";

async fn claim_with(
    app: &Router,
    setup: &str,
    chosen: &str,
) -> (StatusCode, Value, Option<String>) {
    rpc(
        app,
        CLAIM,
        Some(json!({"password":setup,"new_password":chosen})),
        None,
        Some(ORIGIN),
    )
    .await
}

#[tokio::test]
async fn first_run_accepts_a_password_the_person_chose() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    let config = rpc(&app, CONFIG, None, None, None).await.1;
    assert_eq!(config["password_policy"]["min"], 8);
    assert_eq!(config["password_policy"]["recommended_min"], 15);
    assert_eq!(config["password_policy"]["recommended_max"], 18);

    let setup = a.issue_setup(false).await.unwrap();
    // Too short is refused without using up the setup password.
    assert_eq!(
        claim_with(&app, &setup, "seven77").await.0,
        StatusCode::BAD_REQUEST
    );
    let (code, body, cookie) = claim_with(&app, &setup, "eight888").await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert!(
        body["password"].is_null(),
        "a chosen password is never echoed"
    );
    assert!(cookie.is_some());
    let unsaved: Option<String> = sqlx::query_scalar(
        "SELECT unsaved_session_sha256 FROM native_credentials WHERE email='admin'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(
        unsaved.is_none(),
        "nothing to save when the person chose it"
    );
    assert_eq!(sign_in(&app, "eight888").await.0, StatusCode::OK);
    assert_eq!(
        claim(&app, &setup).await.0,
        StatusCode::UNAUTHORIZED,
        "works once"
    );
}

#[tokio::test]
async fn only_an_instance_admin_changes_settings_and_a_raised_minimum_locks_nobody_out() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accounts();
    let app = f.app().await;
    assert_eq!(
        rpc(&app, SETTINGS, None, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );

    let setup = a.issue_setup(false).await.unwrap();
    let (_, _, cookie) = claim_with(&app, &setup, "eight888").await;
    let cookie = session(&cookie);
    let (code, current, _) = rpc(&app, SETTINGS, None, Some(&cookie), Some(ORIGIN)).await;
    assert_eq!(code, StatusCode::OK, "{current}");
    let setting = &current["groups"][0]["settings"][0];
    assert_eq!(setting["key"], "password_min_length");
    assert_eq!(setting["value"], 8);

    let revision = current["revision"].as_i64().unwrap();
    let put = |value: i64, revision: i64| {
        let body = json!({"revision":revision,"values":{"password_min_length":value}});
        let cookie = cookie.clone();
        let app = app.clone();
        async move {
            let request = Request::put(SETTINGS)
                .header("cookie", cookie)
                .header("origin", ORIGIN)
                .header("x-rroflow-request", "dashboard")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            app.oneshot(request).await.unwrap().status()
        }
    };
    assert_eq!(put(7, revision).await, StatusCode::BAD_REQUEST);
    assert_eq!(put(15, revision).await, StatusCode::OK);
    assert_eq!(
        put(12, revision).await,
        StatusCode::CONFLICT,
        "stale revision"
    );
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action='instance.settings_changed'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(audited, 1);
    assert_eq!(
        rpc(&app, CONFIG, None, None, None).await.1["password_policy"]["min"],
        15
    );
    assert_eq!(
        sign_in(&app, "eight888").await.0,
        StatusCode::OK,
        "existing password still works"
    );
}
