//! The site's static files, embedded at build time and served with the same two headers.
use axum::response::IntoResponse;

pub(super) const CSS: &str = "text/css; charset=utf-8";
pub(super) const JS: &str = "text/javascript; charset=utf-8";

/// An embedded file, with its content type and `x-content-type-options: nosniff`.
pub(super) fn asset(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    (
        [
            ("content-type", content_type),
            ("x-content-type-options", "nosniff"),
        ],
        body,
    )
}
