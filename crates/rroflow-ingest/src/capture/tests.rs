//! Capture event tests.
use crate::tests::*;

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
async fn engine_observations_are_immutable_isolated_and_survive_restart() {
    for backend in backends() {
        let env = Env::new(backend, "engine-observation");
        let app = env.app().await;
        // Protocol fixture, not an attested engine result or model-quality label.
        let event = serde_json::json!({
            "schema":"rroflow.capture/v1",
            "event_id":"11111111-1111-4111-8111-111111111111",
            "capture_id":"22222222-2222-4222-8222-222222222222",
            "sequence":0,"agent":"rrflow","session_id":"fixture-session",
            "operation_id":"fixture-read","observed_at":"2026-10-31T00:00:00Z",
            "kind":"engine.observed",
            "engine":{"version":1,"instance_id":"fixture-instance","scope":"fixture-scope",
                "operation":"task-attempt-read","response_sha256":"a".repeat(64),
                "task_id":"fixture-task","attempt_id":"fixture-attempt",
                "configuration_sha256":"b".repeat(64)}
        });
        let expected: rroflow_core::capture::CaptureEvent =
            serde_json::from_value(event.clone()).unwrap();
        let send = |key: &str, body: serde_json::Value| {
            json_post(key, "/v1/events", serde_json::json!({"events":[body]}))
        };
        assert_eq!(
            call(&app, send("invalid-key", event.clone())).await.0,
            StatusCode::UNAUTHORIZED
        );
        for _ in 0..2 {
            let (code, receipt) = call(&app, send("key-jesse", event.clone())).await;
            assert_eq!(code, StatusCode::OK, "{receipt}");
            assert_eq!(
                receipt["accepted_ids"],
                serde_json::json!([event["event_id"]])
            );
        }
        let mut conflicting = event.clone();
        conflicting["engine"]["response_sha256"] = "c".repeat(64).into();
        assert_eq!(
            call(&app, send("key-jesse", conflicting)).await.0,
            StatusCode::CONFLICT
        );
        let mut forbidden = event.clone();
        forbidden["payload"] = serde_json::json!({"private":"fixture text"});
        assert_eq!(
            call(&app, send("key-jesse", forbidden)).await.0,
            StatusCode::BAD_REQUEST
        );
        let (_, other) = call(&app, get("key-other", "/v1/events?agent=rrflow")).await;
        assert_eq!(other["events"], serde_json::json!([]));
        drop(app);
        let reopened = env.app().await;
        let (code, stored) = call(&reopened, get("key-jesse", "/v1/events?agent=rrflow")).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(stored["events"], serde_json::json!([expected]));
        // Capture admission must not fabricate a legacy turn that dataset policies
        // could mistake for a provider outcome.
        let (code, turns) = call(&reopened, get("key-jesse", "/v1/turns")).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(turns["turns"], serde_json::json!([]));
        // Reopen still refuses changing the retained observation.
        let mut conflicting = event;
        conflicting["engine"]["task_id"] = "different-task".into();
        assert_eq!(
            call(&reopened, send("key-jesse", conflicting)).await.0,
            StatusCode::CONFLICT
        );
    }
}
