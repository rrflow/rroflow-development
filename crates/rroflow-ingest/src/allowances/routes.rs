//! Allowance routes.
use crate::AppState;
use axum::Router;
use axum::routing::get;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/allowances", get(super::status))
}
