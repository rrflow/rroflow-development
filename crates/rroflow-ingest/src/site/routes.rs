//! Site handlers.
use crate::{ApiError, AppState};
use axum::{extract::State, http::HeaderMap, response::IntoResponse};

/// Static public product page; no third-party scripts or client-side provider credentials.
pub(crate) async fn public_site(State(st): State<AppState>) -> impl IntoResponse {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            (
                "content-security-policy",
                "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "no-referrer"),
            ("cache-control", "no-store"),
        ],
        st.brand.public.clone(),
    )
}

/// Same-origin dashboard assets; no third-party scripts or client-side provider credentials.
pub(crate) async fn dashboard(State(st): State<AppState>) -> impl IntoResponse {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            (
                "content-security-policy",
                "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "no-referrer"),
            ("cache-control", "no-store"),
        ],
        st.brand.dashboard.clone(),
    )
}

/// Source notes are visible only after workspace authentication.
pub(crate) async fn research_notes(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    st.allowed(&headers, crate::Permission::ProjectRead).await?;
    Ok((
        [
            ("content-type", "application/json; charset=utf-8"),
            ("cache-control", "no-store"),
            ("x-content-type-options", "nosniff"),
        ],
        include_str!("../research.json"),
    ))
}
