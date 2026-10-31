//! Parsed turns → `rroflow.turn/v1` records (rroflow-core), with signals evaluated
//! on the user's machine before any content is dropped for upload.

use crate::otlp::event_id;
use crate::redact::{Redactor, truncate};
use crate::transcript::Turn;
use rroflow_core::signals::{SignalConfig, evaluate_session_with};
use rroflow_core::{Content, SCHEMA, Termination, ToolRecord, ToolStatus, TurnRecord, Usage};

/// Longest prompt or response text kept in a record.
const TEXT_MAX: usize = 20_000;
/// Longest tool input or output kept in a record.
const TOOL_MAX: usize = 2_000;

fn termination(t: &Turn) -> Termination {
    let finished: Vec<_> = t.tools.iter().filter(|x| x.end.is_some()).collect();
    if t.aborted {
        Termination::UserCancel
    } else if !t.output.is_empty() {
        Termination::Completed
    } else if finished.last().is_some_and(|x| x.error) {
        Termination::ToolError
    } else {
        Termination::Unknown
    }
}

/// Build one record with full (redacted) content. Signals and content stripping happen
/// in [`records`], which sees the whole session.
pub(crate) fn to_record(t: &Turn, project: Option<&str>, r: &Redactor) -> TurnRecord {
    let finished: Vec<_> = t.tools.iter().filter(|x| x.end.is_some()).collect();
    TurnRecord {
        schema: SCHEMA.into(),
        event_id: event_id(t),
        agent: t.agent.to_string(),
        agent_version: t.version.clone(),
        provider: t.provider.clone(),
        model: t.model.clone(),
        session_id: t.session.clone(),
        turn_key: t.key.clone(),
        subagent: t.subagent.clone(),
        prompt_author: t.prompt_author.clone(),
        started_at: t.start.clone(),
        ended_at: t.end.clone(),
        cwd: t.cwd.clone(),
        git_branch: t.branch.clone(),
        // Without an explicit label, keep working directories distinct. This is
        // a directory grouping, not a claim that we discovered the Git root.
        project: project
            .map(str::to_string)
            .or_else(|| (!t.cwd.is_empty()).then(|| r.apply(&t.cwd))),
        usage: t.hosted_usage.clone().unwrap_or(Usage {
            input_tokens: t.usage_known[0].then_some(t.in_tokens),
            output_tokens: t.usage_known[1].then_some(t.out_tokens),
            cache_read_tokens: t.usage_known[2].then_some(t.cache_read),
            cache_write_tokens: t.usage_known[3].then_some(t.cache_write),
            // OpenAI's input_tokens include cached input; Anthropic's do not.
            input_includes_cache: t.agent == "codex",
        }),
        termination: termination(t),
        compacted: t.compacted,
        tools: finished
            .iter()
            .map(|x| ToolRecord {
                id: x.id.clone(),
                name: x.name.clone(),
                started_at: x.start.clone(),
                ended_at: x.end.clone().unwrap_or_default(),
                status: if x.error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Ok
                },
                input: Some(r.apply(&truncate(&x.input, TOOL_MAX))),
                output: Some(r.apply(&truncate(&x.output, TOOL_MAX))),
            })
            .collect(),
        tools_unfinished: (t.tools.len() - finished.len()) as u32,
        content: Some(Content {
            input: r.apply(&truncate(&t.input, TEXT_MAX)),
            output: r.apply(&truncate(&t.output.join("\n\n"), TEXT_MAX)),
        }),
        signals: vec![],
        collector: concat!("rroflow/", env!("CARGO_PKG_VERSION")).into(),
    }
}

