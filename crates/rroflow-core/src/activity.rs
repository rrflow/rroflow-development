//! Reproducible metadata projections of immutable SDK events. No storage or network.
//! Operations are SDK calls/tools, not inferred human turns or provider-internal calls.
use crate::{Usage, capture::CaptureEvent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "rroflow.sdk-activity/v1";
pub const PROJECTION_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Failed,
    Cancelled,
    Partial,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventRef {
    pub event_id: String,
    pub sequence: u64,
    pub kind: String,
    pub native_type: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub signal: String,
    pub rule_version: u32,
    pub summary: String,
    pub event_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub native_id: String,
    pub kind: String,
    pub name: Option<String>,
    pub outcome: Outcome,
    pub event_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub schema: String,
    pub projection_version: u32,
    pub activity_id: String,
    pub capture_id: String,
    pub operation_id: String,
    pub parent_activity_id: Option<String>,
    pub agent: String,
    pub session_id: String,
    /// call, tool or observation. An operation is not necessarily a human turn.
    pub kind: String,
    pub outcome: Outcome,
    pub outcome_basis: String,
    pub provider_outcome: Option<Outcome>,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub models: Vec<String>,
    /// One unambiguous terminal snapshot only. Deltas are never added together.
    pub usage: Option<Usage>,
    pub usage_event_id: Option<String>,
    pub tools: Vec<Tool>,
    pub findings: Vec<Finding>,
    pub events: Vec<EventRef>,
    pub unsupported_event_ids: Vec<String>,
    pub coverage_notes: Vec<String>,
}

pub fn identity(capture: &str, operation: &str) -> String {
    let encoded = serde_json::to_vec(&(capture, operation)).expect("identity");
    format!("sdk-{:x}", Sha256::digest(encoded))
}
pub fn revision(activity: &Activity) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(activity).expect("activity"))
    )
}

/// Caller supplies a complete received cohort, not a sliced page of events.
/// Ingestion order and identical replay do not affect projections. Conflicting
/// identities are refused; silently choosing one would destroy evidence.
pub fn project(events: &[CaptureEvent]) -> Result<Vec<Activity>, &'static str> {
    let mut ids = BTreeMap::new();
    let mut sequences = BTreeMap::new();
    let mut groups: BTreeMap<(&str, &str), Vec<&CaptureEvent>> = BTreeMap::new();
    for event in events {
        event.validate()?;
        if let Some(old) = ids.insert(event.event_id.as_str(), event) {
            if old != event {
                return Err("conflicting event identity");
            }
            continue;
        }
        if sequences
            .insert((&event.capture_id, event.sequence), &event.event_id)
            .is_some()
        {
            return Err("conflicting capture sequence");
        }
        groups
            .entry((&event.capture_id, &event.operation_id))
            .or_default()
            .push(event);
    }
    let mut activities: Vec<Activity> = groups
        .into_values()
        .map(|mut rows| {
            rows.sort_by_key(|e| e.sequence);
            project_operation(&rows)
        })
        .collect::<Result<_, _>>()?;
    let parents: BTreeMap<_, _> = activities
        .iter()
        .map(|a| (a.activity_id.clone(), a.parent_activity_id.clone()))
        .collect();
    for a in &mut activities {
        let mut seen = BTreeSet::from([a.activity_id.clone()]);
        let mut parent = a.parent_activity_id.clone();
        while let Some(id) = parent {
            if !seen.insert(id.clone()) {
                return Err("cyclic operation parents");
            }
            match parents.get(&id) {
                Some(next) => parent = next.clone(),
                None => {
                    a.coverage_notes.push("ancestor_not_observed".into());
                    break;
                }
            }
        }
        a.coverage_notes.sort();
    }
    Ok(activities)
}

