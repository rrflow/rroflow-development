//! SDK activity routes.
use crate::AppState;
use axum::Router;
use axum::routing::get;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/sdk/activity", get(super::list))
        .route("/v1/sdk/summary", get(super::summary))
        .route(
            "/v1/sdk/reviews",
            get(super::reviews).post(super::save_review),
        )
}
