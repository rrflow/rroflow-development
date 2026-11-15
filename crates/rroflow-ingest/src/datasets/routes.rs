//! Dataset routes.
use crate::AppState;
use axum::Router;
use axum::routing::{get, put};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/datasets", get(super::status))
        .route("/v1/datasets/policy", put(super::save))
        .route("/v1/datasets/jobs/{id}/artifact", get(super::artifact))
}
