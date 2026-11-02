//! React-owned member management; generated assets originate in console/src.
use crate::AppState;
use axum::{extract::State, response::IntoResponse};

pub(crate) async fn page(State(st): State<AppState>) -> impl IntoResponse {
    (
        [
            ("content-type", "text/html; charset=utf-8"),
            (
                "content-security-policy",
                "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
            ("x-content-type-options", "nosniff"),
            ("cache-control", "no-store"),
        ],
        st.brand.member.clone(),
    )
}
pub(crate) async fn script() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("x-content-type-options", "nosniff"),
        ],
        include_str!("member-console/app.js"),
    )
}
pub(crate) async fn style() -> impl IntoResponse {
    (
        [
            ("content-type", "text/css; charset=utf-8"),
            ("x-content-type-options", "nosniff"),
        ],
        include_str!("member-console/index.css"),
    )
}
