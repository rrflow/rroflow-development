//! Browser sign-in routes.
use crate::AppState;
use axum::Router;
use axum::routing::{get, post};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/config", get(super::config))
        .route("/auth/login", get(super::login))
        .route("/auth/federation", get(super::federation_login))
        .route("/auth/callback", get(super::callback))
        .route("/auth/logout", post(super::logout))
}
