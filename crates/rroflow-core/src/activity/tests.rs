use super::*;
use crate::capture::NativeFacts;
use serde_json::json;
fn event(seq: u64, kind: &str, native_type: Option<&str>) -> CaptureEvent {
    serde_json::from_value(json!({
        "schema":"rroflow.capture/v1","event_id":format!("00000000-0000-4000-8000-{seq:012}"),
        "capture_id":"11111111-1111-4111-8111-111111111111","sequence":seq,
        "agent":"codex","session_id":"session","operation_id":"call","parent_operation_id":null,
        "observed_at":format!("2026-10-30T12:00:{:02}Z",seq%60),"kind":kind,"native_type":native_type,
        "model":null,"native_id":null,"parent_native_id":null,"native_status":null,"tool_name":null,"exit_code":null,"usage":null,"payload":null
    })).unwrap()
}
fn facts(item: Option<&str>) -> NativeFacts {
    NativeFacts {
        version: 1,
        item_type: item.map(Into::into),
        event_type: None,
        object_type: None,
        stop_reason: None,
        is_error: None,
        is_interrupt: None,
    }
}
fn one(events: &[CaptureEvent]) -> Activity {
    project(events).unwrap().remove(0)
}
#[test]
fn iterator_end_is_not_provider_success_and_partial_usage_is_not_totalled() {
    let start = event(0, "call.started", None);
    let mut delta = event(1, "stream.event", Some("future.event"));
    delta.usage = Some(Usage {
        input_tokens: Some(100),
        output_tokens: Some(2),
        ..Usage::default()
    });
    let a = one(&[start, delta, event(2, "stream.completed", None)]);
    assert_eq!(a.outcome, Outcome::Partial);
    assert_eq!(a.provider_outcome, None);
    assert_eq!(a.usage, None);
    assert_eq!(a.unsupported_event_ids.len(), 1);
    assert!(
        a.coverage_notes
            .contains(&"iterator_ended_without_provider_terminal".into())
    );
}
#[test]
fn terminal_usage_is_once_only_and_zero_is_distinct_from_missing() {
    let mut done = event(2, "stream.event", Some("turn.completed"));
    done.usage = Some(Usage {
        input_tokens: Some(80),
        output_tokens: Some(0),
        cache_read_tokens: Some(70),
        input_includes_cache: true,
        ..Usage::default()
    });
    let a = one(&[
        event(0, "call.started", None),
        done.clone(),
        event(3, "stream.completed", None),
    ]);
    assert_eq!(a.outcome, Outcome::Completed);
    assert_eq!(a.usage, done.usage);
    assert_eq!(a.usage_event_id, Some(done.event_id.clone()));
    done.usage = None;
    assert_eq!(one(&[done]).usage, None);
}
#[test]
fn reorder_and_duplicate_replay_converge_and_late_evidence_changes_revision() {
    let events = vec![
        event(0, "call.started", None),
        event(2, "stream.event", Some("turn.failed")),
        event(3, "stream.completed", None),
    ];
    let first = one(&events);
    let mut reordered = events.clone();
    reordered.reverse();
    reordered.push(events[0].clone());
    assert_eq!(first, one(&reordered));
    assert_eq!(revision(&first), revision(&one(&reordered)));
    assert_ne!(revision(&one(&events[..1])), revision(&first));
    assert_eq!(first.activity_id, one(&events[..1]).activity_id);
    assert_eq!(
        first.findings[0].event_ids,
        vec![events[1].event_id.clone()]
    );
}
#[test]
fn contradictory_identities_are_refused_instead_of_selected() {
    let a = event(1, "call.started", None);
    let mut conflict = a.clone();
    conflict.kind = "call.failed".into();
    assert!(project(&[a.clone(), conflict]).is_err());
    let mut other = event(2, "call.failed", None);
    other.sequence = 1;
    assert!(project(&[a.clone(), other]).is_err());
    let mut other = event(2, "call.failed", None);
    other.agent = "claude".into();
    assert!(project(&[a, other]).is_err());
}
#[test]
fn multiple_terminals_never_sum_or_guess_their_scope() {
    let mut a = event(1, "stream.event", Some("turn.completed"));
    a.usage = Some(Usage {
        output_tokens: Some(5),
        ..Usage::default()
    });
    let mut b = event(2, "stream.event", Some("turn.completed"));
    b.usage = a.usage.clone();
    let out = one(&[a, b]);
    assert_eq!(out.outcome, Outcome::Unknown);
    assert_eq!(out.usage, None);
    assert!(
        out.coverage_notes
            .contains(&"multiple_provider_terminals".into())
    );
}
#[test]
fn claude_success_requires_error_fact_and_failure_is_preserved() {
    let mut result = event(1, "stream.event", Some("result"));
    result.agent = "claude".into();
    result.native_status = Some("success".into());
    assert_eq!(one(&[result.clone()]).outcome, Outcome::Unknown);
    result.native = Some(facts(None));
    result.native.as_mut().unwrap().is_error = Some(false);
    assert_eq!(one(&[result.clone()]).outcome, Outcome::Completed);
    result.native.as_mut().unwrap().is_error = Some(true);
    assert_eq!(one(&[result.clone()]).outcome, Outcome::Failed);
    result.native = None;
    result.native_status = Some("error_max_turns".into());
    assert_eq!(one(&[result]).outcome, Outcome::Failed);
}
#[test]
fn tool_item_completed_can_be_failure_while_the_call_completed() {
    let mut tool = event(1, "stream.event", Some("item.completed"));
    tool.native = Some(facts(Some("command_execution")));
    tool.native_id = Some("tool-1".into());
    tool.native_status = Some("failed".into());
    tool.exit_code = Some(1);
    let out = one(&[
        event(0, "call.started", None),
        tool.clone(),
        event(2, "stream.event", Some("turn.completed")),
    ]);
    assert_eq!(out.outcome, Outcome::Completed);
    assert_eq!(out.tools[0].outcome, Outcome::Failed);
    assert_eq!(out.findings[0].signal, "sdk_tool_failed");
    assert_eq!(out.findings[0].event_ids, vec![tool.event_id]);
}
#[test]
fn missing_item_kind_remains_visible_and_is_not_guessed_from_payload() {
    let mut tool = event(1, "stream.event", Some("item.completed"));
    tool.native_id = Some("tool-1".into());
    tool.native_status = Some("failed".into());
    tool.payload = Some(json!({"item":{"type":"command_execution","command":"private-command"}}));
    let out = one(&[tool]);
    assert!(out.tools.is_empty());
    assert_eq!(out.unsupported_event_ids.len(), 1);
    assert!(
        !serde_json::to_string(&out)
            .unwrap()
            .contains("private-command")
    );
}
#[test]
fn parent_child_identity_and_cancellation_do_not_blame_the_user() {
    let root = event(0, "call.started", None);
    let mut child = event(1, "hook.event", Some("PostToolUseFailure"));
    child.agent = "claude".into();
    child.operation_id = "tool-child".into();
    child.parent_operation_id = Some("call".into());
    child.native = Some(facts(None));
    child.native.as_mut().unwrap().is_interrupt = Some(true);
    let mut root = root;
    root.agent = "claude".into();
    let out = project(&[root, child]).unwrap();
    assert_eq!(out[1].parent_activity_id, Some(out[0].activity_id.clone()));
    assert_eq!(out[1].kind, "tool");
    assert_eq!(out[1].outcome, Outcome::Cancelled);
    assert_eq!(out[1].findings[0].signal, "sdk_interrupted");
}
#[test]
fn consumer_cancellation_preserves_a_provider_completion_separately() {
    let out = one(&[
        event(0, "call.started", None),
        event(1, "stream.event", Some("turn.completed")),
        event(2, "stream.cancelled", None),
    ]);
    assert_eq!(out.outcome, Outcome::Cancelled);
    assert_eq!(out.provider_outcome, Some(Outcome::Completed));
}
#[test]
fn metadata_version_and_old_serialization_have_explicit_boundaries() {
    let mut old = event(0, "call.started", None);
    assert!(
        !serde_json::to_value(&old)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("native")
    );
    old.native = Some(facts(None));
    assert!(old.validate().is_ok());
    old.native.as_mut().unwrap().version = 2;
    assert!(old.validate().is_err());
    old.native.as_mut().unwrap().version = 1;
    old.native.as_mut().unwrap().item_type = Some("private argument text".into());
    assert!(old.validate().is_err());
}

