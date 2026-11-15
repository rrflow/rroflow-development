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
//!
//! Each feature registers its own routes in its `routes.rs`; [`router`] only assembles them.
//! See `docs/spec/code-structure-v1.md`.

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
mod site;
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

use axum::Router;
use axum::extract::DefaultBodyLimit;

/// The HTTP surface, assembled from every feature's routes. Each route is declared in
/// [`permissions::ROUTES`], which [`permissions::enforce`] applies before any handler runs.
pub fn router(st: AppState, max_body_bytes: usize) -> Router {
    Router::new()
        .merge(accounts::routes())
        .merge(web_auth::routes())
        .merge(capture::routes())
        .merge(reviews::routes())
        .merge(turns::routes())
        .merge(server::routes())
        .merge(provenance::routes())
        .merge(tenancy::routes())
        .merge(settings::routes())
        .merge(allowances::routes())
        .merge(sdk_activity::routes())
        .merge(datasets::routes())
        .merge(signals::routes())
        .merge(source_health::routes())
        .merge(health::routes())
        .merge(site::routes())
        .merge(member_console::routes())
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
