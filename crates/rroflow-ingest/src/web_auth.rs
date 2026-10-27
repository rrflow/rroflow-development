//! Browser login. OAuth credentials stay in memory on this server; the browser gets
//! an opaque, host-only, HttpOnly cookie. A restart requires signing in again.
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

const SESSION: &str = "__Host-rroflow-session";
const LOGIN: &str = "__Host-rroflow-login";
const CAPACITY: usize = 4096;
const LOGIN_TTL: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowserConfig {
    /// Canonical HTTPS origin, also registered in Nomen with /auth/callback.
    pub origin: String,
    /// Public PKCE dashboard client. Must be allowed by nomen.clients.
    pub client_id: String,
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
}

fn https_origin(s: &str) -> bool {
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
    pub(crate) fn new(cfg: &Config) -> anyhow::Result<Option<Arc<Self>>> {
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
        })))
    }
    fn same_origin(&self, headers: &HeaderMap) -> bool {
        headers.get("origin").and_then(|v| v.to_str().ok()) == Some(self.origin.as_str())
            && headers
                .get("x-rroflow-request")
                .and_then(|v| v.to_str().ok())
                == Some("dashboard")
    }
    async fn token(&self, headers: &HeaderMap) -> Option<String> {
        let id = cookie(headers, SESSION)?;
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, s| s.expires > Instant::now());
        sessions.get(id).map(|s| s.token.clone())
    }
}

fn random() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn cookie<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
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
fn set_cookie(r: &mut Response, name: &str, value: &str, seconds: u64) {
    r.headers_mut().append(
        "set-cookie",
        HeaderValue::from_str(&format!(
            "{name}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={seconds}"
        ))
        .expect("cookie values are generated base64url"),
    );
}

pub(crate) async fn config(State(st): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"enabled": st.web.is_some()}))
}

pub(crate) async fn login(State(st): State<AppState>) -> Response {
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
    if pending.len() >= CAPACITY {
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
    let success = complete(&st, &headers, q).await;
    let mut r = Redirect::to(if success.is_some() {
        "/"
    } else {
        "/?signin=failed"
    })
    .into_response();
    set_cookie(&mut r, LOGIN, "", 0);
    if let Some((id, ttl)) = success {
        set_cookie(&mut r, SESSION, &id, ttl);
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
    st.caller(&auth).await.ok()?;
    let ttl = token.expires_in.min(3600);
    if ttl == 0 {
        return None;
    }
    let id = random();
    let mut sessions = web.sessions.lock().await;
    sessions.retain(|_, s| s.expires > Instant::now());
    if sessions.len() >= CAPACITY {
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
    if let Some(web) = &st.web {
        if !web.same_origin(&headers) {
            return StatusCode::FORBIDDEN.into_response();
        }
        if let Some(id) = cookie(&headers, SESSION) {
            web.sessions.lock().await.remove(id);
        }
        if let Some(binding) = cookie(&headers, LOGIN) {
            web.pending.lock().await.retain(|_, p| p.binding != binding);
        }
    }
    let mut r = StatusCode::NO_CONTENT.into_response();
    set_cookie(&mut r, SESSION, "", 0);
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
    if req.uri().path().starts_with("/v1/")
        && !req.headers().contains_key("authorization")
        && let Some(web) = &st.web
        && let Some(token) = web.token(req.headers()).await
    {
        if req.method() != Method::GET
            && req.method() != Method::HEAD
            && !web.same_origin(req.headers())
        {
            denied = true;
        } else if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            req.headers_mut().insert("authorization", value);
        }
    }
    let mut r = if denied {
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
