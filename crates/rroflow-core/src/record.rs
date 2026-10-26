//! `rroflow.turn/v1`: one record per agent turn (one prompt and everything the agent did
//! until it answered, stopped or was interrupted).
//!
//! Design rules, from the DevPulse notes (devpulselabs `AiEvent`, signal doctrine):
//! - Tokens are truth; cost is derived later from a versioned price table, never stored here.
//! - Every token count is optional: `None` means "the provider did not say", never zero.
//! - Content (prompt, response, tool input and output text) is optional and opt-in. Metrics
//!   and signal results never need it.

use serde::{Deserialize, Serialize};

/// Schema identifier carried by every record.
pub const SCHEMA: &str = "rroflow.turn/v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub schema: String,
    /// Deterministic id (UUIDv5 of session and turn key): re-sending replaces, never duplicates.
    pub event_id: String,
    /// `claude-code` or `codex`.
    pub agent: String,
    pub agent_version: String,
    /// Model provider, e.g. `anthropic`, `openai`.
    pub provider: String,
    pub model: Option<String>,
    pub session_id: String,
    pub turn_key: String,
    /// The subagent this turn belongs to, e.g. `/root/review`.
    pub subagent: Option<String>,
    /// Set when another agent, not a person, gave the prompt.
    pub prompt_author: Option<String>,
    /// RFC 3339.
    pub started_at: String,
    pub ended_at: String,
    pub cwd: String,
    pub git_branch: String,
    /// Project within the devspace (e.g. `shippinai`, `rrflow`, `clyffy`).
    pub project: Option<String>,
    pub usage: Usage,
    pub termination: Termination,
    /// The turn resumed after the agent compacted its context.
    pub compacted: bool,
    /// Finished tool calls only; unfinished ones are counted and sent by a later run.
    pub tools: Vec<ToolRecord>,
    pub tools_unfinished: u32,
    /// Prompt and response text, present only when the user opted in to sending content.
    pub content: Option<Content>,
    pub signals: Vec<SignalHit>,
    /// The collector that produced the record, e.g. `rroflow/0.2.0`.
    pub collector: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    /// Whether `input_tokens` already includes cached input. OpenAI counts it in; Anthropic
    /// reports it separately. Pricing needs to know which.
    pub input_includes_cache: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    /// The agent answered.
    Completed,
    /// The last finished tool call failed and nothing followed it.
    ToolError,
    /// The person interrupted the turn.
    UserCancel,
    /// Still running, or the transcript does not say.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Ok,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolRecord {
    pub id: String,
    pub name: String,
    pub started_at: String,
    pub ended_at: String,
    pub status: ToolStatus,
    /// Present only with content opt-in.
    pub input: Option<String>,
    /// Present only with content opt-in.
    pub output: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Content {
    pub input: String,
    pub output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Critical,
}

/// One behaviour signal that matched a turn, with the evidence that made it match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalHit {
    /// Stable signal id, e.g. `blocked_handoff`.
    pub signal: String,
    /// Version of the rule that produced the hit; a changed rule gets a new version.
    pub rule_version: u32,
    pub severity: Severity,
    /// Plain-language reason, readable by a non-developer. Never contains content.
    pub summary: String,
    /// Optional numeric score (e.g. mood from -2 to +2).
    pub score: Option<f64>,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Prompt,
    Response,
    Tool,
    Turn,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// The tool call this evidence points at, when `kind` is `tool`.
    pub tool_id: Option<String>,
    /// The matched text. Dropped together with content when content is not sent.
    pub excerpt: Option<String>,
}

impl TurnRecord {
    /// Remove every piece of text (prompt, response, tool input and output, evidence
    /// excerpts), keeping metrics, tool names and statuses, and signal results.
    pub fn strip_content(&mut self) {
        self.content = None;
        for t in &mut self.tools {
            t.input = None;
            t.output = None;
        }
        for s in &mut self.signals {
            for e in &mut s.evidence {
                e.excerpt = None;
            }
        }
    }
}