/// All turns of one transcript → records with signals. Content is kept only when
/// `content` is true; signal results and metrics are always kept.
pub(crate) fn records(
    turns: &[Turn],
    project: Option<&str>,
    content: bool,
    r: &Redactor,
    signals: &SignalConfig,
) -> Vec<TurnRecord> {
    let mut recs: Vec<TurnRecord> = turns.iter().map(|t| to_record(t, project, r)).collect();
    evaluate_session_with(&mut recs, signals);
    if !content {
        for rec in &mut recs {
            rec.strip_content();
        }
    }
    recs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Tool;

    fn turn() -> Turn {
        Turn {
            key: "k".into(),
            session: "s".into(),
            start: "2026-10-26T00:00:00Z".into(),
            end: "2026-10-26T00:01:00Z".into(),
            input: "deploy it with key sk-ABCDEFGHIJKLMNOPQRSTUV".into(),
            output: vec!["You'll need to approve the deploy manually.".into()],
            model: Some("claude-opus-5-5".into()),
            in_tokens: 10,
            out_tokens: 20,
            cache_read: 900,
            usage_known: [true, true, true, false],
            usage_seen: true,
            agent: "claude-code",
            provider: "anthropic".into(),
            tools: vec![
                Tool {
                    id: "t1".into(),
                    name: "Bash".into(),
                    input: "{}".into(),
                    start: "2026-10-26T00:00:10Z".into(),
                    end: Some("2026-10-26T00:00:11Z".into()),
                    output: "permission denied".into(),
                    error: true,
                },
                Tool {
                    id: "t2".into(),
                    name: "Bash".into(),
                    start: "2026-10-26T00:00:20Z".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn signals_survive_without_content_and_secrets_never_appear() {
        let recs = records(
            &[turn()],
            Some("shippinai"),
            false,
            &Redactor::new(),
            &SignalConfig::default(),
        );
        let r = &recs[0];
        assert!(r.content.is_none());
        assert!(r.signals.iter().any(|s| s.signal == "blocked_handoff"));
        assert_eq!(r.tools.len(), 1, "unfinished calls are held back");
        assert_eq!(r.tools_unfinished, 1);
        assert_eq!(r.usage.cache_read_tokens, Some(900));
        assert!(!r.usage.input_includes_cache);
        let json = serde_json::to_string(r).unwrap();
        assert!(!json.contains("sk-ABCDEF"));
        assert!(
            !json.contains("permission denied"),
            "no text without content opt-in"
        );
    }

    #[test]
    fn content_opt_in_keeps_redacted_text() {
        let recs = records(
            &[turn()],
            None,
            true,
            &Redactor::new(),
            &SignalConfig::default(),
        );
        let c = recs[0].content.as_ref().unwrap();
        assert!(c.input.contains("[REDACTED]") && !c.input.contains("sk-ABCDEF"));
    }

    #[test]
    fn known_model_does_not_imply_reported_usage() {
        let t = Turn {
            model: Some("known-model".into()),
            ..Default::default()
        };
        let usage = to_record(&t, None, &Redactor::new()).usage;
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, None);
        assert_eq!(usage.cache_read_tokens, None);
        assert_eq!(usage.cache_write_tokens, None);
    }

    #[test]
    fn unanswered_turn_has_unknown_usage_not_zero() {
        let t = Turn::default();
        let r = to_record(&t, None, &Redactor::new());
        assert_eq!(r.usage.input_tokens, None);
    }

    #[test]
    fn explicit_zero_and_usage_without_model_remain_known() {
        let mut t = Turn::default();
        t.observe_usage([Some(0), Some(7), None, Some(0)]);
        let r = to_record(&t, None, &Redactor::new());
        assert_eq!(r.usage.input_tokens, Some(0));
        assert_eq!(r.usage.output_tokens, Some(7));
        assert_eq!(r.usage.cache_read_tokens, None);
        assert_eq!(r.usage.cache_write_tokens, Some(0));
        assert_eq!(r.model, None);
    }

    #[test]
    fn incomplete_reports_and_overflow_cannot_be_complete_totals() {
        let mut t = Turn::default();
        t.observe_usage([Some(u64::MAX), Some(2), None, Some(0)]);
        t.observe_usage([Some(1), None, Some(9), Some(0)]);
        t.observe_usage([Some(0), Some(3), Some(1), Some(0)]);
        let r = to_record(&t, None, &Redactor::new());
        assert_eq!(r.usage.input_tokens, None);
        assert_eq!(r.usage.output_tokens, None);
        assert_eq!(r.usage.cache_read_tokens, None);
        assert_eq!(r.usage.cache_write_tokens, Some(0));
    }
}
