//! Member console routes: the console's pages and its built assets.
use crate::AppState;
use axum::Router;
use axum::routing::get;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/app/manage", get(super::page))
        .route("/app/account", get(super::page))
        .route("/app/device", get(super::page))
        .route("/app/settings", get(super::page))
        .route("/assets/member-console/app.js", get(super::script))
        .route("/assets/member-console/index.css", get(super::style))
}
