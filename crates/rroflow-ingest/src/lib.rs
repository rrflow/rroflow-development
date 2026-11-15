//! rroflow-ingest: the rroflow.ai service that receives `rroflow.turn/v1` records from
//! collectors and files them into the sender's devspace.
//!
//! - **Tenancy:** organizations own projects, and a project is a devspace, the partition
//!   every monitoring table keys on. The credential decides what a request may reach: a key
//!   opens only its own project, and a member may name a project with a header that is
//!   checked against membership on every request. Keys are stored only as SHA-256 hashes.
//! - **Idempotence:** records upsert by `event_id`, so re-sending replaces, never duplicates.
//! - **Privacy:** the service never logs record content; content arrives only when the
//!   user opted in on the collector.
//! - **Store:** [`Store`]: Postgres when `database_url` is set, else JSONL files.
//! - **Signals:** the catalogue is public at `GET /v1/signals`; each devspace keeps its own
//!   choices (on/off, thresholds) at `/v1/signals/settings`, which collectors read before
//!   evaluating.

pub mod allowances;
mod analytics;
mod brand;
pub mod datasets;
mod member_console;
pub mod sdk_activity;
pub use brand::{BrandConfig, BrandLink};
pub mod accounts;
pub use accounts::AccountConfig;
mod secret_box;
mod web_auth;
pub use web_auth::BrowserConfig;
mod capture;
pub mod nomen;
pub mod permissions;
pub mod pg;
mod provenance;
mod reviews;
mod server;
mod settings;
mod source_health;
pub mod store;
pub mod tenancy;
#[cfg(test)]
mod test_keys;

pub use nomen::{AuthError, NomenVerifier, Subject};
pub use permissions::{Grant, Permission};
pub use pg::PgStore;
pub(crate) use server::internal;
pub use server::{
    ApiError, AppState, Caller, Config, Devspace, NomenConfig, hash_key, valid_devspace_id,
};
pub use store::{JsonlStore, Store, Summary, TurnFilter, Upserted};
pub use tenancy::{Membership, Role, TenancyConfig};

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use rroflow_core::signals::{CATALOGUE, Category, SignalConfig};
use rroflow_core::{SCHEMA, TurnRecord};
use serde::{Deserialize, Serialize};

/// Embedded at build time, independent of the serving host's checkout or environment.
#[derive(Serialize)]
pub struct BuildInfo {
    pub revision: &'static str,
    pub target: &'static str,
    pub migrations: &'static str,
}

pub fn build_info() -> BuildInfo {
    BuildInfo {
        revision: env!("RROFLOW_RELEASE_REVISION"),
        target: env!("RROFLOW_RELEASE_TARGET"),
        migrations: env!("RROFLOW_MIGRATIONS_SHA256"),
    }
}

