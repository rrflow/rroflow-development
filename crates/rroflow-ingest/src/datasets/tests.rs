use super::*;
use crate::tests::{call, fake_nomen, get, post, rec, signed_in};
use crate::{Config, Devspace, NomenConfig, hash_key, router};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;

fn policy(purpose: Purpose) -> Policy {
    Policy {
        purpose,
        enabled: true,
        consent: true,
        agents: vec!["codex".into()],
        all_projects: false,
        projects: vec!["shippinai".into()],
        retention_days: 7,
    }
}
fn update_request(token: &str, revision: u64, policy: &Policy) -> Request<Body> {
    Request::put("/v1/datasets/policy")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"expected_revision":revision,"policy":policy}).to_string(),
        ))
        .unwrap()
}
async fn review(app: &axum::Router, turn: &rroflow_core::TurnRecord, n: u32, verdict: &str) {
    let request=Request::post("/v1/reviews").header("authorization","Bearer member-a").header("content-type","application/json")
        .body(Body::from(json!({"review_id":format!("00000000-0000-4000-8000-{n:012}"),"event_id":turn.event_id,"turn_digest":crate::reviews::digest(turn),"signal":"mood","rule_version":1,"verdict":verdict}).to_string())).unwrap();
    let (code, body) = call(app, request).await;
    assert_eq!(code, StatusCode::OK, "{body}");
}
async fn due(pg: &PgStore, purpose: &str) {
    sqlx::query("UPDATE dataset_policies SET next_scan_at=clock_timestamp()-interval '1 second' WHERE devspace_id='a' AND purpose=$1").bind(purpose).execute(&pg.pool).await.unwrap();
}
async fn newest(pg: &PgStore, purpose: &str) -> String {
    sqlx::query_scalar("SELECT id FROM dataset_jobs WHERE devspace_id='a' AND purpose=$1 ORDER BY created_at DESC,id LIMIT 1").bind(purpose).fetch_one(&pg.pool).await.unwrap()
}

#[test]
fn consent_is_explicit_bounded_and_separate_from_training() {
    let mut p = policy(Purpose::Patterns);
    assert!(p.validate().is_ok());
    p.consent = false;
    assert!(p.validate().is_err());
    p.enabled = false;
    assert!(p.validate().is_ok());
    p.retention_days = 0;
    assert!(p.validate().is_err());
    let mut p = policy(Purpose::SignalReviews);
    p.projects.clear();
    assert!(p.validate().is_err());
    p.all_projects = true;
    assert!(p.validate().is_ok());
    let value = json!({"purpose":"patterns","enabled":true,"consent":true,"training":true,"agents":["codex"],"all_projects":true,"projects":[],"retention_days":7});
    assert!(serde_json::from_value::<Policy>(value).is_err());
    let cfg = DatasetConfig {
        max_rows: 0,
        ..DatasetConfig::default()
    };
    assert!(cfg.validate().is_err());
}

