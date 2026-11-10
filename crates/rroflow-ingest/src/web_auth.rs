//! Browser login. The browser gets an opaque, host-only, HttpOnly cookie.
//! A configured key ring enables encrypted PostgreSQL sessions across restarts.
use crate::{AppState, Config};
use axum::Json;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub(crate) const SESSION: &str = "__Host-rroflow-session";
const LOGIN: &str = "__Host-rroflow-login";
const CAPACITY: usize = 4096;
fn default_capacity() -> usize {
    CAPACITY
}
fn default_session_ttl() -> u64 {
    3600
}
const LOGIN_TTL: Duration = Duration::from_secs(600);
mod sessions;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowserConfig {
    /// Canonical HTTPS origin, also registered in Nomen with /auth/callback.
    pub origin: String,
    /// Public PKCE dashboard client. Must be allowed by nomen.clients.
    pub client_id: String,
    /// Private JSON key ring. Requires PostgreSQL; absent selects legacy memory sessions.
    #[serde(default)]
    pub session_keys_file: Option<std::path::PathBuf>,
    #[serde(default = "default_capacity")]
    pub session_capacity: usize,
    #[serde(default = "default_session_ttl")]
    pub session_ttl_seconds: u64,
}

struct Pending {
    binding: String,
    verifier: String,
    expires: Instant,
}
struct Session {
    token: String,
    expires: Instant,
}
pub(crate) struct WebAuth {
    origin: String,
    issuer: String,
    client_id: String,
    audience: String,
    http: reqwest::Client,
    pending: Mutex<HashMap<String, Pending>>,
    sessions: Mutex<HashMap<String, Session>>,
    durable: Option<sessions::DurableSessions>,
    capacity: usize,
    session_ttl: u64,
}

/// Browsers treat http://localhost as secure for cookies and passkeys, so a fresh local
/// install can sign in before TLS exists. An IP address or other host cannot.
pub(crate) fn member_origin(s: &str) -> bool {
    https_origin(s)
        || reqwest::Url::parse(s).is_ok_and(|u| {
            u.scheme() == "http"
                && u.host_str() == Some("localhost")
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
                && u.path() == "/"
        })
}

pub(crate) fn https_origin(s: &str) -> bool {
    reqwest::Url::parse(s).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.path() == "/"
    })
}

impl WebAuth {
    pub(crate) fn new(
        cfg: &Config,
        pool: Option<&sqlx::PgPool>,
    ) -> anyhow::Result<Option<Arc<Self>>> {
        let Some(b) = &cfg.browser else {
            return Ok(None);
        };
        let n = cfg
            .nomen
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("[browser] requires [nomen]"))?;
        anyhow::ensure!(
            https_origin(&b.origin) && https_origin(&n.issuer),
            "browser origin and Nomen issuer must be HTTPS origins"
        );
        anyhow::ensure!(
            n.clients.contains(&b.client_id) && !b.client_id.is_empty(),
            "browser client_id must be in nomen.clients"
        );
        anyhow::ensure!(
            n.audiences.len() == 1,
            "browser sign-in requires exactly one Nomen project audience"
        );
        anyhow::ensure!(
            (1..=1_000_000).contains(&b.session_capacity)
                && (1..=86400).contains(&b.session_ttl_seconds),
            "browser session limits are out of range"
        );
        let durable = b
            .session_keys_file
            .as_ref()
            .map(|path| {
                let pool = pool.ok_or_else(|| {
                    anyhow::anyhow!("browser session_keys_file requires PostgreSQL")
                })?;
                sessions::DurableSessions::new(
                    pool.clone(),
                    path,
                    &[
                        "rroflow.browser-session-context/v1",
                        b.origin.trim_end_matches('/'),
                        n.issuer.trim_end_matches('/'),
                        &b.client_id,
                        &n.audiences[0],
                    ],
                    b.session_capacity,
                )
            })
            .transpose()?;
        Ok(Some(Arc::new(Self {
            origin: b.origin.trim_end_matches('/').into(),
            issuer: n.issuer.trim_end_matches('/').into(),
            client_id: b.client_id.clone(),
            audience: n.audiences[0].clone(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            pending: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            durable,
            capacity: b.session_capacity,
            session_ttl: b.session_ttl_seconds,
        })))
    }
    fn same_origin(&self, headers: &HeaderMap) -> bool {
        headers.get("origin").and_then(|v| v.to_str().ok()) == Some(self.origin.as_str())
            && headers
                .get("x-rroflow-request")
                .and_then(|v| v.to_str().ok())
                == Some("dashboard")
    }
    pub(crate) async fn revoke(&self, headers: &HeaderMap) -> anyhow::Result<()> {
        if let Some(id) = cookie(headers, SESSION) {
            if let Some(durable) = &self.durable {
                durable.remove(id).await?;
            } else {
                self.sessions.lock().await.remove(id);
            }
        }
        Ok(())
    }
    async fn token(&self, headers: &HeaderMap) -> anyhow::Result<Option<String>> {
        let Some(id) = cookie(headers, SESSION) else {
            return Ok(None);
        };
        if let Some(durable) = &self.durable {
            return durable.get(id).await;
        }
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, s| s.expires > Instant::now());
        Ok(sessions.get(id).map(|s| s.token.clone()))
    }
}

