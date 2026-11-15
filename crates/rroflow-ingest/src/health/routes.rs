//! Liveness, readiness and build identity routes.
use super::build_info;
use crate::AppState;
use axum::Json;
use axum::Router;
use axum::routing::get;
use axum::{extract::State, http::StatusCode, response::IntoResponse};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/build-info", get(|| async { Json(build_info()) }))
}

pub(crate) async fn ready(State(st): State<AppState>) -> impl IntoResponse {
    match tokio::time::timeout(std::time::Duration::from_secs(2), st.store.ready()).await {
        Ok(Ok(())) => (StatusCode::OK, "ready"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "store unavailable"),
    }
}
