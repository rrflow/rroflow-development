//! Turn records: receiving them from collectors and reading them back page by page. See
//! `docs/spec/turns-v1.md`.
mod model;
mod routes;
pub(crate) use routes::routes;

pub use model::{ListQuery, TurnsBody};
