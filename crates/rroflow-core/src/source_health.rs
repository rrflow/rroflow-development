//! Latest collector observations, separate from captured activity and uptime claims.
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "rroflow.source-health/v1";
pub const MAX_SEQUENCE: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReport {
    pub schema: String,
    pub source_id: String,
    pub sequence: u64,
    pub observed_at: String,
    pub collector_version: String,
    pub pending_revisions: u64,
    pub pending_bytes: u64,
    pub journal_bytes: u64,
    pub oldest_pending_at: Option<String>,
    pub capture_errors: u32,
    /// None until this process has attempted delivery; never inferred from queue size.
    pub delivery_ok: Option<bool>,
    pub signal_settings_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_capture: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk: Option<SdkReport>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SdkReport {
    pub linked_queues: u32,
    pub pending_events: u64,
    pub pending_bytes: u64,
    pub retained_bytes: u64,
    pub retrying_queues: u32,
    pub queue_errors: u32,
}

impl SourceReport {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(sdk) = &self.sdk
            && (sdk.linked_queues > 16
                || sdk.retrying_queues > sdk.linked_queues
                || sdk.queue_errors > 16
                || sdk.pending_bytes > sdk.retained_bytes
                || (sdk.pending_events == 0 && sdk.pending_bytes != 0)
                || [sdk.pending_events, sdk.pending_bytes, sdk.retained_bytes]
                    .iter()
                    .any(|n| *n > MAX_SEQUENCE))
        {
            return Err("invalid SDK queue report");
        }
        let id = self.source_id.as_bytes();
        if self.schema != SCHEMA
            || id.len() != 36
            || !id.iter().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    *c == b'-'
                } else {
                    c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
                }
            })
        {
            return Err("invalid source report identity");
        }
        if self.sequence == 0
            || [
                self.sequence,
                self.pending_revisions,
                self.pending_bytes,
                self.journal_bytes,
                self.signal_settings_revision,
            ]
            .iter()
            .any(|n| *n > MAX_SEQUENCE)
        {
            return Err("source report counter out of range");
        }
        if self.pending_bytes > self.journal_bytes
            || (self.pending_revisions == 0
                && (self.pending_bytes != 0 || self.oldest_pending_at.is_some()))
        {
            return Err("inconsistent source queue counts");
        }
        if self.collector_version.is_empty()
            || self.collector_version.len() > 80
            || !self
                .collector_version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-+_".contains(&c))
        {
            return Err("invalid collector version");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceObservation {
    pub report: SourceReport,
    /// Assigned once by ingest for this sequence. Identical replay preserves it.
    pub received_at: String,
}
