//! Claude assistant blocks can share one API message and its usage. Count that
//! message once for hosted input/cache totals; per-step output is a placeholder.

use super::Turn;
use rroflow_core::Usage;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct MessageUsage {
    messages: HashMap<(String, String), MessageObservation>,
    invalid_turns: HashSet<usize>,
}

struct MessageObservation {
    turn: usize,
    fields: [Option<u64>; 4],
}

impl MessageUsage {
    pub(super) fn observe(&mut self, turn: usize, session: &str, record: &Value) {
        let message = &record["message"];
        let id = message["id"].as_str().filter(|s| !s.is_empty());
        if session.is_empty()
            || id.is_none()
            || record["sessionId"]
                .as_str()
                .is_some_and(|reported| reported != session)
        {
            self.invalid_turns.insert(turn);
            return;
        }
        let usage = &message["usage"];
        let fields = [
            usage["input_tokens"].as_u64(),
            None, // Per-step output is not the authoritative turn output count.
            usage["cache_read_input_tokens"].as_u64(),
            usage["cache_creation_input_tokens"].as_u64(),
        ];
        let key = (session.to_string(), id.unwrap().to_string());
        if let Some(previous) = self.messages.get(&key) {
            if previous.fields != fields {
                self.invalid_turns.insert(previous.turn);
                self.invalid_turns.insert(turn);
            }
            // Identical late/replayed blocks retain their original owner.
            return;
        }
        self.messages
            .insert(key, MessageObservation { turn, fields });
    }

    pub(super) fn apply(self, turns: &mut [Turn]) {
        let mut totals: HashMap<usize, Turn> = HashMap::new();
        for observation in self.messages.values() {
            totals
                .entry(observation.turn)
                .or_default()
                .observe_usage(observation.fields);
        }
        for (index, turn) in turns.iter_mut().enumerate() {
            let mut usage = Usage::default();
            if !self.invalid_turns.contains(&index)
                && let Some(total) = totals.get(&index)
            {
                usage.input_tokens = total.usage_known[0].then_some(total.in_tokens);
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

    fn report(id: Value, input: Value) -> Value {
        json!({"type":"assistant","sessionId":"s","message":{"id":id,"usage":{
            "input_tokens":input,"output_tokens":900,"cache_read_input_tokens":0,"cache_creation_input_tokens":2}}})
    }

    #[test]
    fn duplicates_do_not_charge_later_turns_and_placeholder_output_is_unknown() {
        let mut state = MessageUsage::default();
        state.observe(0, "s", &report(json!("m1"), json!(3)));
        state.observe(1, "s", &report(json!("m1"), json!(3)));
        state.observe(0, "s", &report(json!("m2"), json!(3)));
        let mut turns = [Turn::default(), Turn::default()];
        state.apply(&mut turns);
        let usage = turns[0].hosted_usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(6));
        assert_eq!(usage.cache_read_tokens, Some(0));
        assert_eq!(usage.cache_write_tokens, Some(4));
        assert_eq!(usage.output_tokens, None);
        assert_eq!(turns[1].hosted_usage.as_ref().unwrap().input_tokens, None);
    }

    #[test]
    fn invalid_missing_and_conflicting_identities_never_become_totals() {
        let mut state = MessageUsage::default();
        state.observe(0, "s", &report(json!("m"), json!(3)));
        state.observe(1, "s", &report(json!("m"), json!(4)));
        state.observe(2, "s", &report(Value::Null, json!(3)));
        state.observe(3, "different", &report(json!("n"), json!(3)));
        let mut turns = std::array::from_fn::<_, 4, _>(|_| Turn::default());
        state.apply(&mut turns);
        assert!(
            turns
                .iter()
                .all(|t| t.hosted_usage.as_ref().unwrap().input_tokens.is_none())
        );
    }

    #[test]
    fn same_message_id_in_different_sessions_is_not_a_duplicate() {
        let mut state = MessageUsage::default();
        state.observe(0, "s", &report(json!("m"), json!(3)));
        let mut other = report(json!("m"), json!(4));
        other["sessionId"] = json!("other");
        state.observe(1, "other", &other);
        let mut turns = [Turn::default(), Turn::default()];
        state.apply(&mut turns);
        assert_eq!(
            turns[0].hosted_usage.as_ref().unwrap().input_tokens,
            Some(3)
        );
        assert_eq!(
            turns[1].hosted_usage.as_ref().unwrap().input_tokens,
            Some(4)
        );
    }

    #[test]
    fn both_readers_preserve_parallel_tools_but_count_usage_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parallel.jsonl");
        let mut first = report(json!("m"), json!(9));
        first["message"]["content"] = json!([{"type":"tool_use","id":"a","name":"one","input":{}}]);
        let mut second = first.clone();
        second["message"]["content"] =
            json!([{"type":"tool_use","id":"b","name":"two","input":{}}]);
        let lines = [
            json!({"type":"user","sessionId":"s","promptId":"p","message":{"content":"work"}}),
            first,
            second,
            json!({"type":"user","sessionId":"s","timestamp":"2026-10-31T00:00:00Z","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"ok"},{"type":"tool_result","tool_use_id":"b","content":"ok"}]}}),
            report(json!("final"), json!(6)),
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
            assert_eq!(turns.len(), 1);
            assert_eq!(
                turns[0].in_tokens, 24,
                "legacy numeric projection is unchanged"
            );
            let record = crate::record::to_record(&turns[0], None, &crate::redact::Redactor::new());
            assert_eq!(
                record.tools.len(),
                2,
                "usage deduplication does not drop tool calls"
            );
            assert_eq!(record.usage.input_tokens, Some(15));
            assert_eq!(record.usage.output_tokens, None);
            assert_eq!(record.usage.cache_write_tokens, Some(4));
        }
    }
}
