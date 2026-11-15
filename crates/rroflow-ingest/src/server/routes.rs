//! The server's own route: who a credential belongs to.
use super::{ApiError, AppState, Caller};
use axum::{Json, extract::State, http::HeaderMap};

/// Who the credential belongs to and which devspace it opens.
pub(crate) async fn get_whoami(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Caller>, ApiError> {
    Ok(Json(st.authenticate(&headers).await?))
}
