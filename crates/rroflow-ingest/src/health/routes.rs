//! Health handlers.
use crate::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse};

pub(crate) async fn ready(State(st): State<AppState>) -> impl IntoResponse {
    match tokio::time::timeout(std::time::Duration::from_secs(2), st.store.ready()).await {
        Ok(Ok(())) => (StatusCode::OK, "ready"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "store unavailable"),
    }
}
