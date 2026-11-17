//! Source health tests.
use crate::tests::*;

#[tokio::test]
async fn source_reports_are_owner_scoped_durable_and_replay_never_refreshes_receipt() {
    for backend in backends() {
        let env = Env::new(backend, "source-health");
        let app = env.app().await;
        let uri = "/v1/sources/health";
        let report = source_health_report(1);
        assert_eq!(
            call(&app, json_post("wrong", uri, report.clone())).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (status, receipt) = call(&app, json_post("key-jesse", uri, report.clone())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(receipt["devspace"], env.jesse);
        assert_eq!(receipt["observation"]["report"], report);
        assert_eq!(
            call(&app, json_post("key-jesse", uri, report.clone()))
                .await
                .1,
            receipt
        );
        assert_eq!(
            call(&app, get("key-other", uri)).await.1["sources"],
            serde_json::json!([])
        );
        let restored = env.app().await;
        let listed = call(&restored, get("key-jesse", uri)).await.1;
        assert_eq!(listed["sources"][0], receipt["observation"]);
        assert_eq!(listed["recent_for_seconds"], 120);
        let mut conflict = report.clone();
        conflict["capture_errors"] = 9.into();
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, conflict))
                .await
                .0,
            StatusCode::CONFLICT
        );
        let newer = source_health_report(3);
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, newer.clone()))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, report.clone()))
                .await
                .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(&restored, get("key-jesse", uri)).await.1["sources"][0]["report"],
            newer
        );
        // The same source identity cannot route data across an authenticated owner boundary.
        assert_eq!(
            call(&restored, json_post("key-other", uri, report.clone()))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&restored, get("key-other", uri)).await.1["sources"][0]["report"]["sequence"],
            1
        );
        let (a, b) = tokio::join!(
            call(
                &restored,
                json_post("key-jesse", uri, source_health_report(4))
            ),
            call(
                &restored,
                json_post("key-jesse", uri, source_health_report(5))
            )
        );
        assert!([StatusCode::OK, StatusCode::CONFLICT].contains(&a.0));
        assert_eq!(b.0, StatusCode::OK);
        assert_eq!(
            call(&restored, get("key-jesse", uri)).await.1["sources"][0]["report"]["sequence"],
            5
        );
        for (field, value) in [
            ("source_id", serde_json::json!("../../other")),
            ("sequence", serde_json::json!(0)),
            ("observed_at", serde_json::json!("no time")),
            ("pending_bytes", serde_json::json!(999)),
            ("collector_version", serde_json::json!("unsafe/path")),
        ] {
            let mut invalid = report.clone();
            invalid[field] = value;
            assert_eq!(
                call(&restored, json_post("key-jesse", uri, invalid))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        let mut unknown = report.clone();
        let mut sdk_report = source_health_report(6);
        sdk_report["session_capture"] = false.into();
        sdk_report["sdk"] = serde_json::json!({"linked_queues":2,"pending_events":9,"pending_bytes":1024,"retained_bytes":2048,"retrying_queues":1,"queue_errors":0});
        let (code, sdk_receipt) =
            call(&restored, json_post("key-jesse", uri, sdk_report.clone())).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(sdk_receipt["observation"]["report"], sdk_report);
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, sdk_report.clone()))
                .await
                .1,
            sdk_receipt
        );
        for (field, value) in [
            ("linked_queues", 17),
            ("retrying_queues", 3),
            ("pending_bytes", 2049),
        ] {
            let mut invalid = sdk_report.clone();
            invalid["sdk"][field] = value.into();
            assert_eq!(
                call(&restored, json_post("key-jesse", uri, invalid))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        unknown["path"] = serde_json::json!("/private/session");
        assert_eq!(
            call(&restored, json_post("key-jesse", uri, unknown))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[tokio::test]
async fn source_report_storage_is_bounded_without_blocking_existing_sources() {
    for backend in backends() {
        let env = Env::new(backend, "source-capacity");
        let app = env.app().await;
        for n in 0..128 {
            let mut report = source_health_report(1);
            report["source_id"] = format!("00000000-0000-4000-8000-{n:012x}").into();
            assert_eq!(
                call(&app, json_post("key-jesse", "/v1/sources/health", report))
                    .await
                    .0,
                StatusCode::OK
            );
        }
        let mut report = source_health_report(1);
        report["source_id"] = "00000000-0000-4000-8000-ffffffffffff".into();
        assert_eq!(
            call(&app, json_post("key-jesse", "/v1/sources/health", report))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(
                &app,
                json_post("key-jesse", "/v1/sources/health", source_health_report(2))
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            call(&app, get("key-jesse", "/v1/sources/health")).await.1["sources"]
                .as_array()
                .unwrap()
                .len(),
            128
        );
    }
}