fn random() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
pub(crate) fn cookie<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    // Reject duplicate cookies instead of choosing an ambiguous credential.
    let mut values = h
        .get_all("cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .filter_map(|p| p.trim().split_once('='))
        .filter(|(n, _)| *n == name)
        .map(|(_, v)| v);
    let first = values.next()?;
    if values.next().is_some() {
        None
    } else {
        Some(first)
    }
}
pub(crate) fn set_cookie(r: &mut Response, name: &str, value: &str, seconds: u64) {
    r.headers_mut().append(
        "set-cookie",
        HeaderValue::from_str(&format!(
            "{name}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={seconds}"
        ))
        .expect("cookie values are generated base64url"),
    );
}

pub(crate) async fn config(State(st): State<AppState>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"enabled": st.web.is_some() || st.accounts.is_some(), "member_origin": st.web.as_ref().map(|web| &web.origin).or_else(||st.accounts.as_ref().map(|a|&a.cfg.origin)),
            "session_storage": st.web.as_ref().map(|web| if web.durable.is_some() { "postgres" } else { "memory" }).or_else(||st.accounts.as_ref().map(|_|"postgres"))}),
    )
}

pub(crate) async fn login(State(st): State<AppState>) -> Response {
    if st.accounts.is_some() || st.web.is_some() {
        return Redirect::to("/app/account").into_response();
    }
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

pub(crate) async fn federation_login(State(st): State<AppState>) -> Response {
    let Some(web) = &st.web else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (state, binding, verifier) = (random(), random(), random());
    let mut url = reqwest::Url::parse(&format!("{}/oauth/v2/authorize", web.issuer))
        .expect("validated issuer");
    url.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", web.client_id.as_str()),
        ("redirect_uri", &format!("{}/auth/callback", web.origin)),
        (
            "scope",
            &format!(
                "openid profile urn:zitadel:iam:org:project:id:{}:aud",
                web.audience
            ),
        ),
        ("state", &state),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge(&verifier)),
    ]);
    let mut pending = web.pending.lock().await;
    pending.retain(|_, p| p.expires > Instant::now());
    if pending.len() >= web.capacity {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    pending.insert(
        state,
        Pending {
            binding: binding.clone(),
            verifier,
            expires: Instant::now() + LOGIN_TTL,
        },
    );
    let mut r = Redirect::to(url.as_str()).into_response();
    set_cookie(&mut r, LOGIN, &binding, LOGIN_TTL.as_secs());
    r
}

