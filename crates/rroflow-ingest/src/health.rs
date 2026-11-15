//! Liveness, readiness and the build identity. See `docs/spec/health-v1.md`.
mod routes;
pub(crate) use routes::routes;

use serde::Serialize;

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

#[cfg(test)]
mod tests;
