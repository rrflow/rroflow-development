//! Source health routes.
use crate::AppState;
use axum::Router;
use axum::routing::get;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/sources/health", get(super::list).post(super::ingest))
}