#[derive(Deserialize)]
pub(crate) struct Callback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct Token {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

pub(crate) async fn callback(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<Callback>,
) -> Response {
    let mut success = complete(&st, &headers, q).await;
    if success.is_some()
        && let Some(a) = &st.accounts
        && let Some(token) = cookie(&headers, crate::accounts::http::COOKIE)
        && a.logout(token).await.is_err()
    {
        success = None;
    }
    let mut r = Redirect::to(if success.is_some() {
        "/app"
    } else {
        "/app?signin=failed"
    })
    .into_response();
    set_cookie(&mut r, LOGIN, "", 0);
    if let Some((id, ttl)) = success {
        set_cookie(&mut r, SESSION, &id, ttl);
        set_cookie(&mut r, crate::accounts::http::COOKIE, "", 0);
    }
    r
}

async fn complete(st: &AppState, headers: &HeaderMap, q: Callback) -> Option<(String, u64)> {
    let web = st.web.as_ref()?;
    let state = q.state?;
    let pending = {
        let mut all = web.pending.lock().await;
        all.retain(|_, p| p.expires > Instant::now());
        let p = all.get(&state)?;
        if cookie(headers, LOGIN) != Some(p.binding.as_str()) {
            return None;
        }
        all.remove(&state)? // One use, including provider refusals and failed exchanges.
    };
    if q.error.is_some() {
        return None;
    }
    let code = q.code.filter(|c| !c.is_empty() && c.len() <= 8192)?;
    let mut response = web
        .http
        .post(format!("{}/oauth/v2/token", web.issuer))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", web.client_id.as_str()),
            ("redirect_uri", &format!("{}/auth/callback", web.origin)),
            ("code", &code),
            ("code_verifier", &pending.verifier),
        ])
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 65536 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let token: Token = serde_json::from_slice(&bytes).ok()?;
    if !token.token_type.eq_ignore_ascii_case("bearer") || token.access_token.is_empty() {
        return None;
    }
    // Establish identity through the existing introspection and tenant checks, never
    // by trusting the token response's unverified ID-token claims.
    let mut auth = HeaderMap::new();
    auth.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", token.access_token)).ok()?,
    );
    let caller = st.caller(&auth).await.ok()?;
    let ttl = token.expires_in.min(web.session_ttl);
    if ttl == 0 {
        return None;
    }
    let id = random();
    if let Some(durable) = &web.durable {
        durable
            .put(
                &id,
                cookie(headers, SESSION),
                caller.user.as_deref()?,
                &token.access_token,
                ttl,
            )
            .await
            .ok()?;
        return Some((id, ttl));
    }
    let mut sessions = web.sessions.lock().await;
    sessions.retain(|_, s| s.expires > Instant::now());
    if sessions.len() >= web.capacity {
        return None;
    }
    if let Some(old) = cookie(headers, SESSION) {
        sessions.remove(old);
    }
    sessions.insert(
        id.clone(),
        Session {
            token: token.access_token,
            expires: Instant::now() + Duration::from_secs(ttl),
        },
    );
    Some((id, ttl))
}

pub(crate) async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(a) = &st.accounts {
        if !crate::accounts::http::same_origin(a, &headers) {
            return StatusCode::FORBIDDEN.into_response();
        }
        if let Some(token) = cookie(&headers, crate::accounts::http::COOKIE)
            && a.logout(token).await.is_err()
        {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    if let Some(web) = &st.web {
        if !web.same_origin(&headers) {
            return StatusCode::FORBIDDEN.into_response();
        }
        if web.revoke(&headers).await.is_err() {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if let Some(binding) = cookie(&headers, LOGIN) {
            web.pending.lock().await.retain(|_, p| p.binding != binding);
        }
    }
    let mut r = StatusCode::NO_CONTENT.into_response();
    set_cookie(&mut r, SESSION, "", 0);
    set_cookie(&mut r, crate::accounts::http::COOKIE, "", 0);
    set_cookie(&mut r, LOGIN, "", 0);
    r
}

/// Cookie credentials apply only to API routes. Explicit bearer credentials remain
/// independent for collectors. Cookie writes require the configured origin and a
/// non-simple header; no CORS permission is granted to other origins.
pub(crate) async fn browser_session(
    State(st): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let mut denied = false;
    let mut unavailable = false;
    if req.uri().path().starts_with("/v1/") && !req.headers().contains_key("authorization") {
        let native = st.accounts.as_ref().and_then(|a| {
            cookie(req.headers(), crate::accounts::http::COOKIE).map(|token| (a, token.to_string()))
        });
        let credential = if let Some((a, token)) = native {
            Some((
                Ok(Some(token)),
                crate::accounts::http::same_origin(a, req.headers()),
            ))
        } else if let Some(web) = &st.web {
            Some((
                web.token(req.headers()).await,
                web.same_origin(req.headers()),
            ))
        } else {
            None
        };
        if let Some((token, same_origin)) = credential {
            match token {
                Err(_) => unavailable = true,
                Ok(None) => {}
                Ok(Some(token)) => {
                    if req.method() != Method::GET && req.method() != Method::HEAD && !same_origin {
                        denied = true;
                    } else if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
                        req.headers_mut().insert("authorization", value);
                    }
                }
            }
        }
    }
    denied |= !crate::accounts::device_http::route_allowed(&req);
    let mut r = if unavailable {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"Session storage is unavailable. Please retry."})),
        )
            .into_response()
    } else if denied {
        StatusCode::FORBIDDEN.into_response()
    } else {
        next.run(req).await
    };
    r.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    r.headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    r
}

#[cfg(test)]
mod tests;