async fn ready(State(st): State<AppState>) -> impl IntoResponse {
    match tokio::time::timeout(std::time::Duration::from_secs(2), st.store.ready()).await {
        Ok(Ok(())) => (StatusCode::OK, "ready"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "store unavailable"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnsBody {
    #[serde(deserialize_with = "provenance::deserialize_turns")]
    pub turns: Vec<TurnRecord>,
}

/// Check one record before it is stored. Returns the problem in plain words.
fn check_record(r: &TurnRecord) -> Result<(), String> {
    if r.schema != SCHEMA {
        return Err(format!("schema {:?} is not {SCHEMA}", r.schema));
    }
    if r.event_id.is_empty() {
        return Err("event_id is empty".into());
    }
    for (field, v) in [("started_at", &r.started_at), ("ended_at", &r.ended_at)] {
        if !v.is_empty() && pg::timestamp(v).is_none() {
            return Err(format!("{field} {v:?} is not an RFC 3339 timestamp"));
        }
    }
    Ok(())
}

async fn post_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<TurnsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = st.allowed(&headers, crate::Permission::ProjectSend).await?;
    let devspace = caller.devspace.clone();
    if body.turns.len() > st.max_batch {
        return Err(ApiError::BadRequest(format!(
            "batch of {} exceeds the limit of {}",
            body.turns.len(),
            st.max_batch
        )));
    }
    for (i, r) in body.turns.iter().enumerate() {
        check_record(r).map_err(|m| ApiError::BadRequest(format!("turns[{i}]: {m}")))?;
    }
    let settings = st.store.signal_config(&devspace).await.map_err(internal)?;
    for turn in &mut body.turns {
        // This namespace belongs to the receiving devspace, never the submitting client.
        turn.signals
            .retain(|hit| !hit.signal.starts_with("custom_"));
        for signal in settings.custom.iter().filter(|s| s.enabled) {
            if let rroflow_core::custom::Evaluation::Match(hit) = signal.evaluate(turn) {
                turn.signals.push(hit);
            }
        }
    }
    let up = match st.store.as_ref() {
        Store::Postgres(pg) => {
            pg.upsert_received(&devspace, &body.turns, Some(&caller))
                .await
        }
        Store::Jsonl(_) => st.store.upsert(&devspace, &body.turns).await,
    }
    .map_err(provenance::ingest_error)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "accepted": body.turns.len(),
        "new": up.new,
        "replaced": up.replaced,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub agent: Option<String>,
    pub signal: Option<String>,
    pub project: Option<String>,
    pub session: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub cursor: Option<String>,
    pub event: Option<String>,
}

/// Most records one read returns.
const LIST_MAX: usize = 1000;

async fn get_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let limit = q.limit.unwrap_or(50).clamp(1, LIST_MAX);
    let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if search.is_some_and(|s| s.chars().count() > 200) {
        return Err(ApiError::BadRequest(
            "Search must be 200 characters or fewer".into(),
        ));
    }
    let since = q
        .since
        .as_deref()
        .map(|value| {
            pg::timestamp(value).ok_or_else(|| ApiError::BadRequest("Invalid start date".into()))
        })
        .transpose()?;
    let until = q
        .until
        .as_deref()
        .map(|value| {
            pg::timestamp(value).ok_or_else(|| ApiError::BadRequest("Invalid end date".into()))
        })
        .transpose()?;
    if since.zip(until).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::BadRequest("Start must be before end".into()));
    }
    let cursor: Option<(String, String)> = q
        .cursor
        .as_deref()
        .map(|value| {
            if value.len() > 4096 {
                return Err(ApiError::BadRequest("Invalid page cursor".into()));
            }
            URL_SAFE_NO_PAD
                .decode(value)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .ok_or_else(|| ApiError::BadRequest("Invalid page cursor".into()))
        })
        .transpose()?;
    let before = cursor
        .as_ref()
        .map(|(at, id)| {
            if id.is_empty() || (!at.is_empty() && pg::timestamp(at).is_none()) {
                return Err(ApiError::BadRequest("Invalid page cursor".into()));
            }
            Ok((pg::timestamp(at), id.as_str()))
        })
        .transpose()?;
    let mut turns = st
        .store
        .list(
            &devspace,
            limit + 1,
            &TurnFilter {
                agent: q.agent.as_deref().filter(|a| !a.is_empty()),
                signal: q.signal.as_deref().filter(|s| !s.is_empty()),
                project: q.project.as_deref().filter(|s| !s.is_empty()),
                session: q.session.as_deref().filter(|s| !s.is_empty()),
                search,
                since,
                until,
                before,
                event: q.event.as_deref().filter(|s| !s.is_empty()),
            },
        )
        .await
        .map_err(internal)?;
    let has_more = turns.len() > limit;
    turns.truncate(limit);
    let next_cursor = if has_more {
        turns.last().map(|turn| {
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&(
                    pg::timestamp(&turn.started_at)
                        .map(|at| at.to_rfc3339())
                        .unwrap_or_default(),
                    &turn.event_id,
                ))
                .expect("string tuple serializes"),
            )
        })
    } else {
        None
    };
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "next_cursor": next_cursor, "revisions":turns.iter().map(|t| (t.event_id.clone(), reviews::digest(t))).collect::<std::collections::BTreeMap<_,_>>(), "turns": turns }),
    ))
}

/// Schema id of the signal catalogue document.
pub const SIGNALS_SCHEMA: &str = "rroflow.signals/v1";

