use super::*;
use crate::tests::{call, get, post, rec};
use crate::{AppState, Config, Devspace, Store, hash_key, router};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;

fn policy(revision: u64, turns: u64, events: u64) -> Policy {
    Policy {
        schema: "rroflow.allowance-policy/v1".into(),
        revision,
        default: Allowance {
            plan: "free-fixture".into(),
            label: "Free test allowance".into(),
            turns_per_month: turns,
            events_per_month: events,
        },
    }
}
fn grant(revision: u64, limit: u64, expires: &str) -> Grant {
    Grant {
        schema: "rroflow.allowance-grant/v1".into(),
        revision,
        allowance: Allowance {
            plan: "paid-fixture".into(),
            label: "Paid test allowance".into(),
            turns_per_month: limit,
            events_per_month: limit,
        },
        expires_at: expires.into(),
    }
}
fn event(n: u64, sequence: u64) -> rroflow_core::capture::CaptureEvent {
    serde_json::from_value(serde_json::json!({
        "schema":"rroflow.capture/v1", "event_id":format!("00000000-0000-4000-8000-{n:012}"),
        "capture_id":"00000000-0000-4000-8000-000000000001", "sequence":sequence,
        "agent":"codex", "session_id":"s", "operation_id":"o", "observed_at":"2026-10-01T00:00:00Z", "kind":"call.started"
    })).unwrap()
}
fn events_request(events: &[rroflow_core::capture::CaptureEvent]) -> Request<Body> {
    Request::post("/v1/events")
        .header("authorization", "Bearer fixture-a")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({"events":events})).unwrap(),
        ))
        .unwrap()
}

#[test]
fn periods_expiry_and_configuration_are_explicit() {
    for (at, start, end) in [
        (
            "2026-12-31T23:59:59Z",
            "2026-12-01T00:00:00Z",
            "2027-01-01T00:00:00Z",
        ),
        (
            "2028-02-29T23:59:59Z",
            "2028-02-01T00:00:00Z",
            "2028-03-01T00:00:00Z",
        ),
    ] {
        let (a, b) = month(crate::pg::timestamp(at).unwrap());
        assert_eq!(a, crate::pg::timestamp(start).unwrap());
        assert_eq!(b, crate::pg::timestamp(end).unwrap());
    }
    let mut p = policy(1, 0, 0);
    assert!(p.validate().is_ok());
    p.revision = 0;
    assert!(p.validate().is_err());
    p.revision = 1;
    p.default.events_per_month = u64::MAX;
    assert!(p.validate().is_err());
    let at = crate::pg::timestamp("2026-11-01T00:00:00Z").unwrap();
    let e = Effective {
        policy: Some(policy(1, 1, 1)),
        grant: Some(grant(1, 10, "2026-11-01T00:00:00Z")),
        at,
    };
    assert_eq!(
        e.allowance().unwrap().turns_per_month,
        1,
        "expiry is exclusive"
    );
    assert!(Config::default().validate_authentication().is_err());
    let cfg = Config {
        nomen: Some(crate::NomenConfig {
            issuer: "https://issuer.example".into(),
            audiences: vec!["project".into()],
            clients: vec!["client".into()],
            key_file: "fixture.json".into(),
        }),
        ..Config::default()
    };
    assert!(
        cfg.validate_authentication().is_ok(),
        "member-only service needs no dummy devspace"
    );
}