fn finding(signal: &str, summary: &str, ids: Vec<String>) -> Finding {
    Finding {
        signal: signal.into(),
        rule_version: 1,
        summary: summary.into(),
        event_ids: ids,
    }
}
fn facts_error(e: &CaptureEvent) -> Option<bool> {
    e.native.as_ref().and_then(|n| n.is_error)
}
fn item_type(e: &CaptureEvent) -> Option<&str> {
    e.native.as_ref().and_then(|n| n.item_type.as_deref())
}

// A wrapper's iterator reaching EOF is deliberately absent from this mapping.
fn provider_terminal(e: &CaptureEvent) -> Option<Outcome> {
    if e.kind != "stream.event" && !(e.agent == "grok" && e.kind == "call.completed") {
        return None;
    }
    match (e.agent.as_str(), e.native_type.as_deref()) {
        ("codex", Some("turn.completed")) => Some(Outcome::Completed),
        ("codex", Some("turn.failed" | "error")) => Some(Outcome::Failed),
        ("claude", Some("result")) => match (e.native_status.as_deref(), facts_error(e)) {
            (Some("success"), Some(false)) => Some(Outcome::Completed),
            (_, Some(true)) => Some(Outcome::Failed),
            (Some(s), _) if s.starts_with("error_") => Some(Outcome::Failed),
            _ => Some(Outcome::Unknown),
        },
        // xAI Response object status is the authority, not an assumed stream
        // event name. Missing/new statuses remain unsupported and visible.
        ("grok", _)
            if e.native.as_ref().and_then(|n| n.object_type.as_deref()) == Some("response") =>
        {
            if facts_error(e) == Some(true) {
                return Some(Outcome::Failed);
            }
            match e.native_status.as_deref() {
                Some("completed") => Some(Outcome::Completed),
                Some("incomplete") => Some(Outcome::Partial),
                _ => None,
            }
        }
        _ => None,
    }
}

