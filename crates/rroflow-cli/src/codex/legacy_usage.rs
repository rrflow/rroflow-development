//! Older Codex notifications have no response identity. Accept an increment only
//! when cumulative counters support it; never guess through resets or gaps.

use crate::transcript::Turn;
use rroflow_core::Usage;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

type Counts = [Option<u64>; 4];

struct Snapshot {
    total: Counts,
    last: Counts,
    owner: Option<usize>,
}

#[derive(Default)]
pub(super) struct LegacyUsage {
    previous: Option<Snapshot>,
    totals: HashMap<usize, Turn>,
    invalid: HashSet<usize>,
}

fn counts(value: &Value) -> Counts {
    [
        "input_tokens",
        "output_tokens",
        "cached_input_tokens",
        "cache_write_input_tokens",
    ]
    .map(|field| value[field].as_u64())
}

impl LegacyUsage {
    pub(super) fn observe(&mut self, owner: Option<usize>, info: &Value) {
        if info.is_null() {
            return; // Rate-limit-only notification, not a usage report.
        }
        if !info["total_token_usage"].is_object() || !info["last_token_usage"].is_object() {
            if let Some(owner) = owner {
                self.invalid.insert(owner);
            }
            self.previous = None;
            return;
        }
        let total = counts(&info["total_token_usage"]);
        let last = counts(&info["last_token_usage"]);
        if let Some(previous) = &self.previous
            && previous.total == total
        {
            if previous.last != last {
                self.invalid.extend(previous.owner);
                self.invalid.extend(owner);
            }
            return;
        }
        let delta = std::array::from_fn(|i| {
            let current = total[i]?;
            let increment = last[i]?;
            let base = match &self.previous {
                Some(previous) => previous.total[i]?,
                None => 0,
            };
            (current.checked_sub(base)? == increment).then_some(increment)
        });
        if let Some(owner) = owner {
            self.totals.entry(owner).or_default().observe_usage(delta);
        }
        self.previous = Some(Snapshot { total, last, owner });
    }

    pub(super) fn apply(self, turns: &mut [Turn]) {
        for (index, turn) in turns.iter_mut().enumerate() {
            let mut usage = Usage {
                input_includes_cache: true,
                ..Usage::default()
            };
            if !self.invalid.contains(&index)
                && let Some(total) = self.totals.get(&index)
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

    fn report(total: u64, last: u64) -> Value {
        json!({"total_token_usage":{"input_tokens":total,"output_tokens":total,"cached_input_tokens":0},
            "last_token_usage":{"input_tokens":last,"output_tokens":last,"cached_input_tokens":0}})
    }

    #[test]
    fn snapshots_count_once_and_new_turns_receive_only_their_increment() {
        let mut state = LegacyUsage::default();
        state.observe(Some(0), &report(10, 10));
        state.observe(Some(0), &report(10, 10));
        state.observe(Some(1), &report(10, 10));
        state.observe(Some(1), &Value::Null);
        state.observe(Some(1), &report(20, 10));
        let mut turns = [Turn::default(), Turn::default()];
        state.apply(&mut turns);
        for t in turns {
            let usage = t.hosted_usage.unwrap();
            assert_eq!(usage.input_tokens, Some(10));
            assert_eq!(usage.cache_read_tokens, Some(0));
            assert_eq!(usage.cache_write_tokens, None);
        }
    }

    #[test]
    fn resets_gaps_and_partial_history_are_not_silently_priced() {
        let mut state = LegacyUsage::default();
        state.observe(Some(0), &report(100, 10)); // Missing earlier history.
        state.observe(Some(1), &report(130, 10)); // Delta and last disagree.
        state.observe(Some(2), &report(5, 5)); // Counter reset.
        state.observe(Some(2), &report(10, 5)); // Cannot repair that turn.
        state.observe(Some(3), &report(15, 5)); // Later turn has a valid baseline.
        let mut turns = std::array::from_fn::<_, 4, _>(|_| Turn::default());
        state.apply(&mut turns);
        assert!(
            turns[..3]
                .iter()
                .all(|t| t.hosted_usage.as_ref().unwrap().input_tokens.is_none())
        );
        assert_eq!(
            turns[3].hosted_usage.as_ref().unwrap().input_tokens,
            Some(5)
        );
    }

    #[test]
    fn conflicting_or_missing_snapshots_cannot_become_known_zero() {
        let mut state = LegacyUsage::default();
        state.observe(Some(0), &report(10, 10));
        state.observe(Some(1), &report(10, 5));
        state.observe(Some(2), &json!({"last_token_usage":{"input_tokens":0}}));
        let mut turns = std::array::from_fn::<_, 3, _>(|_| Turn::default());
        state.apply(&mut turns);
        assert!(
            turns
                .iter()
                .all(|t| t.hosted_usage.as_ref().unwrap().input_tokens.is_none())
        );
    }

    #[test]
    fn both_readers_deduplicate_legacy_snapshots_without_changing_raindrop_totals() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.jsonl");
        let event =
            |info: Value| json!({"type":"event_msg","payload":{"type":"token_count","info":info}});
        let lines = [
            json!({"type":"session_meta","payload":{"id":"thread"}}),
            json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"T1"}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"first"}}),
            event(report(10, 10)),
            event(report(10, 10)),
            json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"T2"}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"second"}}),
            event(report(10, 10)),
            event(report(15, 5)),
            event(Value::Null),
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
            assert_eq!((turns[0].in_tokens, turns[1].in_tokens), (20, 15));
            assert_eq!(
                turns[0].hosted_usage.as_ref().unwrap().input_tokens,
                Some(10)
            );
            assert_eq!(
                turns[1].hosted_usage.as_ref().unwrap().input_tokens,
                Some(5)
            );
        }
    }
}
