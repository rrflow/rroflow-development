//! Review tests.
use crate::tests::*;

#[tokio::test]
async fn reviews_pin_evidence_reject_stale_and_isolate_owners() {
    for b in backends() {
        let env = Env::new(b, "reviews");
        let app = env.app().await;
        let turn = rec("review-turn", Some("blocked_handoff"));
        call(&app, post("key-jesse", std::slice::from_ref(&turn))).await;
        let (_, readback) = call(&app, get("key-jesse", "/v1/turns")).await;
        let mut review = serde_json::json!({"review_id":"11111111-1111-4111-8111-111111111111","event_id":turn.event_id,"turn_digest":readback["revisions"][&turn.event_id],"signal":"blocked_handoff","rule_version":1,"verdict":"not_match"});
        let request = |key: &str, body: &serde_json::Value| {
            Request::post("/v1/reviews")
                .header("authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap()
        };
        assert_eq!(
            call(&app, request("key-other", &review)).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::OK
        );
        let mut changed = turn.clone();
        changed.tools_unfinished = 1;
        call(&app, post("key-jesse", &[changed])).await;
        // Same receipt is idempotent even after the turn changes.
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::OK
        );
        review["verdict"] = serde_json::json!("match");
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::CONFLICT
        );
        review["review_id"] = serde_json::json!("22222222-2222-4222-8222-222222222222");
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::BAD_REQUEST
        );
        review["review_id"] = serde_json::json!("../../outside");
        assert_eq!(
            call(&app, request("key-jesse", &review)).await.0,
            StatusCode::BAD_REQUEST
        );
        drop(app);
        let app = env.app().await;
        let (_, rows) = call(&app, get("key-jesse", "/v1/reviews")).await;
        assert_eq!(rows["reviews"].as_array().unwrap().len(), 1);
        assert_eq!(rows["reviews"][0]["request"]["verdict"], "not_match");
        let (_, other) = call(&app, get("key-other", "/v1/reviews")).await;
        assert!(other["reviews"].as_array().unwrap().is_empty());
    }
}
