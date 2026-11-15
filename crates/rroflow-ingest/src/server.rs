//! Request plumbing every feature shares: the service configuration, the application state,
//! the caller a credential proves, and the answer a refused request gets. See
//! `docs/spec/server-v1.md`.
mod caller;
mod config;
mod error;
pub(crate) mod routes;
mod state;

pub use caller::Caller;
pub use config::{Config, Devspace, NomenConfig, hash_key, valid_devspace_id};
pub use error::ApiError;
pub(crate) use error::internal;
pub use state::AppState;