fn project_operation(rows: &[&CaptureEvent]) -> Result<Activity, &'static str> {
    let first = rows[0];
    if rows.iter().any(|e| {
        e.agent != first.agent
            || e.session_id != first.session_id
            || e.parent_operation_id != first.parent_operation_id
    }) {
        return Err("inconsistent operation identity");
    }
    if first.parent_operation_id.as_ref() == Some(&first.operation_id) {
        return Err("operation cannot parent itself");
    }
    let tool_operation = rows.iter().any(|e| {
        e.kind.starts_with("tool.")
            || (e.kind == "hook.event"
                && matches!(
                    e.native_type.as_deref(),
                    Some("PreToolUse" | "PostToolUse" | "PostToolUseFailure")
                ))
    });
    let call_operation = rows
        .iter()
        .any(|e| e.kind.starts_with("call.") || e.kind.starts_with("stream."));
    let mut notes = BTreeSet::new();
    let terminals: Vec<_> = rows
        .iter()
        .filter_map(|e| provider_terminal(e).map(|status| (*e, status)))
        .collect();
    // Multiple provider terminal boundaries may represent multiple inner turns.
    // Do not deduplicate distinct IDs just because their usage happens to match.
    let provider_outcome = (terminals.len() == 1).then(|| terminals[0].1);
    if terminals.len() > 1 {
        notes.insert("multiple_provider_terminals".into());
    }
    let failures: Vec<_> = rows
        .iter()
        .filter(|e| {
            matches!(e.kind.as_str(), "call.failed" | "tool.failed")
                || (e.kind == "hook.event"
                    && e.native_type.as_deref() == Some("PostToolUseFailure")
                    && e.native.as_ref().and_then(|n| n.is_interrupt) != Some(true))
        })
        .collect();
    let cancellations: Vec<_> = rows
        .iter()
        .filter(|e| {
            e.kind == "stream.cancelled"
                || (e.kind == "hook.event"
                    && e.native_type.as_deref() == Some("PostToolUseFailure")
                    && e.native.as_ref().and_then(|n| n.is_interrupt) == Some(true))
        })
        .collect();
    let returned: Vec<_> = rows
        .iter()
        .filter(|e| {
            matches!(e.kind.as_str(), "call.completed" | "tool.completed")
                || (e.kind == "hook.event" && e.native_type.as_deref() == Some("PostToolUse"))
        })
        .collect();
    let started = rows.iter().any(|e| {
        matches!(e.kind.as_str(), "call.started" | "tool.started")
            || (e.kind == "hook.event" && e.native_type.as_deref() == Some("PreToolUse"))
    });
    if !started {
        notes.insert("start_not_observed".into());
    }
    let (outcome, basis) = if !failures.is_empty() {
        (Outcome::Failed, "observed_failure")
    } else if !cancellations.is_empty() {
        (Outcome::Cancelled, "observed_interruption")
    } else if let Some(status) = provider_outcome {
        (status, "provider_terminal")
    } else if terminals.len() > 1 {
        (Outcome::Unknown, "multiple_provider_terminals")
    } else if returned.len() == 1 {
        (Outcome::Completed, "wrapper_return")
    } else if returned.len() > 1 {
        notes.insert("multiple_returns".into());
        (Outcome::Unknown, "multiple_returns")
    } else if started {
        (Outcome::Partial, "end_not_observed")
    } else {
        (Outcome::Unknown, "insufficient_evidence")
    };
    if rows.iter().any(|e| e.kind == "stream.completed") && terminals.is_empty() {
        notes.insert("iterator_ended_without_provider_terminal".into());
    }
    if !failures.is_empty() && !returned.is_empty() {
        notes.insert("return_and_failure_observed".into());
    }
    let usage_source = if terminals.len() == 1 {
        Some(terminals[0].0)
    } else if terminals.is_empty() && returned.len() == 1 && returned[0].kind == "call.completed" {
        Some(*returned[0])
    } else {
        None
    };
    let usage_source = usage_source.filter(|e| e.usage.is_some());
    let mut findings = vec![];
    let failure_ids: Vec<_> = failures
        .iter()
        .map(|e| e.event_id.clone())
        .chain(
            terminals
                .iter()
                .filter(|(_, s)| *s == Outcome::Failed)
                .map(|(e, _)| e.event_id.clone()),
        )
        .collect();
    if !failure_ids.is_empty() {
        findings.push(finding(if tool_operation { "sdk_tool_failed" } else { "sdk_call_failed" },
            "The captured operation reported a failure. Inspect its source events before deciding what to change.", failure_ids));
    }
    if !cancellations.is_empty() {
        findings.push(finding(
            "sdk_interrupted",
            "The captured operation was interrupted. The events do not establish who requested it.",
            cancellations.iter().map(|e| e.event_id.clone()).collect(),
        ));
    }
    let mut tools: BTreeMap<&str, Vec<&CaptureEvent>> = BTreeMap::new();
    let mut unsupported = vec![];
    for e in rows {
        let recognized = if e.kind == "stream.event" {
            match (e.agent.as_str(), e.native_type.as_deref()) {
                ("codex", Some("item.started" | "item.updated" | "item.completed")) => {
                    match item_type(e) {
                        Some(
                            "command_execution" | "file_change" | "mcp_tool_call" | "web_search",
                        ) if e.native_id.is_some() => {
                            tools
                                .entry(e.native_id.as_deref().expect("id"))
                                .or_default()
                                .push(e);
                            true
                        }
                        Some("error") => {
                            findings.push(finding("sdk_reported_error", "The provider reported a non-terminal error item. This does not establish that the whole call failed.", vec![e.event_id.clone()]));
                            true
                        }
                        Some("agent_message" | "reasoning" | "todo_list") => true,
                        _ => false,
                    }
                }
                (
                    "codex",
                    Some(
                        "thread.started" | "turn.started" | "turn.completed" | "turn.failed"
                        | "error",
                    ),
                ) => true,
                ("grok", _) if provider_terminal(e).is_some() => true,
                ("claude", Some("result" | "assistant" | "user")) => true,
                ("claude", Some("stream_event")) => matches!(
                    e.native.as_ref().and_then(|n| n.event_type.as_deref()),
                    Some(
                        "message_start"
                            | "message_delta"
                            | "message_stop"
                            | "content_block_start"
                            | "content_block_delta"
                            | "content_block_stop"
                            | "ping"
                    )
                ),
                _ => false,
            }
        } else if e.kind == "hook.event" {
            matches!(
                e.native_type.as_deref(),
                Some(
                    "PreToolUse"
                        | "PostToolUse"
                        | "PostToolUseFailure"
                        | "UserPromptSubmit"
                        | "Stop"
                        | "SubagentStart"
                        | "SubagentStop"
                        | "PreCompact"
                        | "PermissionRequest"
                        | "SessionStart"
                        | "SessionEnd"
                        | "Notification"
                )
            )
        } else {
            true
        };
        if !recognized {
            unsupported.push(e.event_id.clone());
        }
    }
    let tools = tools.into_iter().map(|(id, events)| {
        let last = events[events.len()-1];
        let terminal = last.native_type.as_deref() == Some("item.completed");
        let conflicting = events.iter().filter(|e| e.native_type.as_deref() == Some("item.completed")).count() > 1
            || events.iter().any(|e| item_type(e) != item_type(last));
        let reopened = !terminal && events.iter().any(|e| e.native_type.as_deref() == Some("item.completed"));
        if conflicting || reopened { notes.insert("conflicting_tool_lifecycle".into()); }
        let outcome = if conflicting || reopened { Outcome::Unknown } else if terminal {
            match last.native_status.as_deref() {
                Some("failed") => Outcome::Failed,
                Some("completed") if last.exit_code.is_none_or(|code| code == 0) => Outcome::Completed,
                _ if last.exit_code.is_some_and(|code| code != 0) => Outcome::Failed,
                _ => Outcome::Unknown,
            }
        } else { Outcome::Partial };
        let failures: Vec<_> = events.iter().filter(|e| e.native_type.as_deref() == Some("item.completed")
            && (e.native_status.as_deref() == Some("failed") || e.exit_code.is_some_and(|c| c != 0))).map(|e| e.event_id.clone()).collect();
        if !failures.is_empty() { findings.push(finding("sdk_tool_failed", "A captured tool reported failure. A completed item event does not imply success.", failures)); }
        Tool { native_id:id.into(),kind:item_type(last).unwrap_or("unknown").into(),name:events.iter().rev().find_map(|e| e.tool_name.clone()), outcome,event_ids:events.iter().map(|e| e.event_id.clone()).collect() }
    }).collect();
    if !unsupported.is_empty() {
        notes.insert("unsupported_events_present".into());
    }
    Ok(Activity {
        schema: SCHEMA.into(),
        projection_version: PROJECTION_VERSION,
        activity_id: identity(&first.capture_id, &first.operation_id),
        capture_id: first.capture_id.clone(),
        operation_id: first.operation_id.clone(),
        parent_activity_id: first
            .parent_operation_id
            .as_ref()
            .map(|p| identity(&first.capture_id, p)),
        agent: first.agent.clone(),
        session_id: first.session_id.clone(),
        kind: if tool_operation {
            "tool"
        } else if call_operation {
            "call"
        } else {
            "observation"
        }
        .into(),
        outcome,
        outcome_basis: basis.into(),
        provider_outcome,
        first_observed_at: first.observed_at.clone(),
        last_observed_at: rows[rows.len() - 1].observed_at.clone(),
        models: rows
            .iter()
            .filter_map(|e| e.model.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        usage: usage_source.and_then(|e| e.usage.clone()),
        usage_event_id: usage_source.map(|e| e.event_id.clone()),
        tools,
        findings,
        events: rows
            .iter()
            .map(|e| EventRef {
                event_id: e.event_id.clone(),
                sequence: e.sequence,
                kind: e.kind.clone(),
                native_type: e.native_type.clone(),
            })
            .collect(),
        unsupported_event_ids: unsupported,
        coverage_notes: notes.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests;