/// The signal catalogue, grouped: every category with its signals in catalogue order.
fn catalogue() -> serde_json::Value {
    serde_json::json!({
        "schema": SIGNALS_SCHEMA,
        "categories": Category::ALL
            .iter()
            .map(|c| serde_json::json!({
                "id": c,
                "label": c.label(),
                "meaning": c.meaning(),
                "signals": CATALOGUE.iter().filter(|s| s.category == *c).map(|s| s.id).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "signals": CATALOGUE,
    })
}

async fn get_signals() -> Json<serde_json::Value> {
    Json(catalogue())
}

/// Collectors read this before evaluating, including with a write-only key. Custom rules run
/// on the server and can describe private work, so that key does not receive them.
async fn get_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = st.authenticate(&headers).await?;
    let reader = caller.grant().allows(Permission::ProjectRead);
    if !reader {
        permissions::require(caller.grant(), Permission::ProjectSend)?;
    }
    let mut settings = st
        .store
        .signal_config(&caller.devspace)
        .await
        .map_err(internal)?;
    if !reader {
        settings.custom.clear();
    }
    Ok(Json(
        serde_json::json!({ "devspace": caller.devspace, "settings": settings }),
    ))
}

async fn put_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(mut cfg): Json<SignalConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectWrite)
        .await?
        .devspace;
    cfg.validate().map_err(ApiError::BadRequest)?;
    let old = st.store.signal_config(&devspace).await.map_err(internal)?;
    if old.revision != cfg.revision {
        return Err(ApiError::SettingsConflict);
    }
    cfg.validate_update(&old).map_err(ApiError::BadRequest)?;
    let saved = st
        .store
        .set_signal_config(&devspace, &cfg)
        .await
        .map_err(internal)?;
    if !saved {
        return Err(ApiError::SettingsConflict);
    }
    cfg.revision += 1;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "settings": cfg }),
    ))
}

async fn preview_signal(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(rule): Json<rroflow_core::custom::CustomSignal>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    rule.validate().map_err(ApiError::BadRequest)?;
    let turns = st
        .store
        .list(
            &devspace,
            101,
            &store::TurnFilter {
                agent: rule.agent.as_deref(),
                project: rule.project.as_deref(),
                ..Default::default()
            },
        )
        .await
        .map_err(internal)?;
    let mut matches = vec![];
    let mut evaluated = 0;
    let mut unavailable = 0;
    for turn in turns.iter().take(100) {
        match rule.evaluate(turn) {
            rroflow_core::custom::Evaluation::Match(hit) => {
                evaluated += 1;
                matches.push(serde_json::json!({"event_id":turn.event_id,"hit":hit}));
            }
            rroflow_core::custom::Evaluation::NoMatch => evaluated += 1,
            rroflow_core::custom::Evaluation::Unavailable => unavailable += 1,
            rroflow_core::custom::Evaluation::OutOfScope => {}
        }
    }
    Ok(Json(
        serde_json::json!({"devspace":devspace,"scanned":turns.len().min(100),"evaluated":evaluated,"unavailable":unavailable,"more_history":turns.len()>100,"matches":matches}),
    ))
}

async fn get_summary(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    let (summary, settings) = tokio::try_join!(
        st.store.summary(&devspace),
        st.store.signal_config(&devspace)
    )
    .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "summary": summary,
        "catalogue": catalogue(),
        "settings": settings,
    })))
}

/// Static public product page; no third-party scripts or client-side provider credentials.
async fn public_site(State(st): State<AppState>) -> impl IntoResponse {
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
async fn dashboard(State(st): State<AppState>) -> impl IntoResponse {
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
async fn research_notes(
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
        include_str!("research.json"),
    ))
}

