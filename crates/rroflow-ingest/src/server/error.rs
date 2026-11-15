//! The answer a refused or failed request gets.
use crate::allowances;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum ApiError {
    DatasetPolicyConflict,
    MemberRequired,
    NotFound,
    /// No such organization, project or invitation, or one this caller cannot see.
    Missing,
    Forbidden(&'static str),
    /// Understood and allowed, but a rule or limit stops it. The message says which.
    Refused(&'static str),
    Quota(allowances::QuotaExceeded),
    AllowanceUnavailable,
    Unauthorized,
    Conflict,
    SettingsConflict,
    SourceConflict,
    /// Nomen could not be asked about a token; retry. Never a refusal.
    IssuerUnavailable,
    AccountUnavailable,
    BadRequest(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::Quota(quota) = self {
            return (StatusCode::TOO_MANY_REQUESTS, [("Retry-After", "60")], Json(serde_json::json!({
                "error":"allowance_exceeded", "message":"This workspace has reached its monthly allowance. Keep pending captures locally and retry later.",
                "quota": quota
            }))).into_response();
        }
        let (code, msg) = match self {
            ApiError::DatasetPolicyConflict => (
                StatusCode::CONFLICT,
                "Dataset permission changed. Refresh before saving.".into(),
            ),
            ApiError::MemberRequired => (
                StatusCode::FORBIDDEN,
                "Sign in with your member account to do this.".into(),
            ),
            ApiError::Missing => (StatusCode::NOT_FOUND, "Not found.".into()),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message.into()),
            ApiError::Refused(message) => (StatusCode::CONFLICT, message.into()),
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                "Dataset unavailable, expired or withdrawn.".into(),
            ),
            ApiError::Quota(_) => unreachable!(),
            ApiError::AllowanceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "allowance_policy_unavailable: keep pending captures and retry".into(),
            ),
            ApiError::Conflict => (StatusCode::CONFLICT, "capture identity conflict".into()),
            ApiError::SourceConflict => (
                StatusCode::CONFLICT,
                "source report sequence is stale or conflicting".into(),
            ),
            ApiError::SettingsConflict => (
                StatusCode::CONFLICT,
                "Signal settings changed in another tab. Refresh before saving.".into(),
            ),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "missing or unknown credential: sign in or provide a valid ingest key".to_string(),
            ),
            ApiError::AccountUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Account storage is unavailable. Please retry.".into(),
            ),
            ApiError::IssuerUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "issuer_unavailable: Nomen could not be reached to check the sign-in; retry".into(),
            ),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "store error".into()),
        };
        (code, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

/// Log a store failure (never record content) and answer 500.
pub(crate) fn internal(e: anyhow::Error) -> ApiError {
    eprintln!("store error: {e:#}");
    ApiError::Internal
}