#[test]
fn grok_response_status_is_used_only_with_a_typed_response_object() {
    let mut e = event(1, "stream.event", Some("response.completed"));
    e.agent = "grok".into();
    e.native_status = Some("completed".into());
    assert_eq!(one(&[e.clone()]).outcome, Outcome::Unknown);
    e.native = Some(facts(None));
    e.native.as_mut().unwrap().object_type = Some("response".into());
    assert_eq!(one(&[e.clone()]).outcome, Outcome::Completed);
    e.native_status = Some("incomplete".into());
    assert_eq!(one(&[e.clone()]).outcome, Outcome::Partial);
    e.native.as_mut().unwrap().is_error = Some(true);
    assert_eq!(one(&[e]).outcome, Outcome::Failed);
}

#[test]
fn cycles_are_refused_and_missing_ancestors_are_visible() {
    let mut a = event(0, "call.started", None);
    a.parent_operation_id = Some("parent".into());
    assert!(
        one(&[a.clone()])
            .coverage_notes
            .contains(&"ancestor_not_observed".into())
    );
    let mut b = event(1, "call.started", None);
    b.operation_id = "parent".into();
    b.parent_operation_id = Some("call".into());
    assert!(project(&[a, b]).is_err());
}
#[test]
fn a_reopened_tool_is_unknown_instead_of_a_fresh_success_or_live_tool() {
    let mut a = event(1, "stream.event", Some("item.completed"));
    a.native_id = Some("tool".into());
    a.native = Some(facts(Some("mcp_tool_call")));
    a.native_status = Some("failed".into());
    let mut b = event(2, "stream.event", Some("item.updated"));
    b.native_id = a.native_id.clone();
    b.native = a.native.clone();
    b.native_status = Some("in_progress".into());
    let out = one(&[a, b]);
    assert_eq!(out.tools[0].outcome, Outcome::Unknown);
    assert!(
        out.coverage_notes
            .contains(&"conflicting_tool_lifecycle".into())
    );
}
