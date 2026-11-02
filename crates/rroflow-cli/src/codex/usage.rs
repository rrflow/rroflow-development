//! Hosted usage from response-specific Codex records. Notification snapshots are
//! not response identities and are never added to this projection.

use crate::transcript::Turn;
use rroflow_core::Usage;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct ResponseUsage {
    present: bool,
    responses: HashMap<String, (String, [Option<u64>; 4])>,
    invalid_turns: HashSet<String>,
}

impl ResponseUsage {
    pub(super) fn observe(&mut self, thread: &str, payload: &Value) {
        // A parent/copied or foreign thread must not charge this transcript.
        if thread.is_empty() || payload["thread_id"].as_str() != Some(thread) {
            return;
        }
        self.present = true;
        let Some(turn) = payload["turn_id"].as_str().filter(|s| !s.is_empty()) else {
            return;
        };
        let Some(response) = payload["response_id"].as_str().filter(|s| !s.is_empty()) else {
            self.invalid_turns.insert(turn.to_string());
            return;
        };
        let usage = &payload["usage"];
        let fields = [
            usage["input_tokens"].as_u64(),
            usage["output_tokens"].as_u64(),
            usage["cached_input_tokens"].as_u64(),
            usage["cache_write_input_tokens"].as_u64(),
        ];
        if let Some((previous_turn, previous_fields)) = self.responses.get(response) {
            if previous_turn != turn || previous_fields != &fields {
                self.invalid_turns.insert(previous_turn.clone());
                self.invalid_turns.insert(turn.to_string());
            }
            return;
        }
        self.responses
            .insert(response.to_string(), (turn.to_string(), fields));
    }

