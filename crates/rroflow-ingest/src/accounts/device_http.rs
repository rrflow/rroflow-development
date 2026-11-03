use super::{
    Accounts,
    device::{self, Outcome},
    http,
};
use crate::{AppState, web_auth};
use axum::{
    Form, Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

fn error(code: &str) -> Response {
    let status = if code == "temporarily_unavailable" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({"error":code}))).into_response()
}
fn answer<T: Serialize>(result: anyhow::Result<Outcome<T>>) -> Response {
    match result {
        Ok(Outcome::Value(v)) => Json(v).into_response(),
        Ok(Outcome::Error(code)) => error(code),
        Err(_) => error("temporarily_unavailable"),
    }
}
fn accounts(st: &AppState) -> Option<&Accounts> {
    st.accounts.as_deref().filter(|a| a.device_cfg().is_some())
}
pub(crate) async fn config(State(st): State<AppState>) -> Json<serde_json::Value> {
    Json(match accounts(&st).and_then(|a| a.device_cfg()) {
        Some(cfg) => json!({"enabled":true,"resource":cfg.resource,"clients":cfg.clients}),
        None => json!({"enabled":false}),
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Start {
    client_id: String,
    resource: String,
    device_name: String,
    scope: String,
}
pub(crate) async fn start(State(st): State<AppState>, Form(input): Form<Start>) -> Response {
    let Some(a) = accounts(&st) else {
        return error("unauthorized_client");
    };
    if input.scope != device::SCOPE {
        return error("invalid_scope");
    }
    match a.device_allow("device-start", &input.client_id, true).await {
        Ok(true) => answer(
            a.device_start(&input.client_id, &input.resource, &input.device_name)
                .await,
        ),
        _ => error("temporarily_unavailable"),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Token {
    client_id: String,
    grant_type: String,
    device_code: Option<String>,
    refresh_token: Option<String>,
    request_id: String,
}
pub(crate) async fn token(State(st): State<AppState>, Form(input): Form<Token>) -> Response {
    let Some(a) = accounts(&st) else {
        return error("unauthorized_client");
    };
    let result = match (
        input.grant_type.as_str(),
        input.device_code.as_deref(),
        input.refresh_token.as_deref(),
    ) {
        ("urn:ietf:params:oauth:grant-type:device_code", Some(code), None) => {
            a.device_exchange(&input.client_id, code, &input.request_id)
                .await
        }
        ("refresh_token", None, Some(refresh)) => {
            a.device_refresh(&input.client_id, refresh, &input.request_id)
                .await
        }
        _ => return error("invalid_request"),
    };
    // A recovered receipt may describe an already-expired access token. Keep its
    // absolute expiry, and never advertise the original lifetime as fresh again.
    answer(result.map(|outcome| match outcome {
        Outcome::Value(mut tokens) => {
            tokens.expires_in = (tokens.expires_at - chrono::Utc::now().timestamp()).max(0);
            Outcome::Value(tokens)
        }
        Outcome::Error(e) => Outcome::Error(e),
    }))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Revoke {
    client_id: String,
    token: String,
}
pub(crate) async fn revoke(State(st): State<AppState>, Form(input): Form<Revoke>) -> Response {
    let Some(a) = accounts(&st) else {
        return error("unauthorized_client");
    };
    match a.device_revoke(&input.client_id, &input.token).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(_) => error("temporarily_unavailable"),
    }
}
async fn browser<'a>(
    st: &'a AppState,
    h: &'a HeaderMap,
) -> Result<(&'a Accounts, &'a str), Box<Response>> {
    let Some(a) = accounts(st) else {
        return Err(Box::new(error("unauthorized_client")));
    };
    if !http::same_origin(a, h) || h.contains_key("authorization") {
        return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
    }
    let Some(session) = web_auth::cookie(h, http::COOKIE) else {
        return Err(Box::new(StatusCode::UNAUTHORIZED.into_response()));
    };
    let user = match a.subject(session).await {
        Ok(Some(user)) => user,
        Ok(None) => return Err(Box::new(StatusCode::UNAUTHORIZED.into_response())),
        Err(_) => return Err(Box::new(error("temporarily_unavailable"))),
    };
    match a.device_allow("device-browser", &user.id, false).await {
        Ok(true) => Ok((a, session)),
        _ => Err(Box::new(error("temporarily_unavailable"))),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inspect {
    user_code: String,
}
pub(crate) async fn inspect(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Inspect>,
) -> Response {
    let (a, session) = match browser(&st, &h).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    answer(a.device_inspect(session, &input.user_code).await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Decide {
    user_code: String,
    project_id: String,
    approve: bool,
}
pub(crate) async fn decide(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Decide>,
) -> Response {
    let (a, session) = match browser(&st, &h).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    answer(
        a.device_decide(session, &input.user_code, &input.project_id, input.approve)
            .await,
    )
}
pub(crate) async fn grants(State(st): State<AppState>, h: HeaderMap) -> Response {
    let (a, session) = match browser(&st, &h).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    answer(a.device_grants(session).await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Disconnect {
    grant_id: String,
}
pub(crate) async fn disconnect(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Disconnect>,
) -> Response {
    let (a, session) = match browser(&st, &h).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    answer(a.device_disconnect(session, &input.grant_id).await)
}
/// Default-deny capability boundary, applied AFTER cookie translation too.
pub(crate) fn route_allowed(req: &Request) -> bool {
    let Some(token) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return true;
    };
    if !token.trim().starts_with(device::ACCESS_PREFIX) {
        return true;
    }
    matches!(
        (req.method().as_str(), req.uri().path()),
        ("GET", "/v1/whoami" | "/v1/signals/settings")
            | ("POST", "/v1/turns" | "/v1/events" | "/v1/sources/health")
    )
}
