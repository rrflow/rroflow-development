//! Site routes: the public product page, the original dashboard and their assets.
use super::assets::{CSS, JS, asset};
use crate::{ApiError, AppState};
use axum::Router;
use axum::routing::get;
use axum::{extract::State, http::HeaderMap, response::IntoResponse};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/assets/dashboard-datasets.js",
            get(|| async { asset(JS, include_str!("../dashboard-datasets.js")) }),
        )
        .route(
            "/assets/dashboard-sdk.js",
            get(|| async { asset(JS, include_str!("../dashboard-sdk.js")) }),
        )
        .route("/", get(public_site))
        .route("/app", get(dashboard))
        .route("/app/", get(dashboard))
        .route(
            "/assets/brand.css",
            get(|State(st): State<AppState>| async move {
                (
                    [
                        ("content-type", "text/css; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                        ("cache-control", "no-store"),
                    ],
                    st.brand.css.clone(),
                )
            }),
        )
        .route(
            "/assets/public.css",
            get(|| async { asset(CSS, include_str!("../public.css")) }),
        )
        .route(
            "/assets/public.js",
            get(|| async { asset(JS, include_str!("../public.js")) }),
        )
        .route(
            "/assets/dashboard.css",
            get(|| async { asset(CSS, include_str!("../dashboard.css")) }),
        )
        .route(
            "/assets/dashboard-a2ui.js",
            get(|| async { asset(JS, include_str!("../dashboard-a2ui.js")) }),
        )
        .route(
            "/assets/dashboard.js",
            get(|| async { asset(JS, include_str!("../dashboard.js")) }),
        )
        .route("/assets/research.json", get(research_notes))
}

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