    pub(super) fn apply(self, turns: &mut [Turn]) {
        if !self.present {
            return;
        }
        let mut totals: HashMap<String, Turn> = HashMap::new();
        for (turn, fields) in self.responses.values() {
            totals
                .entry(turn.clone())
                .or_default()
                .observe_usage(*fields);
        }
        let mut identities: HashMap<String, usize> = HashMap::new();
        for turn in turns.iter() {
            *identities.entry(turn.key.clone()).or_default() += 1;
        }
        for turn in turns {
            let mut usage = Usage {
                input_includes_cache: true,
                ..Usage::default()
            };
            // Missing, conflicting or ambiguous identities are unknown, not a
            // fallback to notification sums or attribution to the latest turn.
            if !self.invalid_turns.contains(&turn.key)
                && identities[&turn.key] == 1
                && let Some(total) = totals.get(&turn.key)
            {
                usage.input_tokens = total.usage_known[0].then_some(total.in_tokens);
                usage.output_tokens = total.usage_known[1].then_some(total.out_tokens);
                usage.cache_read_tokens = total.usage_known[2].then_some(total.cache_read);
                usage.cache_write_tokens = total.usage_known[3].then_some(total.cache_write);
            }
            turn.hosted_usage = Some(usage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report(response: &str, turn: &str, input: Value) -> Value {
        json!({"thread_id":"thread","turn_id":turn,"response_id":response,
            "usage":{"input_tokens":input,"output_tokens":2,"cached_input_tokens":0,"cache_write_input_tokens":1},
            "turn_token_usage":{"input_tokens":999},"thread_token_usage":{"input_tokens":9999}})
    }

    fn turn(key: &str) -> Turn {
        Turn {
            key: key.into(),
            ..Turn::default()
        }
    }

    #[test]
    fn response_ids_deduplicate_and_distinct_equal_responses_still_count() {
        let mut state = ResponseUsage::default();
        state.observe("thread", &report("a", "T1", json!(3)));
        state.observe("thread", &report("a", "T1", json!(3)));
        state.observe("thread", &report("b", "T1", json!(3)));
        let mut turns = [turn("T1"), turn("T2")];
        state.apply(&mut turns);
        let usage = turns[0].hosted_usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(6));
        assert_eq!(usage.output_tokens, Some(4));
        assert_eq!(usage.cache_read_tokens, Some(0));
        assert_eq!(usage.cache_write_tokens, Some(2));
        assert_eq!(turns[1].hosted_usage.as_ref().unwrap().input_tokens, None);
    }

    #[test]
    fn conflicting_response_identity_invalidates_both_owners_regardless_of_order() {
        let reports = [report("a", "T1", json!(3)), report("a", "T2", json!(4))];
        for reverse in [false, true] {
            let mut state = ResponseUsage::default();
            for index in if reverse { [1, 0] } else { [0, 1] } {
                state.observe("thread", &reports[index]);
            }
            let mut turns = [turn("T1"), turn("T2")];
            state.apply(&mut turns);
            assert!(
                turns
                    .iter()
                    .all(|t| t.hosted_usage.as_ref().unwrap().input_tokens.is_none())
            );
        }
        let mut state = ResponseUsage::default();
        state.observe("thread", &report("a", "T1", json!(3)));
        state.observe("thread", &report("a", "T1", json!(4)));
        let mut turns = [turn("T1")];
        state.apply(&mut turns);
        assert_eq!(turns[0].hosted_usage.as_ref().unwrap().input_tokens, None);
    }

    #[test]
    fn foreign_missing_and_ambiguous_identity_never_charges_the_latest_turn() {
        let mut state = ResponseUsage::default();
        state.observe("different-thread", &report("a", "T1", json!(3)));
        let mut turns = [turn("T1")];
        state.apply(&mut turns);
        assert!(turns[0].hosted_usage.is_none());

        let mut state = ResponseUsage::default();
        state.observe("thread", &report("a", "unknown-turn", json!(100)));
        state.observe("thread", &report("", "T1", json!(3)));
        state.observe("thread", &report("b", "T1", json!(3)));
        state.observe("thread", &report("c", "T2", json!(3)));
        let mut turns = [turn("T1"), turn("T2"), turn("T2")];
        state.apply(&mut turns);
        assert!(
            turns
                .iter()
                .all(|t| t.hosted_usage.as_ref().unwrap().input_tokens.is_none())
        );
    }

    #[test]
    fn incomplete_response_fields_and_overflow_stay_unknown() {
        let mut state = ResponseUsage::default();
        state.observe("thread", &report("a", "T1", json!(u64::MAX)));
        state.observe("thread", &report("b", "T1", json!(1)));
        state.observe("thread", &report("c", "T2", Value::Null));
        state.observe("thread", &report("d", "T2", json!(0)));
        let mut turns = [turn("T1"), turn("T2")];
        state.apply(&mut turns);
        for t in turns {
            let usage = t.hosted_usage.unwrap();
            assert_eq!(usage.input_tokens, None);
            assert_eq!(usage.output_tokens, Some(4));
        }
    }

    #[test]
    fn rollout_readers_bind_late_records_and_ignore_notification_totals() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("responses.jsonl");
        let usage = report("a", "T1", json!(7));
        let lines = [
            json!({"type":"session_meta","payload":{"id":"thread"}}),
            json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"T1"}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"first"}}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":99,"output_tokens":99}}}}),
            json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"T2"}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"second"}}),
            json!({"type":"token_usage_record","payload":usage}),
            json!({"type":"token_usage_record","payload":usage}),
        ];
        std::fs::write(
            &path,
            lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        for bounded in [false, true] {
            let turns = super::super::parse(&path, bounded).unwrap();
            assert_eq!(turns.len(), 2);
            assert_eq!(
                turns[0].in_tokens, 99,
                "legacy sink projection stays unchanged"
            );
            let first = crate::record::to_record(&turns[0], None, &crate::redact::Redactor::new());
            let second = crate::record::to_record(&turns[1], None, &crate::redact::Redactor::new());
            assert_eq!(first.usage.input_tokens, Some(7));
            assert_eq!(first.usage.cache_write_tokens, Some(1));
            assert_eq!(second.usage.input_tokens, None);
        }
    }
}
