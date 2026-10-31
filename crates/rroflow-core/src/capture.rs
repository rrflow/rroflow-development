//! Lossless event boundaries, separate from aggregated turn projections.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CAPTURE_SCHEMA: &str = "rroflow.capture/v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CaptureEvent {
    pub schema: String,
    pub event_id: String,
    pub capture_id: String,
    /// Monotonic within capture_id; no ordering claim across different producers.
    pub sequence: u64,
    pub agent: String,
    pub session_id: String,
    pub operation_id: String,
    pub parent_operation_id: Option<String>,
    pub observed_at: String,
    pub kind: String,
    pub native_type: Option<String>,
    pub model: Option<String>,
    pub native_id: Option<String>,
    pub parent_native_id: Option<String>,
    pub native_status: Option<String>,
    pub tool_name: Option<String>,
    pub exit_code: Option<i32>,
    /// Bounded structural facts, independent of private prompt/tool payloads.
    /// Absent on older collectors; absence never implies success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<NativeFacts>,
    /// Native event usage snapshot; never summed blindly across stream events.
    pub usage: Option<crate::Usage>,
    /// Original event/body, present only with explicit content opt-in and redacted.
    pub payload: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NativeFacts {
    pub version: u32,
    pub item_type: Option<String>,
    pub event_type: Option<String>,
    pub object_type: Option<String>,
    pub stop_reason: Option<String>,
    pub is_error: Option<bool>,
    pub is_interrupt: Option<bool>,
}

impl CaptureEvent {
    pub fn validate(&self) -> Result<(), &'static str> {
        let id = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.:/".contains(&c))
        };
        let uuid = |s: &str| {
            s.len() == 36
                && s.bytes().enumerate().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == b'-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                })
        };
        if self.schema != CAPTURE_SCHEMA {
            return Err("unsupported capture schema");
        }
        if !uuid(&self.event_id) || !uuid(&self.capture_id) {
            return Err("event and capture IDs must be UUIDs");
        }
        if !["codex", "claude", "grok"].contains(&self.agent.as_str()) {
            return Err("unsupported capture agent");
        }
        if !id(&self.session_id)
            || !id(&self.operation_id)
            || self.parent_operation_id.as_deref().is_some_and(|s| !id(s))
        {
            return Err("invalid correlation ID");
        }
        if ![
            "call.started",
            "call.completed",
            "call.failed",
            "stream.event",
            "stream.completed",
            "stream.cancelled",
            "tool.started",
            "tool.completed",
            "tool.failed",
            "hook.event",
        ]
        .contains(&self.kind.as_str())
        {
            return Err("unknown capture kind");
        }
        if self.native_type.as_deref().is_some_and(|s| !id(s)) {
            return Err("invalid native event type");
        }
        if self.model.as_deref().is_some_and(|s| !id(s)) {
            return Err("invalid model name");
        }
        if [
            &self.native_id,
            &self.parent_native_id,
            &self.native_status,
            &self.tool_name,
        ]
        .into_iter()
        .any(|v| v.as_deref().is_some_and(|s| !id(s)))
        {
            return Err("invalid native metadata");
        }
        if self.observed_at.len() > 64 {
            return Err("invalid observation timestamp");
        }
        if let Some(n) = &self.native
            && (n.version != 1
                || [&n.item_type, &n.event_type, &n.object_type, &n.stop_reason]
                    .into_iter()
                    .any(|v| v.as_deref().is_some_and(|s| !id(s))))
        {
            return Err("invalid native facts");
        }
        Ok(())
    }
}
