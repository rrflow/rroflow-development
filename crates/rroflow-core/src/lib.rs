//! rroflow-core: the turn record every collector produces (`rroflow.turn/v1`) and the
//! deterministic behaviour signals evaluated over it.
//!
//! The record is agent-neutral: Claude Code, Codex and future agents map onto it.
//! Signals run where the content is (on the user's machine, before upload), so a
//! devspace can receive signal results and metrics without receiving prompt text.

pub mod capture;
pub mod record;
pub mod signals;

pub use record::{
    Content, Evidence, EvidenceKind, SCHEMA, Severity, SignalHit, Termination, ToolRecord,
    ToolStatus, TurnRecord, Usage,
};
