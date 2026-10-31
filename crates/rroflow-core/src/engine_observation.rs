//! Collector-reported references to public RRFlow observations, never lifecycle authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineObservation {
    pub version: u32,
    pub instance_id: String,
    pub scope: String,
    /// Observed public operation; the source exporter validates the engine catalogue.
    pub operation: String,
    /// SHA-256 of the exact observed public response bytes retained by the exporter.
    pub response_sha256: String,
    pub task_id: Option<String>,
    pub attempt_id: Option<String>,
    pub job_id: Option<String>,
    pub plan_sha256: Option<String>,
    pub source_sha256: Option<String>,
    pub configuration_sha256: Option<String>,
}

impl EngineObservation {
    pub fn validate(&self) -> Result<(), &'static str> {
        let id = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.:/".contains(&c))
        };
        let digest = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        };
        if self.version != 1
            || !id(&self.instance_id)
            || !id(&self.scope)
            || !id(&self.operation)
            || [&self.task_id, &self.attempt_id, &self.job_id]
                .into_iter()
                .any(|v| v.as_deref().is_some_and(|s| !id(s)))
        {
            return Err("invalid engine observation identity");
        }
        if (self.task_id.is_none() && self.job_id.is_none())
            || (self.attempt_id.is_some() && self.task_id.is_none())
        {
            return Err("engine observation requires a task or job; attempts require their task");
        }
        if !digest(&self.response_sha256)
            || [
                &self.plan_sha256,
                &self.source_sha256,
                &self.configuration_sha256,
            ]
            .into_iter()
            .any(|v| v.as_deref().is_some_and(|s| !digest(s)))
        {
            return Err("invalid engine observation digest");
        }
        Ok(())
    }
}