/// The HTTP surface: `GET /` (public), `GET /app` (dashboard), `POST|GET /v1/turns`, `GET /v1/summary`,
/// `GET /v1/signals`, `GET|PUT /v1/signals/settings`, `GET /v1/whoami`, `GET /healthz`, and the
/// account, organization and project routes in [`tenancy`].
pub fn router(st: AppState, max_body_bytes: usize) -> Router {
    Router::new()
        .merge(
            Router::new()
                .route("/auth/native/config", get(accounts::http::config))
                .route("/auth/native/request", post(accounts::http::request))
                .route("/auth/native/login", post(accounts::http::login))
                .route("/auth/native/complete", post(accounts::http::complete))
                .route("/auth/native/first-run", post(accounts::http::first_run))
                .route(
                    "/auth/native/password/status",
                    post(accounts::http::password_status),
                )
                .route(
                    "/auth/native/password/saved",
                    post(accounts::http::password_saved),
                )
                .route(
                    "/auth/native/password/replace",
                    post(accounts::http::password_replace),
                )
                .route(
                    "/auth/native/device/config",
                    get(accounts::device_http::config),
                )
                .route(
                    "/auth/native/device/start",
                    post(accounts::device_http::start),
                )
                .route(
                    "/auth/native/device/token",
                    post(accounts::device_http::token),
                )
                .route(
                    "/auth/native/device/revoke",
                    post(accounts::device_http::revoke),
                )
                .route(
                    "/auth/native/device/inspect",
                    post(accounts::device_http::inspect),
                )
                .route(
                    "/auth/native/device/decide",
                    post(accounts::device_http::decide),
                )
                .route(
                    "/auth/native/device/grants",
                    post(accounts::device_http::grants),
                )
                .route(
                    "/auth/native/device/disconnect",
                    post(accounts::device_http::disconnect),
                )
                .layer(axum::extract::DefaultBodyLimit::max(4096)),
        )
        .route("/auth/config", get(web_auth::config))
        .route("/auth/login", get(web_auth::login))
        .route("/auth/federation", get(web_auth::federation_login))
        .route("/auth/callback", get(web_auth::callback))
        .route("/auth/logout", post(web_auth::logout))
        .route("/v1/events", post(capture::ingest).get(capture::list))
        .route("/v1/reviews", post(reviews::save).get(reviews::list))
        .route("/v1/turns", post(post_turns).get(get_turns))
        .route("/v1/summary", get(get_summary))
        .route("/v1/whoami", get(server::routes::get_whoami))
        .route("/v1/provenance", get(provenance::get))
        .route("/v1/provenance/coverage", get(provenance::coverage))
        .route("/v1/me", get(tenancy::me))
        .route(
            "/v1/instance/settings",
            get(settings::get).put(settings::update),
        )
        .route("/v1/organizations", post(tenancy::create_organization))
        .route(
            "/v1/organizations/{organization}/projects",
            post(tenancy::create_project),
        )
        .route(
            "/v1/organizations/{organization}/members",
            get(tenancy::members),
        )
        .route(
            "/v1/organizations/{organization}/members/{user}",
            put(tenancy::set_role).delete(tenancy::remove_member),
        )
        .route(
            "/v1/organizations/{organization}/invitations",
            get(tenancy::invitations).post(tenancy::invite),
        )
        .route(
            "/v1/organizations/{organization}/invitations/{invitation}",
            delete(tenancy::revoke_invitation),
        )
        .route(
            "/v1/organizations/{organization}/audit",
            get(tenancy::audit_log),
        )
        .route("/v1/invitations/accept", post(tenancy::accept_invitation))
        .route(
            "/v1/projects/{project}/keys",
            get(tenancy::keys).post(tenancy::create_key),
        )
        .route(
            "/v1/projects/{project}/keys/{key}",
            delete(tenancy::revoke_key),
        )
        .route("/v1/allowances", get(allowances::status))
        .route("/v1/sdk/activity", get(sdk_activity::list))
        .route("/v1/sdk/summary", get(sdk_activity::summary))
        .route(
            "/v1/sdk/reviews",
            get(sdk_activity::reviews).post(sdk_activity::save_review),
        )
        .route("/v1/datasets", get(datasets::status))
        .route("/v1/datasets/policy", axum::routing::put(datasets::save))
        .route("/v1/datasets/jobs/{id}/artifact", get(datasets::artifact))
        .route(
            "/assets/dashboard-datasets.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard-datasets.js"),
                )
            }),
        )
        .route(
            "/assets/dashboard-sdk.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard-sdk.js"),
                )
            }),
        )
        .route("/v1/signals", get(get_signals))
        .route("/v1/signals/stats", get(analytics::get))
        .route("/v1/signals/preview", post(preview_signal))
        .route(
            "/v1/sources/health",
            get(source_health::list).post(source_health::ingest),
        )
        .route(
            "/v1/signals/settings",
            get(get_signal_settings).put(put_signal_settings),
        )
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/build-info", get(|| async { Json(build_info()) }))
        .route("/", get(public_site))
        .route("/app", get(dashboard))
        .route("/app/", get(dashboard))
        .route("/app/manage", get(member_console::page))
        .route("/app/account", get(member_console::page))
        .route("/app/device", get(member_console::page))
        .route("/app/settings", get(member_console::page))
        .route("/assets/member-console/app.js", get(member_console::script))
        .route(
            "/assets/member-console/index.css",
            get(member_console::style),
        )
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
            get(|| async {
                (
                    [
                        ("content-type", "text/css; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("public.css"),
                )
            }),
        )
        .route(
            "/assets/public.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("public.js"),
                )
            }),
        )
        .route(
            "/assets/dashboard.css",
            get(|| async {
                (
                    [
                        ("content-type", "text/css; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard.css"),
                )
            }),
        )
        .route(
            "/assets/dashboard-a2ui.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard-a2ui.js"),
                )
            }),
        )
        .route(
            "/assets/dashboard.js",
            get(|| async {
                (
                    [
                        ("content-type", "text/javascript; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    include_str!("dashboard.js"),
                )
            }),
        )
        .route("/assets/research.json", get(research_notes))
        .layer(axum::middleware::from_fn(permissions::enforce))
        .layer(axum::middleware::from_fn_with_state(
            st.clone(),
            web_auth::browser_session,
        ))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(st)
}

#[cfg(test)]
mod tests;
