use super::*;
use crate::{AppState, web_auth};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

pub(crate) const COOKIE: &str = "__Host-rroflow-native";
pub(crate) fn same_origin(accounts: &Accounts, headers: &HeaderMap) -> bool {
    headers.get("origin").and_then(|v| v.to_str().ok()) == Some(accounts.cfg.origin.as_str())
        && headers
            .get("x-rroflow-request")
            .and_then(|v| v.to_str().ok())
            == Some("dashboard")
}
fn message(status: StatusCode, value: &str) -> Response {
    (status, Json(json!({"message":value}))).into_response()
}
fn unavailable() -> Response {
    message(
        StatusCode::SERVICE_UNAVAILABLE,
        "Account service is unavailable. Please retry.",
    )
}
fn invalid() -> Response {
    message(
        StatusCode::BAD_REQUEST,
        "Check the email, password or recovery link and try again.",
    )
}
fn limited() -> Response {
    let mut r = message(
        StatusCode::TOO_MANY_REQUESTS,
        "Too many attempts. Please try again in ten minutes.",
    );
    r.headers_mut()
        .insert("retry-after", "600".parse().unwrap());
    r
}
fn active<'a>(st: &'a AppState, h: &HeaderMap) -> Result<&'a Accounts, Box<Response>> {
    let Some(accounts) = &st.accounts else {
        return Err(Box::new(unavailable()));
    };
    if !same_origin(accounts, h) {
        return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
    }
    Ok(accounts)
}
pub(crate) async fn config(State(st): State<AppState>) -> Json<serde_json::Value> {
    let (first_run, builtin_admin) = match &st.accounts {
        Some(a) => a.first_run_state().await.unwrap_or((false, false)),
        None => (false, false),
    };
    Json(
        json!({"enabled":st.accounts.is_some(),"first_run":first_run,"builtin_admin":builtin_admin,"signup_enabled":st.accounts.as_ref().is_some_and(|a|a.cfg.signup_enabled),
        "recovery_enabled":st.accounts.as_ref().is_some_and(|a|a.cfg.smtp.is_some()),"federation_enabled":st.web.is_some(),
        "federation_label":st.brand.federation_label}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestEmail {
    email: String,
    purpose: Purpose,
}
pub(crate) async fn request(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<RequestEmail>,
) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    if a.cfg.smtp.is_none() || (input.purpose == Purpose::Register && !a.cfg.signup_enabled) {
        return unavailable();
    }
    let Some(email) = normalize_email(&input.email) else {
        return invalid();
    };
    match a.allow("email", &email).await {
        Ok(true) => {}
        Ok(false) => return limited(),
        Err(_) => return unavailable(),
    }
    match a.request(&email, input.purpose).await {
        Ok(()) => message(
            StatusCode::ACCEPTED,
            "If this address is eligible, an email will arrive with the next step.",
        ),
        Err(_) => unavailable(),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordLogin {
    email: String,
    password: String,
}
pub(crate) async fn login(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<PasswordLogin>,
) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let Some(email) = normalize_login(&input.email) else {
        return invalid();
    };
    if !valid_password(&input.password) {
        return invalid();
    }
    match a.allow("login", &email).await {
        Ok(true) => {}
        Ok(false) => return limited(),
        Err(_) => return unavailable(),
    }
    match a.login(&email, input.password).await {
        Ok(Some(token)) => {
            if let Some(old) = web_auth::cookie(&h, COOKIE)
                && a.logout(old).await.is_err()
            {
                return unavailable();
            }
            if let Some(web) = &st.web
                && web.revoke(&h).await.is_err()
            {
                return unavailable();
            }
            let mut r = message(StatusCode::OK, "Signed in.");
            web_auth::set_cookie(&mut r, COOKIE, &token, a.cfg.session_ttl_seconds as u64);
            web_auth::set_cookie(&mut r, web_auth::SESSION, "", 0);
            r
        }
        Ok(None) => message(
            StatusCode::UNAUTHORIZED,
            "Email or password was not recognized.",
        ),
        Err(_) => unavailable(),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Complete {
    token: String,
    password: String,
}
pub(crate) async fn complete(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<Complete>,
) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    if !valid_secret(&input.token, ACTION_PREFIX) || !valid_password(&input.password) {
        return invalid();
    }
    match a.allow("complete", &hash_key(&input.token)).await {
        Ok(true) => {}
        Ok(false) => return limited(),
        Err(_) => return unavailable(),
    }
    match a.complete(&input.token, input.password).await {
        Ok(true) => message(StatusCode::OK, "Password saved. Sign in to continue."),
        Ok(false) => message(
            StatusCode::BAD_REQUEST,
            "This link has expired or was already used. Request a new email.",
        ),
        Err(_) => unavailable(),
    }
}

fn private(value: serde_json::Value) -> Response {
    let mut r = Json(value).into_response();
    r.headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    r
}
fn setup_refused() -> Response {
    message(
        StatusCode::UNAUTHORIZED,
        "That setup password has expired or was already used. Run the setup command for a new one.",
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClaimSetup {
    password: String,
}
/// Returns the only copy of the new password, to the browser that used the setup password.
pub(crate) async fn first_run(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<ClaimSetup>,
) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let supplied = input.password.trim();
    // No attempt limit: a guess costs one digest lookup, and a shared limit would let
    // anonymous guesses lock the real password out.
    match a.claim_setup(supplied).await {
        Ok(Some(claimed)) => {
            // Best effort. The claim has committed and this response is the only copy of
            // the new password, so a failed cleanup must not lose it.
            if let Some(old) = web_auth::cookie(&h, COOKIE) {
                let _ = a.logout(old).await;
            }
            if let Some(web) = &st.web {
                let _ = web.revoke(&h).await;
            }
            let mut r = private(json!({
                "signin": first_run::ADMIN_LOGIN,
                "password": claimed.password.as_str(),
                "revision": claimed.revision,
                "active_keys": claimed.active_keys,
            }));
            web_auth::set_cookie(
                &mut r,
                COOKIE,
                &claimed.session,
                a.cfg.session_ttl_seconds as u64,
            );
            web_auth::set_cookie(&mut r, web_auth::SESSION, "", 0);
            r
        }
        Ok(None) => setup_refused(),
        Err(_) => unavailable(),
    }
}
fn signed_out() -> Response {
    message(StatusCode::UNAUTHORIZED, "Sign in to continue.")
}
pub(crate) async fn password_status(State(st): State<AppState>, h: HeaderMap) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let Some(token) = web_auth::cookie(&h, COOKIE) else {
        return signed_out();
    };
    match a.password_status(token).await {
        Ok(Some(status)) => private(json!({
            "signin": status.signin,
            "unsaved": status.unsaved,
            "revision": status.revision,
        })),
        Ok(None) => signed_out(),
        Err(_) => unavailable(),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfirmSaved {
    revision: i64,
}
pub(crate) async fn password_saved(
    State(st): State<AppState>,
    h: HeaderMap,
    Json(input): Json<ConfirmSaved>,
) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let Some(token) = web_auth::cookie(&h, COOKIE) else {
        return signed_out();
    };
    match a.confirm_saved(token, input.revision).await {
        Ok(first_run::Saved::Done) => message(StatusCode::OK, "Password saved."),
        Ok(first_run::Saved::Stale) => message(
            StatusCode::CONFLICT,
            "A newer password was made in another tab. Reload this page.",
        ),
        Ok(first_run::Saved::SignedOut) => signed_out(),
        Err(_) => unavailable(),
    }
}
pub(crate) async fn password_replace(State(st): State<AppState>, h: HeaderMap) -> Response {
    let a = match active(&st, &h) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let Some(token) = web_auth::cookie(&h, COOKIE) else {
        return signed_out();
    };
    match a.password_status(token).await {
        Ok(Some(status)) if status.unsaved => {}
        Ok(Some(_)) => {
            return message(StatusCode::CONFLICT, "This password was already saved.");
        }
        Ok(None) => return signed_out(),
        Err(_) => return unavailable(),
    }
    match a.replace_unsaved(token).await {
        Ok(Some((password, revision))) => private(json!({
            "signin": first_run::ADMIN_LOGIN,
            "password": password.as_str(),
            "revision": revision,
        })),
        Ok(None) => message(StatusCode::CONFLICT, "This password was already saved."),
        Err(_) => unavailable(),
    }
}
