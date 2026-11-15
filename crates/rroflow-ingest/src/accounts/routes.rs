//! Native account and device sign-in routes. Their bodies are small, so they share a 4 KiB limit.
use super::{device_http, http};
use crate::AppState;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/native/config", get(http::config))
        .route("/auth/native/request", post(http::request))
        .route("/auth/native/login", post(http::login))
        .route("/auth/native/complete", post(http::complete))
        .route("/auth/native/first-run", post(http::first_run))
        .route("/auth/native/password/status", post(http::password_status))
        .route("/auth/native/password/saved", post(http::password_saved))
        .route(
            "/auth/native/password/replace",
            post(http::password_replace),
        )
        .route("/auth/native/device/config", get(device_http::config))
        .route("/auth/native/device/start", post(device_http::start))
        .route("/auth/native/device/token", post(device_http::token))
        .route("/auth/native/device/revoke", post(device_http::revoke))
        .route("/auth/native/device/inspect", post(device_http::inspect))
        .route("/auth/native/device/decide", post(device_http::decide))
        .route("/auth/native/device/grants", post(device_http::grants))
        .route(
            "/auth/native/device/disconnect",
            post(device_http::disconnect),
        )
        .layer(DefaultBodyLimit::max(4096))
}
