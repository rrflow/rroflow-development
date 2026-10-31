use super::*;
use crate::tests::{call, get};
use crate::{Config, Devspace, hash_key, router};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use rroflow_core::capture::CaptureEvent;
use std::sync::Arc;
fn event(n: u64, kind: &str) -> CaptureEvent {
    serde_json::from_value(json!({"schema":"rroflow.capture/v1","event_id":format!("00000000-0000-4000-8000-{n:012}"),"capture_id":"11111111-1111-4111-8111-111111111111","sequence":n,"agent":"codex","session_id":"fixture-session","operation_id":"call","observed_at":"2026-10-30T00:00:00Z","kind":kind})).unwrap()
}
fn post_review(key: &str, value: Value) -> Request<Body> {
    Request::post("/v1/sdk/reviews")
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}
#[test]
fn worker_configuration_is_bounded() {
    let mut c = ActivityConfig::default();
    assert!(c.validate().is_ok());
    c.max_events_per_capture = 0;
    assert!(c.validate().is_err());
    c = ActivityConfig::default();
    c.lease_seconds = 0;
    assert!(c.validate().is_err());
}
#[tokio::test]
async fn hosted_sdk_projections_recover_isolate_paginate_and_pin_reviews() {
    let Ok(base) = std::env::var("RROFLOW_TEST_DATABASE_URL") else {
        return;
    };
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let schema = format!("sdk_activity_fixture_{}", rand::random::<u64>());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    let pg = PgStore::connect(url.as_str(), 8).await.unwrap();
    pg.ensure_devspaces(&["a".into(), "b".into()])
        .await
        .unwrap();
    pg.set_allowance_policy(&serde_json::from_value(json!({"schema":"rroflow.allowance-policy/v1","revision":1,"default":{"plan":"free","label":"Fixture","turns_per_month":0,"events_per_month":1000}})).unwrap()).await.unwrap();
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
    let store = Arc::new(Store::Postgres(pg));
    let app = router(
        AppState::new(&cfg, store.clone()).unwrap(),
        cfg.max_body_bytes,
    );
    let Store::Postgres(pg) = store.as_ref() else {
        unreachable!()
    };
    let options = ActivityConfig::default();
    let initial = vec![event(0, "call.started"), event(1, "call.failed")];
    pg.capture_insert("a", &initial).await.unwrap();
    pg.capture_insert("a", &initial).await.unwrap();
    let source: i64 =
        sqlx::query_scalar("SELECT source_revision FROM sdk_activity_work WHERE devspace_id='a'")
            .fetch_one(&pg.pool)
            .await
            .unwrap();
    assert_eq!(source, 1, "replay does not dirty the projection");
    let lease = worker::claim(pg, &options).await.unwrap().unwrap();
    let (rev, activities) = worker::snapshot(pg, &lease, &options).await.unwrap();
    pg.capture_insert("a", &[event(2, "stream.completed")])
        .await
        .unwrap();
    assert!(worker::publish(pg, &lease, rev, &activities).await.unwrap());
    let (code, list) = call(&app, get("fixture-a", "/v1/sdk/activity")).await;
    assert_eq!(code, StatusCode::OK, "{list}");
    assert_eq!(list["matched_activities"], 1);
    assert_eq!(list["activities"][0]["new_events_pending"], true);
    assert_eq!(list["coverage"]["pending_captures"], 1);
    let old = list["activities"][0].clone();
    let review = json!({"review_id":"22222222-2222-4222-8222-222222222222","activity_id":old["activity"]["activity_id"],"revision":old["revision"],"signal":"sdk_call_failed","rule_version":1,"verdict":"match"});
    assert_eq!(
        call(&app, post_review("fixture-a", review.clone())).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, post_review("fixture-b", review.clone())).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(run_once(pg, &options).await.unwrap()["published"], true);
    let (_, fresh) = call(
        &app,
        get(
            "fixture-a",
            "/v1/sdk/activity?signal=sdk_call_failed&outcome=failed&limit=1",
        ),
    )
    .await;
    assert_eq!(fresh["matched_activities"], 1);
    assert_eq!(fresh["coverage"]["pending_captures"], 0);
    assert_ne!(old["revision"], fresh["activities"][0]["revision"]);
    let mut stale = review.clone();
    stale["review_id"] = json!("33333333-3333-4333-8333-333333333333");
    assert_eq!(
        call(&app, post_review("fixture-a", stale)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, post_review("fixture-a", review.clone())).await.0,
        StatusCode::OK,
        "accepted review replay survives projection changes"
    );
    let (_, other) = call(&app, get("fixture-b", "/v1/sdk/activity")).await;
    assert_eq!(other["matched_activities"], 0);
    assert_eq!(
        call(&app, get("fixture-b", "/v1/sdk/reviews")).await.1["reviews"],
        json!([])
    );
    let saved: SqlJson<Review> =
        sqlx::query_scalar("SELECT record FROM sdk_activity_reviews WHERE devspace_id='a'")
            .fetch_one(&pg.pool)
            .await
            .unwrap();
    assert_eq!(revision(&saved.0.snapshot), old["revision"]);
    let exact_review = "/v1/sdk/reviews?review_id=22222222-2222-4222-8222-222222222222";
    assert_eq!(
        call(&app, get("fixture-a", exact_review)).await.1["reviews"][0]["snapshot"]["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        call(&app, get("fixture-b", exact_review)).await.1["reviews"],
        json!([])
    );
    assert!(
        old["activity"]["events"].is_null(),
        "list contains compact metadata only"
    );
    let detail_uri = format!(
        "/v1/sdk/activity?id={}",
        old["activity"]["activity_id"].as_str().unwrap()
    );
    assert_eq!(
        call(&app, get("fixture-a", &detail_uri)).await.1["activities"][0]["activity"]["events"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let raw_uri = "/v1/events?event_id=00000000-0000-4000-8000-000000000000&limit=1";
    assert_eq!(
        call(&app, get("fixture-a", raw_uri)).await.1["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&app, get("fixture-b", raw_uri)).await.1["events"],
        json!([])
    );
    let charged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingest_identities WHERE devspace_id='a' AND kind='turns'",
    )
    .fetch_one(&pg.pool)
    .await
    .unwrap();
    assert_eq!(charged, 0);
    // Crash after claim: lease expiry permits another worker, never the stale publisher.
    pg.capture_insert("a", &[event(3, "stream.completed")])
        .await
        .unwrap();
    let expired = worker::claim(pg, &options).await.unwrap().unwrap();
    let (rev, snap) = worker::snapshot(pg, &expired, &options).await.unwrap();
    sqlx::query("UPDATE sdk_activity_work SET lease_until=clock_timestamp()-interval '1 second' WHERE devspace_id='a'").execute(&pg.pool).await.unwrap();
    let replacement = worker::claim(pg, &options).await.unwrap().unwrap();
    assert!(!worker::publish(pg, &expired, rev, &snap).await.unwrap());
    assert!(worker::publish(pg, &replacement, rev, &snap).await.unwrap());
    // Capacity is visible, retains the last published revision and never drops raw data.
    pg.capture_insert("a", &[event(4, "stream.completed")])
        .await
        .unwrap();
    let small = ActivityConfig {
        max_events_per_capture: 1,
        ..options.clone()
    };
    assert_eq!(
        run_once(pg, &small).await.unwrap()["error"],
        "event_capacity"
    );
    let (_, limited) = call(&app, get("fixture-a", "/v1/sdk/activity")).await;
    assert_eq!(limited["coverage"]["error_captures"], 1);
    assert_eq!(limited["matched_activities"], 1);
    retry_errors(pg).await.unwrap();
    assert_eq!(run_once(pg, &options).await.unwrap()["published"], true);
    // Same timestamps: cursor pages and their full-cohort count use identical filters.
    for i in 10..15 {
        let mut e = event(i, "call.failed");
        e.capture_id = format!("44444444-4444-4444-8444-{i:012}");
        e.operation_id = format!("op-{i}");
        pg.capture_insert("a", &[e]).await.unwrap();
        run_once(pg, &options).await.unwrap();
    }
    let mut uri = "/v1/sdk/activity?signal=sdk_call_failed&limit=2".to_string();
    let mut ids = std::collections::BTreeSet::new();
    loop {
        let (code, page) = call(&app, get("fixture-a", &uri)).await;
        assert_eq!(code, StatusCode::OK, "{page}");
        assert_eq!(page["matched_activities"], 6);
        for row in page["activities"].as_array().unwrap() {
            assert!(ids.insert(row["activity"]["activity_id"].as_str().unwrap().to_string()));
        }
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        let q = reqwest::Url::parse_with_params(
            "http://fixture/v1/sdk/activity",
            [
                ("signal", "sdk_call_failed"),
                ("limit", "2"),
                ("cursor", cursor),
            ],
        )
        .unwrap();
        uri = format!("/v1/sdk/activity?{}", q.query().unwrap());
    }
    assert_eq!(ids.len(), 6);
    assert_eq!(
        call(&app, get("fixture-a", "/v1/sdk/activity?search=%25_"))
            .await
            .1["matched_activities"],
        0
    );
    assert_eq!(
        call(&app, get("fixture-a", "/v1/sdk/activity?cursor=bad"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, get("fixture-a", "/v1/sdk/activity?since=bad"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut good = event(30, "call.completed");
    good.capture_id = "55555555-5555-4555-8555-555555555555".into();
    pg.capture_insert("a", &[good]).await.unwrap();
    run_once(pg, &options).await.unwrap();
    assert_eq!(
        call(&app, get("fixture-a", "/v1/sdk/activity?has_findings=true"))
            .await
            .1["matched_activities"],
        6
    );
    assert_eq!(
        call(&app, get("fixture-a", "/v1/sdk/activity")).await.1["matched_activities"],
        7
    );
    sqlx::query("UPDATE sdk_activity_work SET status='running',attempts=3,lease_until=clock_timestamp()-interval '1 second' WHERE capture_id='55555555-5555-4555-8555-555555555555'").execute(&pg.pool).await.unwrap();
    assert!(
        !run_once(pg, &options).await.unwrap()["worked"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        call(&app, get("fixture-a", "/v1/sdk/activity")).await.1["coverage"]["error_captures"],
        1
    );
    retry_errors(pg).await.unwrap();
    assert_eq!(run_once(pg, &options).await.unwrap()["published"], true);
    pg.pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
async fn migration_backfills_existing_events_without_rewriting_them() {
    let Ok(base) = std::env::var("RROFLOW_TEST_DATABASE_URL") else {
        return;
    };
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let schema = format!("sdk_upgrade_fixture_{}", rand::random::<u64>());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    let pool = sqlx::PgPool::connect(url.as_str()).await.unwrap();
    let mut old = sqlx::migrate!("./migrations");
    old.migrations =
        std::borrow::Cow::Owned(old.iter().filter(|m| m.version < 10).cloned().collect());
    old.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO devspaces(id) VALUES('legacy')")
        .execute(&pool)
        .await
        .unwrap();
    let e = event(0, "call.failed");
    sqlx::query("INSERT INTO capture_events(devspace_id,event_id,capture_id,sequence,agent,observed_at,record) VALUES('legacy',$1,$2,$3,$4,$5,$6)").bind(&e.event_id).bind(&e.capture_id).bind(e.sequence as i64).bind(&e.agent).bind(crate::pg::timestamp(&e.observed_at)).bind(SqlJson(&e)).execute(&pool).await.unwrap();
    pool.close().await;
    let pg = PgStore::connect(url.as_str(), 4).await.unwrap();
    assert_eq!(
        run_once(&pg, &ActivityConfig::default()).await.unwrap()["published"],
        true
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sdk_activities WHERE devspace_id='legacy'")
            .fetch_one(&pg.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let original: SqlJson<CaptureEvent> =
        sqlx::query_scalar("SELECT record FROM capture_events WHERE devspace_id='legacy'")
            .fetch_one(&pg.pool)
            .await
            .unwrap();
    assert_eq!(original.0, e);
    pg.pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
