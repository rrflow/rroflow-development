//! Health tests.
use crate::tests::*;

#[tokio::test]
async fn build_identity_and_store_readiness_are_distinct_from_liveness() {
    for backend in backends() {
        let env = Env::new(backend, "readiness");
        let store = Arc::new(env.cfg.open_store().await.unwrap());
        let app = router(
            AppState::new(&env.cfg, store.clone()).unwrap(),
            env.cfg.max_body_bytes,
        );
        assert_eq!(call(&app, get("", "/readyz")).await.0, StatusCode::OK);
        let (status, info) = call(&app, get("", "/build-info")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(info["revision"], build_info().revision);
        assert_eq!(info["migrations"].as_str().unwrap().len(), 64);
        if let Store::Postgres(pg) = &*store {
            pg.pool.close().await;
            assert_eq!(
                call(&app, get("", "/readyz")).await.0,
                StatusCode::SERVICE_UNAVAILABLE
            );
            assert_eq!(call(&app, get("", "/healthz")).await.0, StatusCode::OK);
            assert_eq!(call(&app, get("", "/build-info")).await.0, StatusCode::OK);
        }
    }
}
