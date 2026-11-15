//! Signals: the public catalogue, each project's choices, previewing a custom rule, and signal
//! statistics. See `docs/spec/signals-v1.md`.
mod model;
pub(crate) mod routes;
pub(crate) mod stats;

pub use model::SIGNALS_SCHEMA;
pub(crate) use model::catalogue;
