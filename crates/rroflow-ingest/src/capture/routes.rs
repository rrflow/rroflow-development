//! Capture event routes.
use crate::AppState;
use axum::Router;
use axum::routing::post;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/events", post(super::ingest).get(super::list))
}
