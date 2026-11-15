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
mod health;
pub mod nomen;
pub mod permissions;
pub mod pg;
mod provenance;
mod reviews;
mod server;
mod settings;
mod signals;
mod source_health;
pub mod store;
pub mod tenancy;
#[cfg(test)]
mod test_keys;
mod turns;

pub use health::{BuildInfo, build_info};
pub use nomen::{AuthError, NomenVerifier, Subject};
pub use permissions::{Grant, Permission};
pub use pg::PgStore;
pub(crate) use server::internal;
pub use server::{
    ApiError, AppState, Caller, Config, Devspace, NomenConfig, hash_key, valid_devspace_id,
};
pub use signals::SIGNALS_SCHEMA;
pub use store::{JsonlStore, Store, Summary, TurnFilter, Upserted};
pub use tenancy::{Membership, Role, TenancyConfig};
pub use turns::{ListQuery, TurnsBody};

use axum::extract::{DefaultBodyLimit, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

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
        .route(
            "/v1/turns",
            post(turns::routes::post_turns).get(turns::routes::get_turns),
        )
        .route("/v1/summary", get(turns::routes::get_summary))
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
        .route("/v1/signals", get(signals::routes::get_signals))
        .route("/v1/signals/stats", get(signals::stats::get))
        .route("/v1/signals/preview", post(signals::routes::preview_signal))
        .route(
            "/v1/sources/health",
            get(source_health::list).post(source_health::ingest),
        )
        .route(
            "/v1/signals/settings",
            get(signals::routes::get_signal_settings).put(signals::routes::put_signal_settings),
        )
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(health::routes::ready))
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