#[tokio::test]
async fn dataset_jobs_are_consented_private_immutable_recoverable_and_revocable() {
    let Ok(base) = std::env::var("RROFLOW_TEST_DATABASE_URL") else {
        eprintln!("dataset PostgreSQL proof skipped");
        return;
    };
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let schema = format!("dataset_fixture_{}", rand::random::<u64>());
    // Identifier is generated locally from a fixed prefix and u64, never external input.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    let pg = PgStore::connect(url.as_str(), 8).await.unwrap();
    let fake = fake_nomen().await;
    for (token, sub) in [("member-a", "a"), ("member-b", "b")] {
        fake.answers
            .lock()
            .unwrap()
            .insert(token.into(), signed_in(&fake.issuer, sub));
    }
    let dir = tempfile::tempdir().unwrap();
    let keypath = dir.path().join("key.json");
    std::fs::write(&keypath,json!({"type":"application","keyId":"fixture","key":crate::test_keys::rsa().pem,"clientId":"fixture-ingest"}).to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&keypath, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let options = DatasetConfig::default();
    let cfg = Config {
        datasets: Some(options.clone()),
        nomen: Some(NomenConfig {
            issuer: fake.issuer.clone(),
            audiences: vec!["rroflow-project".into()],
            clients: vec!["rroflow-cli".into()],
            key_file: keypath.to_string_lossy().into(),
        }),
        devspace: vec![Devspace {
            id: "a".into(),
            key_sha256: vec![hash_key("ingest-a")],
        }],
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
    assert_eq!(
        call(&app, get("member-a", "/v1/whoami")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, get("member-b", "/v1/whoami")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, get("unknown", "/v1/datasets")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let supervised = policy(Purpose::SignalReviews);
    assert_eq!(
        call(&app, update_request("ingest-a", 0, &supervised))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, get("ingest-a", "/v1/datasets")).await.1["mode"],
        "member_signin_required"
    );
    assert_eq!(
        run_once(pg, &options).await.unwrap()["published"],
        false,
        "no implicit consent"
    );
    let mut original = rec("reviewed", Some("mood"));
    original.content = Some(rroflow_core::Content {
        input: "PRIVATE_PROMPT_SECRET".into(),
        output: "PRIVATE_RESPONSE_SECRET".into(),
    });
    original.cwd = "PRIVATE_PATH".into();
    original.model = Some("PRIVATE_MODEL".into());
    assert_eq!(
        call(&app, post("ingest-a", &[original.clone()])).await.0,
        StatusCode::OK
    );
    review(&app, &original, 1, "match").await;
    review(&app, &original, 2, "match").await;
    let conflict = rec("conflict", Some("mood"));
    call(&app, post("ingest-a", std::slice::from_ref(&conflict))).await;
    review(&app, &conflict, 3, "match").await;
    review(&app, &conflict, 4, "not_match").await;
    assert_eq!(
        call(&app, update_request("member-a", 0, &supervised))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, update_request("member-a", 0, &supervised))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(run_once(pg, &options).await.unwrap()["published"], true);
    let first = newest(pg, "signal_reviews").await;
    let path = format!("/v1/datasets/jobs/{first}/artifact");
    let (code, envelope) = call(&app, get("member-a", &path)).await;
    assert_eq!(code, StatusCode::OK);
    let artifact = &envelope["artifact"];
    assert_eq!(envelope["sha256"], digest(artifact));
    assert_eq!(artifact["manifest"]["row_count"], 1);
    assert_eq!(artifact["manifest"]["exclusions"]["conflicting_reviews"], 2);
    assert_eq!(artifact["manifest"]["exclusions"]["duplicate_reviews"], 1);
    assert_eq!(artifact["rows"][0]["label"]["verdict"], "match");
    assert_eq!(
        artifact["rows"][0]["label"]["review_refs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(artifact["manifest"]["training_authorized"], false);
    assert_eq!(artifact["manifest"]["qualification"], "not_evaluated");
    assert!(!envelope.to_string().contains("PRIVATE_"));
    assert_eq!(
        call(&app, get("member-b", &path)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, get("ingest-a", &path)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, get("member-b", "/v1/datasets?devspace=a"))
            .await
            .1["jobs"],
        json!([])
    );
    due(pg, "signal_reviews").await;
    assert_eq!(
        run_once(pg, &options).await.unwrap()["scheduled"],
        false,
        "unchanged inputs are idempotent"
    );
    let mut replacement = original.clone();
    replacement.usage.input_tokens = Some(999);
    call(&app, post("ingest-a", &[replacement])).await;
    due(pg, "signal_reviews").await;
    run_once(pg, &options).await.unwrap();
    assert_eq!(
        call(&app, get("member-a", &path)).await.1,
        envelope,
        "mutable turns cannot change a reviewed dataset"
    );

    let mut unlabelled = rec("unlabelled", None);
    unlabelled.usage.input_tokens = None;
    call(&app, post("ingest-a", &[unlabelled])).await;
    let patterns = policy(Purpose::Patterns);
    call(&app, update_request("member-a", 0, &patterns)).await;
    assert_eq!(run_once(pg, &options).await.unwrap()["published"], true);
    let pattern_id = newest(pg, "patterns").await;
    let pattern = call(
        &app,
        get(
            "member-a",
            &format!("/v1/datasets/jobs/{pattern_id}/artifact"),
        ),
    )
    .await
    .1;
    assert_eq!(pattern["artifact"]["manifest"]["row_count"], 1);
    assert!(pattern["artifact"]["rows"][0]["label"].is_null());
    assert!(pattern["artifact"]["rows"][0]["features"]["usage"]["input_tokens"].is_null());
    // Capacity failure is visible and never publishes a partial/sampled dataset.
    due(pg, "signal_reviews").await;
    let small = DatasetConfig {
        max_rows: 1,
        ..options.clone()
    };
    run_once(pg, &small).await.unwrap();
    let error:Option<String>=sqlx::query_scalar("SELECT last_error FROM dataset_policies WHERE devspace_id='a' AND purpose='signal_reviews'").fetch_one(&pg.pool).await.unwrap();
    assert_eq!(error.as_deref(), Some("row_capacity"));

    // A worker may die after claiming. A fresh lease recovers it; the old lease cannot publish.
    call(&app, update_request("member-a", 1, &supervised)).await;
    assert!(worker::schedule(pg, &options).await.unwrap());
    let old = worker::claim(pg, &options).await.unwrap().unwrap();
    sqlx::query("UPDATE dataset_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE devspace_id=$1 AND id=$2").bind(&old.ds).bind(&old.id).execute(&pg.pool).await.unwrap();
    let recovered = worker::claim(pg, &options).await.unwrap().unwrap();
    assert_eq!(old.id, recovered.id);
    assert_ne!(old.token, recovered.token);
    let build = |l: &worker::Lease| {
        projection::build(
            &l.input,
            &l.ds,
            &l.id,
            l.revision,
            &l.input_digest,
            &l.created,
            &l.expires,
        )
        .unwrap()
    };
    assert!(!worker::publish(pg, &old, build(&old)).await.unwrap());
    assert!(
        worker::publish(pg, &recovered, build(&recovered))
            .await
            .unwrap()
    );
    assert_eq!(
        call(&app, get("member-a", &path)).await.0,
        StatusCode::NOT_FOUND,
        "policy revision invalidates old artifacts"
    );

    // Withdrawal while a worker holds input must win at the publish boundary.
    call(&app, update_request("member-a", 2, &supervised)).await;
    worker::schedule(pg, &options).await.unwrap();
    let lease = worker::claim(pg, &options).await.unwrap().unwrap();
    let mut stopped = supervised.clone();
    stopped.enabled = false;
    stopped.consent = false;
    call(&app, update_request("member-a", 3, &stopped)).await;
    assert!(!worker::publish(pg, &lease, build(&lease)).await.unwrap());
    let retained:i64=sqlx::query_scalar("SELECT count(*) FROM dataset_jobs WHERE devspace_id='a' AND purpose='signal_reviews' AND (input IS NOT NULL OR artifact IS NOT NULL)").fetch_one(&pg.pool).await.unwrap();
    assert_eq!(retained, 0);
    let consent_count:i64=sqlx::query_scalar("SELECT count(*) FROM dataset_consent_history WHERE devspace_id='a' AND purpose='signal_reviews'").fetch_one(&pg.pool).await.unwrap();
    assert_eq!(consent_count, 4);
    // Expiration blocks download even with no worker; the next pass purges the artifact.
    sqlx::query("UPDATE dataset_jobs SET expires_at=clock_timestamp()-interval '1 second' WHERE devspace_id='a' AND id=$1").bind(&pattern_id).execute(&pg.pool).await.unwrap();
    assert_eq!(
        call(
            &app,
            get(
                "member-a",
                &format!("/v1/datasets/jobs/{pattern_id}/artifact")
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    run_once(pg, &options).await.unwrap();
    let cleared:bool=sqlx::query_scalar("SELECT artifact IS NULL AND input IS NULL FROM dataset_jobs WHERE devspace_id='a' AND id=$1").bind(pattern_id).fetch_one(&pg.pool).await.unwrap();
    assert!(cleared);

    // Exhausted leases become terminal instead of retrying forever.
    call(&app, update_request("member-a", 1, &patterns)).await;
    assert!(worker::schedule(pg, &options).await.unwrap());
    let once = DatasetConfig {
        max_attempts: 1,
        ..options.clone()
    };
    let exhausted = worker::claim(pg, &once).await.unwrap().unwrap();
    sqlx::query("UPDATE dataset_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE devspace_id=$1 AND id=$2")
        .bind(&exhausted.ds).bind(&exhausted.id).execute(&pg.pool).await.unwrap();
    assert_eq!(run_once(pg, &once).await.unwrap()["published"], false);
    let failure: (String, bool) = sqlx::query_as(
        "SELECT error_code,input IS NULL FROM dataset_jobs WHERE devspace_id=$1 AND id=$2",
    )
    .bind(&exhausted.ds)
    .bind(&exhausted.id)
    .fetch_one(&pg.pool)
    .await
    .unwrap();
    assert_eq!(failure, ("attempts_exhausted".into(), true));

    // Corrupt queued snapshots cannot become downloadable artifacts.
    call(&app, update_request("member-a", 2, &patterns)).await;
    assert!(worker::schedule(pg, &options).await.unwrap());
    let corrupt = newest(pg, "patterns").await;
    sqlx::query(
        "UPDATE dataset_jobs SET input_digest=repeat('0',64) WHERE devspace_id='a' AND id=$1",
    )
    .bind(&corrupt)
    .execute(&pg.pool)
    .await
    .unwrap();
    assert_eq!(run_once(pg, &options).await.unwrap()["published"], false);
    let failure: (String, bool) = sqlx::query_as("SELECT error_code,input IS NULL AND artifact IS NULL FROM dataset_jobs WHERE devspace_id='a' AND id=$1")
        .bind(&corrupt).fetch_one(&pg.pool).await.unwrap();
    assert_eq!(failure, ("input_integrity".into(), true));

    // Count retention and byte capacity both operate on real snapshots.
    call(&app, update_request("member-a", 3, &patterns)).await;
    let one_retained = DatasetConfig {
        max_retained_per_purpose: 1,
        ..options.clone()
    };
    assert_eq!(
        run_once(pg, &one_retained).await.unwrap()["published"],
        true
    );
    let preceding = newest(pg, "patterns").await;
    call(
        &app,
        post(
            "ingest-a",
            &[rec("unlabelled-next", None), rec("unlabelled-last", None)],
        ),
    )
    .await;
    due(pg, "patterns").await;
    let tiny = DatasetConfig {
        max_snapshot_bytes: 1024,
        ..one_retained.clone()
    };
    assert_eq!(run_once(pg, &tiny).await.unwrap()["published"], false);
    let capacity: String = sqlx::query_scalar(
        "SELECT last_error FROM dataset_policies WHERE devspace_id='a' AND purpose='patterns'",
    )
    .fetch_one(&pg.pool)
    .await
    .unwrap();
    assert_eq!(capacity, "snapshot_capacity");
    due(pg, "patterns").await;
    assert_eq!(
        run_once(pg, &one_retained).await.unwrap()["published"],
        true
    );
    let replaced: bool = sqlx::query_scalar("SELECT status='expired' AND input IS NULL AND artifact IS NULL FROM dataset_jobs WHERE devspace_id='a' AND id=$1")
        .bind(&preceding).fetch_one(&pg.pool).await.unwrap();
    assert!(replaced);
    let published: i64 = sqlx::query_scalar("SELECT count(*) FROM dataset_jobs WHERE devspace_id='a' AND purpose='patterns' AND status='published'")
        .fetch_one(&pg.pool).await.unwrap();
    assert_eq!(published, 1);
    // Returning to an already-seen input must not evict the current version.
    for id in ["unlabelled-next", "unlabelled-last"] {
        let mut outside_scope = rec(id, None);
        outside_scope.project = Some("outside-consent".into());
        call(&app, post("ingest-a", &[outside_scope])).await;
    }
    due(pg, "patterns").await;
    assert_eq!(
        run_once(pg, &one_retained).await.unwrap()["scheduled"],
        false
    );
    let still_retained: i64 = sqlx::query_scalar("SELECT count(*) FROM dataset_jobs WHERE devspace_id='a' AND purpose='patterns' AND status='published' AND artifact IS NOT NULL")
        .fetch_one(&pg.pool).await.unwrap();
    assert_eq!(still_retained, 1);
    sqlx::query("UPDATE dataset_jobs SET created_at=clock_timestamp()-interval '100 days' WHERE devspace_id='a' AND status IN ('failed','revoked','expired')")
        .execute(&pg.pool).await.unwrap();
    run_once(pg, &one_retained).await.unwrap();
    let old_metadata: i64 = sqlx::query_scalar("SELECT count(*) FROM dataset_jobs WHERE devspace_id='a' AND created_at<clock_timestamp()-interval '90 days'")
        .fetch_one(&pg.pool).await.unwrap();
    assert_eq!(old_metadata, 0);
    pg.pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