#[tokio::test]
async fn hosted_allowances_are_atomic_isolated_replay_safe_and_revisioned() {
    let Ok(base) = std::env::var("RROFLOW_TEST_DATABASE_URL") else {
        eprintln!("Postgres allowance proof skipped: set RROFLOW_TEST_DATABASE_URL");
        return;
    };
    // A private schema avoids changing policy for any other concurrently running test.
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let schema = format!("allowance_fixture_{}", rand::random::<u64>());
    // Identifier is a fixed prefix plus a locally generated u64, never external input.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    let mut pg = PgStore::connect(url.as_str(), 10).await.unwrap();
    pg.ensure_devspaces(&["a".into(), "b".into()])
        .await
        .unwrap();
    assert_eq!(
        pg.allowance_status("a").await.unwrap()["mode"],
        "unconfigured"
    );
    assert!(pg.set_allowance_policy(&policy(2, 2, 2)).await.is_err());
    pg.set_allowance_policy(&policy(1, 2, 2)).await.unwrap();
    pg.set_allowance_policy(&policy(1, 2, 2)).await.unwrap();
    assert!(pg.set_allowance_policy(&policy(1, 3, 3)).await.is_err());
    assert!(pg.set_allowance_policy(&policy(3, 3, 3)).await.is_err());
    pg.require_allowance_policy = true;
    let store = Arc::new(Store::Postgres(pg));
    let cfg = Config {
        devspace: vec![
            Devspace {
                id: "a".into(),
                key_sha256: vec![hash_key("fixture-a")],
            },
            Devspace {
                id: "b".into(),
                key_sha256: vec![hash_key("fixture-b")],
            },
        ],
        ..Config::default()
    };
    let app = router(
        AppState::new(&cfg, store.clone()).unwrap(),
        cfg.max_body_bytes,
    );
    let Store::Postgres(pg) = store.as_ref() else {
        unreachable!()
    };
    assert_eq!(
        call(&app, get("unknown", "/v1/allowances")).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            post("fixture-a", &[rec("one", None), rec("one", None)])
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut jobs = tokio::task::JoinSet::new();
    for n in 0..16 {
        let app = app.clone();
        jobs.spawn(async move {
            call(&app, post("fixture-a", &[rec(&format!("race-{n}"), None)]))
                .await
                .0
        });
    }
    let mut accepted = 0;
    while let Some(result) = jobs.join_next().await {
        if result.unwrap() == StatusCode::OK {
            accepted += 1;
        }
    }
    assert_eq!(
        accepted, 1,
        "parallel uploads may consume only the last remaining identity"
    );
    let response = app
        .clone()
        .oneshot(post("fixture-a", &[rec("denied", None)]))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "60");
    assert_eq!(
        call(&app, post("fixture-a", &[rec("one", Some("mood"))]))
            .await
            .0,
        StatusCode::OK,
        "replacement is free"
    );
    assert_eq!(
        call(&app, get("fixture-a", "/v1/allowances")).await.1["used"]["turns"],
        2
    );
    assert_eq!(
        call(&app, get("fixture-b", "/v1/allowances?devspace=a"))
            .await
            .1["used"]["turns"],
        0
    );
    assert_eq!(
        call(&app, post("fixture-b", &[rec("one", None)])).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, events_request(&[event(1, 1)])).await.0,
        StatusCode::OK
    );
    // A conflict rolls back both new records and the quota ledger.
    assert_eq!(
        call(&app, events_request(&[event(2, 2), event(1, 99)]))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(pg.allowance_status("a").await.unwrap()["used"]["events"], 1);
    assert_eq!(
        call(&app, events_request(&[event(1, 1), event(2, 2)]))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, events_request(&[event(3, 3)])).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&app, events_request(&[event(1, 1), event(1, 1)]))
            .await
            .0,
        StatusCode::OK
    );
    let future = (Utc::now() + chrono::Duration::days(2)).to_rfc3339();
    pg.set_allowance_grant("a", &grant(1, 4, &future))
        .await
        .unwrap();
    pg.set_allowance_grant("a", &grant(1, 4, &future))
        .await
        .unwrap();
    assert!(
        pg.set_allowance_grant("a", &grant(1, 5, &future))
            .await
            .is_err()
    );
    assert!(
        pg.set_allowance_grant("unknown", &grant(1, 5, &future))
            .await
            .is_err()
    );
    assert_eq!(
        call(&app, post("fixture-a", &[rec("paid", None)])).await.0,
        StatusCode::OK
    );
    assert_eq!(
        pg.allowance_status("b").await.unwrap()["grant_active"],
        false
    );
    let past = (Utc::now() - chrono::Duration::seconds(1)).to_rfc3339();
    pg.set_allowance_grant("a", &grant(2, 4, &past))
        .await
        .unwrap();
    assert_eq!(
        pg.allowance_status("a").await.unwrap()["allowance"]["plan"],
        "free-fixture"
    );
    assert_eq!(
        pg.allowance_status("a").await.unwrap()["used"]["turns"],
        3,
        "expiry does not reset usage"
    );
    assert_eq!(
        call(&app, post("fixture-a", &[rec("paid", None)])).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, post("fixture-a", &[rec("after-expiry", None)]))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&app, get("fixture-a", "/v1/turns")).await.0,
        StatusCode::OK,
        "evidence remains readable"
    );
    pg.set_allowance_policy(&policy(2, 0, 0)).await.unwrap();
    assert_eq!(
        call(&app, post("fixture-b", &[rec("blocked", None)]))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&app, post("fixture-b", &[rec("one", None)])).await.0,
        StatusCode::OK
    );
    // Month accounting uses server acceptance time, not backdated client timestamps.
    let (start, _) = month(Utc::now());
    sqlx::query("UPDATE ingest_identities SET accepted_at=$1 WHERE devspace_id='a'")
        .bind(start - chrono::Duration::seconds(1))
        .execute(&pg.pool)
        .await
        .unwrap();
    pg.set_allowance_policy(&policy(3, 1, 1)).await.unwrap();
    assert_eq!(pg.allowance_status("a").await.unwrap()["used"]["turns"], 0);
    assert_eq!(
        call(&app, post("fixture-a", &[rec("old-client-time", None)]))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, post("fixture-a", &[rec("one", None)])).await.0,
        StatusCode::OK,
        "prior-month replay is free"
    );
    assert_eq!(pg.allowance_status("a").await.unwrap()["used"]["turns"], 1);
    // No member write route exists, regardless of supplied plan labels.
    let request = Request::post("/v1/allowances")
        .header("authorization", "Bearer fixture-a")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    sqlx::query("DELETE FROM allowance_policy")
        .execute(&pg.pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, post("fixture-a", &[rec("missing-policy", None)]))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        pg.allowance_status("a").await.unwrap()["mode"],
        "unavailable"
    );
    assert!(store.ready().await.is_err());
    assert_eq!(
        call(&app, get("fixture-a", "/v1/turns")).await.0,
        StatusCode::OK
    );
    pg.pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
