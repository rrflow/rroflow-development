//! Behaviour signals v0: deterministic rules (tier 1) over `rroflow.turn/v1` records.
//!
//! Every rule is versioned and every hit carries evidence (the tool calls and text that
//! matched), so a person can check it and a label can later turn it into training data.
//! Rules run where the content is, before content is stripped for upload.
//!
//! Sources for the set: Raindrop presets and the Blocked Work Handoff signal authored live;
//! Sentry tool-error and spend views; DevPulse failure catalogue (false completion,
//! compaction loss, correction spirals) and the owner's rule that mood runs both ways.

use crate::record::{
    Evidence, EvidenceKind, Severity, SignalHit, Termination, ToolStatus, TurnRecord,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// What a signal is about. Used to group signals for people and for alerting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// The agent's work failed or was claimed done when it was not.
    Failure,
    /// The person and the agent are working against each other.
    Friction,
    /// Money or quota spent without a finished result, or limits in the way.
    Cost,
    /// The agent may be missing information it had.
    Context,
}

impl Category {
    /// Every category, in display order.
    pub const ALL: [Category; 4] = [
        Category::Failure,
        Category::Friction,
        Category::Cost,
        Category::Context,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Failure => "Failure",
            Category::Friction => "Friction",
            Category::Cost => "Cost",
            Category::Context => "Context",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Category::Failure => "The agent's work failed or was claimed done when it was not.",
            Category::Friction => "The person and the agent are working against each other.",
            Category::Cost => {
                "Money or quota spent without a finished result, or limits in the way."
            }
            Category::Context => "The agent may be missing information it had.",
        }
    }
}

/// A tunable number on a signal, with its default and allowed range.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct OptionSpec {
    pub name: &'static str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub meaning: &'static str,
}

/// One signal in the catalogue: id, rule version, category, plain meaning and options.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct SignalSpec {
    pub id: &'static str,
    pub version: u32,
    pub category: Category,
    pub meaning: &'static str,
    pub options: &'static [OptionSpec],
}

/// Every signal this version can emit. A changed rule gets a new version.
pub const CATALOGUE: &[SignalSpec] = &[
    SignalSpec {
        id: "blocked_handoff",
        version: 1,
        category: Category::Friction,
        meaning: "The agent hit a blocker and handed a step back to the person instead of finishing it.",
        options: &[],
    },
    SignalSpec {
        id: "tool_error_streak",
        version: 1,
        category: Category::Failure,
        meaning: "Several tool calls failed in a row.",
        options: &[OptionSpec {
            name: "min_streak",
            default: 3.0,
            min: 2.0,
            max: 20.0,
            meaning: "Consecutive failed tool calls needed to fire.",
        }],
    },
    SignalSpec {
        id: "usage_limit_hit",
        version: 2,
        category: Category::Cost,
        meaning: "A usage, rate or credit limit stopped or slowed the agent.",
        options: &[],
    },
    SignalSpec {
        id: "false_done",
        version: 1,
        category: Category::Failure,
        meaning: "The agent said it was done while its last tool call failed or calls were still running.",
        options: &[],
    },
    SignalSpec {
        id: "aborted_turn",
        version: 1,
        category: Category::Cost,
        meaning: "The person interrupted the turn; its tokens bought nothing finished.",
        options: &[],
    },
    SignalSpec {
        id: "context_loss",
        version: 1,
        category: Category::Context,
        meaning: "The agent compacted its context during this work; earlier detail may be lost.",
        options: &[],
    },
    SignalSpec {
        id: "mood",
        version: 1,
        category: Category::Friction,
        meaning: "How the person sounded: from frustrated (-2) to pleased (+2).",
        options: &[],
    },
    SignalSpec {
        id: "correction_spiral",
        version: 1,
        category: Category::Friction,
        meaning: "Several prompts in a row correct or push back on the agent.",
        options: &[OptionSpec {
            name: "min_prompts",
            default: 3.0,
            min: 2.0,
            max: 20.0,
            meaning: "Consecutive corrective prompts needed to fire.",
        }],
    },
];

/// Look up a signal by id.
pub fn spec(id: &str) -> Option<&'static SignalSpec> {
    CATALOGUE.iter().find(|s| s.id == id)
}

/// Per-devspace choices: which signals run and their option values. Missing values fall
/// back to the catalogue defaults; values outside an option's range are clamped.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Optimistic concurrency token for workspace edits.
    #[serde(default)]
    pub revision: u64,
    /// Cloud rules run only on received projections, not private collector text.
    #[serde(default)]
    pub custom: Vec<crate::custom::CustomSignal>,
    /// Signal ids switched off.
    #[serde(default)]
    pub disabled: std::collections::BTreeSet<String>,
    /// `signal -> option -> value`.
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
}

impl SignalConfig {
    /// Check a config someone submitted: every signal and option must be in the catalogue
    /// and every value inside its range. Returns the first problem in plain words.
    pub fn validate(&self) -> Result<(), String> {
        if self.revision >= 9_007_199_254_740_991 || self.custom.len() > 32 {
            return Err("At most 32 custom signals are supported in this version.".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for signal in &self.custom {
            signal.validate()?;
            if !ids.insert(&signal.id) {
                return Err("Custom signal IDs must be unique.".into());
            }
        }
        for id in &self.disabled {
            if spec(id).is_none() {
                return Err(format!("unknown signal {id:?}"));
            }
        }
        for (id, opts) in &self.options {
            let s = spec(id).ok_or_else(|| format!("unknown signal {id:?}"))?;
            for (name, v) in opts {
                let o = s
                    .options
                    .iter()
                    .find(|o| o.name == name)
                    .ok_or_else(|| format!("signal {id} has no option {name:?}"))?;
                if !v.is_finite() || *v < o.min || *v > o.max {
                    return Err(format!(
                        "{id}.{name} = {v} is outside {}..={}",
                        o.min, o.max
                    ));
                }
            }
        }
        Ok(())
    }

    /// Keep rule identities meaningful for old findings. Pause instead of deleting;
    /// create a fresh ID for a changed definition.
    pub fn validate_update(&self, old: &Self) -> Result<(), String> {
        for previous in &old.custom {
            let Some(next) = self.custom.iter().find(|s| s.id == previous.id) else {
                return Err("Pause an existing signal instead of removing its definition.".into());
            };
            let mut comparable = next.clone();
            comparable.enabled = previous.enabled;
            if comparable != *previous {
                return Err("Create a new signal to change a saved definition.".into());
            }
        }
        Ok(())
    }

    pub fn enabled(&self, signal: &str) -> bool {
        !self.disabled.contains(signal)
    }

    /// The effective value of an option: the configured value clamped to the option's range,
    /// or the catalogue default.
    pub fn option(&self, signal: &str, name: &str) -> f64 {
        let o = spec(signal)
            .and_then(|s| s.options.iter().find(|o| o.name == name))
            .expect("option is in the catalogue");
        self.options
            .get(signal)
            .and_then(|m| m.get(name))
            .map_or(o.default, |v| v.clamp(o.min, o.max))
    }
}

/// Longest excerpt kept as evidence.
const EXCERPT: usize = 160;

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid signal regex")
}

static BLOCKER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(permission (was )?denied|not permitted|access denied|denied by|unauthori[sz]ed|forbidden|http (401|403)\b|requires? (your )?approval|sign[- ]?in required|not (signed|logged) in|could not (authenticate|log in))",
    )
});
static HANDOFF: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(\byou(['’]ll| will)? need to\b|\bcould you\b|\bcan you (please )?(run|create|approve|paste|sign|grant|install|connect|open)\b|\bplease (run|sign in|log in|create|approve|paste|grant|install|connect)\b|\bwaiting (on|for) you\b|\bmanually\b|\byour action\b|\bon your (machine|side|end)\b)",
    )
});
static LIMIT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(usage limit|rate[- ]limit(ed)?|out of (usage )?credits|usagelimitexceeded|too many requests|http 429|status 429|quota (exceeded|reached))",
    )
});
static DONE_CLAIM: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(all done|done\.|it'?s done|is done|completed?|all set|fixed|finished|works now|is working now|ready to go)\b",
    )
});
static NEGATIVE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(\bwtf\b|\bf+u+c+k\w*|\bshit\w*|\bdamn\b|\bstupid\b|\buseless\b|\bwasting my\b|\bstop\b|\bstill (not|doesn'?t|broken|wrong)\b|\bthat'?s (not|wrong)\b|\bnot what i\b|\bwhy (are|did) you\b|\bdoesn'?t work\b|\bdidn'?t work\b|\bagain\b|\bwrong\b|\bbullshit\b|\bi give up\b)",
    )
});
static POSITIVE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(\bperfect\b|\bgreat\b|\bthanks?\b|\bthank you\b|\bawesome\b|\blove (it|this)\b|\bnice\b|\bexcellent\b|\bworks (great|now|perfectly)\b|\bthat'?s it\b)",
    )
});
static CORRECTIVE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(^\s*no\b|\bnot what i\b|\bthat'?s (not|wrong)\b|\bstill\b|\bagain\b|\bwrong\b|\bdoesn'?t\b|\bdidn'?t\b|\bwhy (are|did|is) you\b|\bstop\b|\bi said\b|\binstead\b)",
    )
});

fn excerpt(text: &str, m: regex::Match<'_>) -> String {
    let start = text[..m.start()]
        .char_indices()
        .rev()
        .nth(60)
        .map_or(0, |(i, _)| i);
    let s: String = text[start..].chars().take(EXCERPT).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hit(
    signal: &str,
    severity: Severity,
    summary: String,
    score: Option<f64>,
    evidence: Vec<Evidence>,
) -> SignalHit {
    let version = spec(signal).map_or(0, |s| s.version);
    SignalHit {
        signal: signal.to_string(),
        rule_version: version,
        severity,
        summary,
        score,
        evidence,
    }
}

fn tool_ev(id: &str, text: Option<&str>, m: Option<regex::Match<'_>>) -> Evidence {
    Evidence {
        kind: EvidenceKind::Tool,
        tool_id: Some(id.to_string()),
        excerpt: match (text, m) {
            (Some(t), Some(m)) => Some(excerpt(t, m)),
            _ => None,
        },
    }
}

fn text_ev(kind: EvidenceKind, text: &str, m: regex::Match<'_>) -> Evidence {
    Evidence {
        kind,
        tool_id: None,
        excerpt: Some(excerpt(text, m)),
    }
}

/// Count `rx` matches in `text` (used for mood).
fn count(rx: &Regex, text: &str) -> usize {
    rx.find_iter(text).count()
}

/// Mood of a person's prompt, from -2 (very frustrated) to +2 (very pleased). `None` when neutral.
pub fn mood(prompt: &str) -> Option<(f64, Option<regex::Match<'_>>)> {
    let neg = count(&NEGATIVE, prompt) as f64;
    let pos = count(&POSITIVE, prompt) as f64;
    let letters: Vec<char> = prompt.chars().filter(|c| c.is_alphabetic()).collect();
    let shouting = letters.len() >= 12
        && letters.iter().filter(|c| c.is_uppercase()).count() * 10 >= letters.len() * 6;
    let raw = pos - neg - if shouting { 1.0 } else { 0.0 };
    if raw == 0.0 {
        return None;
    }
    let score = raw.clamp(-2.0, 2.0);
    let m = if score < 0.0 {
        NEGATIVE.find(prompt)
    } else {
        POSITIVE.find(prompt)
    };
    Some((score, m))
}

/// Evaluate the per-turn signals. Text rules need `content`; without it only structural
/// rules (tool errors, aborts, compaction) run.
pub fn evaluate_turn(t: &TurnRecord) -> Vec<SignalHit> {
    evaluate_turn_with(t, &SignalConfig::default())
}

/// [`evaluate_turn`] with a devspace's signal choices.
pub fn evaluate_turn_with(t: &TurnRecord, cfg: &SignalConfig) -> Vec<SignalHit> {
    let min_streak = cfg.option("tool_error_streak", "min_streak") as usize;
    let mut out = Vec::new();
    let input = t.content.as_ref().map(|c| c.input.as_str()).unwrap_or("");
    let output = t.content.as_ref().map(|c| c.output.as_str()).unwrap_or("");
    let human = t.prompt_author.is_none();

    // tool_error_streak
    let mut run: Vec<&str> = Vec::new();
    let mut best: Vec<&str> = Vec::new();
    for tool in &t.tools {
        if tool.status == ToolStatus::Error {
            run.push(&tool.id);
            if run.len() > best.len() {
                best = run.clone();
            }
        } else {
            run.clear();
        }
    }
    if best.len() >= min_streak {
        out.push(hit(
            "tool_error_streak",
            Severity::Warn,
            format!("{} tool calls failed in a row.", best.len()),
            Some(best.len() as f64),
            best.iter().map(|id| tool_ev(id, None, None)).collect(),
        ));
    }

    // blocked_handoff: a blocker in a tool result (or a failed call), then a hand-back in the answer.
    let blockers: Vec<Evidence> = t
        .tools
        .iter()
        .filter_map(|tool| {
            let text = tool.output.as_deref().unwrap_or("");
            BLOCKER
                .find(text)
                .map(|m| tool_ev(&tool.id, Some(text), Some(m)))
        })
        .collect();
    let blocked_by_text = BLOCKER.find(output);
    if let Some(h) = HANDOFF.find(output)
        && (!blockers.is_empty() || blocked_by_text.is_some())
    {
        let mut ev = blockers;
        if let Some(b) = blocked_by_text {
            ev.push(text_ev(EvidenceKind::Response, output, b));
        }
        ev.push(text_ev(EvidenceKind::Response, output, h));
        out.push(hit(
            "blocked_handoff",
            Severity::Warn,
            "The agent was blocked and asked you to do a step it could not finish.".into(),
            None,
            ev,
        ));
    }

    // usage_limit_hit (v2): only failed tool calls and the agent's own answer count. Successful
    // tool output often merely *mentions* rate limits (code, docs), which v1 wrongly matched.
    let limit_ev: Vec<Evidence> = t
        .tools
        .iter()
        .filter(|tool| tool.status == ToolStatus::Error)
        .filter_map(|tool| {
            let text = tool.output.as_deref().unwrap_or("");
            LIMIT
                .find(text)
                .map(|m| tool_ev(&tool.id, Some(text), Some(m)))
        })
        .chain(
            LIMIT
                .find(output)
                .map(|m| text_ev(EvidenceKind::Response, output, m)),
        )
        .collect();
    if !limit_ev.is_empty() {
        out.push(hit(
            "usage_limit_hit",
            Severity::Warn,
            "A usage, rate or credit limit got in the agent's way.".into(),
            None,
            limit_ev,
        ));
    }

    // false_done
    let last_failed = t
        .tools
        .last()
        .is_some_and(|x| x.status == ToolStatus::Error);
    if (last_failed || t.tools_unfinished > 0)
        && let Some(m) = DONE_CLAIM.find(output)
    {
        let mut ev = vec![text_ev(EvidenceKind::Response, output, m)];
        if let Some(last) = t.tools.last().filter(|_| last_failed) {
            ev.push(tool_ev(&last.id, None, None));
        }
        out.push(hit(
            "false_done",
            Severity::Warn,
            if last_failed {
                "The agent said it was done, but its last tool call failed.".into()
            } else {
                format!(
                    "The agent said it was done while {} tool call(s) were still running.",
                    t.tools_unfinished
                )
            },
            None,
            ev,
        ));
    }

    // aborted_turn
    if t.termination == Termination::UserCancel {
        out.push(hit(
            "aborted_turn",
            Severity::Info,
            "You interrupted this turn; the work it did was not finished.".into(),
            None,
            vec![Evidence {
                kind: EvidenceKind::Turn,
                tool_id: None,
                excerpt: None,
            }],
        ));
    }

    // context_loss
    if t.compacted {
        out.push(hit(
            "context_loss",
            Severity::Info,
            "The agent compacted its memory of this conversation; check it still has the details it needs.".into(),
            None,
            vec![Evidence {
                kind: EvidenceKind::Turn,
                tool_id: None,
                excerpt: None,
            }],
        ));
    }

    // mood (only for prompts a person wrote)
    if human && let Some((score, m)) = mood(input) {
        out.push(hit(
            "mood",
            if score <= -2.0 {
                Severity::Warn
            } else {
                Severity::Info
            },
            if score < 0.0 {
                "You sounded frustrated with the agent here.".into()
            } else {
                "You sounded pleased with the agent here.".into()
            },
            Some(score),
            m.map(|m| text_ev(EvidenceKind::Prompt, input, m))
                .into_iter()
                .collect(),
        ));
    }
    out.retain(|h| cfg.enabled(&h.signal));
    out
}

/// Evaluate per-turn signals for every turn, then session signals across turns of the same
/// session (in order). Session hits attach to the turn where the pattern completes.
pub fn evaluate_session(turns: &mut [TurnRecord]) {
    evaluate_session_with(turns, &SignalConfig::default());
}

/// [`evaluate_session`] with a devspace's signal choices. Disabled signals are not emitted.
pub fn evaluate_session_with(turns: &mut [TurnRecord], cfg: &SignalConfig) {
    let spiral_len = cfg.option("correction_spiral", "min_prompts") as usize;
    for t in turns.iter_mut() {
        t.signals = evaluate_turn_with(t, cfg);
    }
    let mut streak: Vec<usize> = Vec::new();
    for i in 0..turns.len() {
        let t = &turns[i];
        if t.prompt_author.is_some() {
            continue;
        }
        let input = t.content.as_ref().map(|c| c.input.as_str()).unwrap_or("");
        if CORRECTIVE.is_match(input) {
            streak.push(i);
        } else {
            streak.clear();
        }
        if streak.len() == spiral_len && cfg.enabled("correction_spiral") {
            let ev: Vec<Evidence> = streak
                .iter()
                .filter_map(|&j| {
                    let p = turns[j].content.as_ref().map(|c| c.input.as_str())?;
                    CORRECTIVE
                        .find(p)
                        .map(|m| text_ev(EvidenceKind::Prompt, p, m))
                })
                .collect();
            turns[i].signals.push(hit(
                "correction_spiral",
                Severity::Warn,
                format!("{spiral_len} prompts in a row corrected the agent."),
                Some(spiral_len as f64),
                ev,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Content, ToolRecord, Usage};

    fn turn(input: &str, output: &str, tools: Vec<ToolRecord>) -> TurnRecord {
        TurnRecord {
            schema: crate::SCHEMA.into(),
            event_id: "e".into(),
            agent: "claude-code".into(),
            agent_version: "x".into(),
            provider: "anthropic".into(),
            model: None,
            session_id: "s".into(),
            turn_key: "k".into(),
            subagent: None,
            prompt_author: None,
            started_at: "2026-10-26T00:00:00Z".into(),
            ended_at: "2026-10-26T00:01:00Z".into(),
            cwd: "/w".into(),
            git_branch: "main".into(),
            project: None,
            usage: Usage::default(),
            termination: Termination::Completed,
            compacted: false,
            tools,
            tools_unfinished: 0,
            content: Some(Content {
                input: input.into(),
                output: output.into(),
            }),
            signals: vec![],
            collector: "test".into(),
        }
    }
    fn tool(id: &str, status: ToolStatus, output: &str) -> ToolRecord {
        ToolRecord {
            id: id.into(),
            name: "Bash".into(),
            started_at: "a".into(),
            ended_at: "b".into(),
            status,
            input: None,
            output: Some(output.into()),
        }
    }
    fn ids(hits: &[SignalHit]) -> Vec<&str> {
        hits.iter().map(|h| h.signal.as_str()).collect()
    }

    #[test]
    fn blocked_handoff_needs_a_blocker_and_a_hand_back() {
        let t = turn(
            "set up the repo",
            "Repo creation was blocked. You'll need to create it on GitHub, then I'll push.",
            vec![tool(
                "t1",
                ToolStatus::Error,
                "HTTP 403: Resource not accessible; permission denied",
            )],
        );
        let hits = evaluate_turn(&t);
        let h = hits
            .iter()
            .find(|h| h.signal == "blocked_handoff")
            .expect("hit");
        assert!(
            h.evidence
                .iter()
                .any(|e| e.tool_id.as_deref() == Some("t1"))
        );
        // A hand-back with no blocker is just a question, not this signal.
        let t = turn("x", "Could you tell me which color you prefer?", vec![]);
        assert!(!ids(&evaluate_turn(&t)).contains(&"blocked_handoff"));
    }

    #[test]
    fn three_failures_in_a_row_is_a_streak() {
        let t = turn(
            "x",
            "y",
            vec![
                tool("a", ToolStatus::Error, ""),
                tool("b", ToolStatus::Error, ""),
                tool("c", ToolStatus::Error, ""),
                tool("d", ToolStatus::Ok, ""),
            ],
        );
        let h = evaluate_turn(&t)
            .into_iter()
            .find(|h| h.signal == "tool_error_streak")
            .unwrap();
        assert_eq!(h.evidence.len(), 3);
    }

    #[test]
    fn done_claim_after_a_failed_last_call_is_false_done() {
        let t = turn(
            "fix it",
            "All done. The build is fixed.",
            vec![tool("a", ToolStatus::Error, "error[E0502]")],
        );
        assert!(ids(&evaluate_turn(&t)).contains(&"false_done"));
        let t = turn("fix it", "All done.", vec![tool("a", ToolStatus::Ok, "ok")]);
        assert!(!ids(&evaluate_turn(&t)).contains(&"false_done"));
    }

    #[test]
    fn usage_limits_are_caught_in_tool_output_and_response() {
        let t = turn(
            "x",
            "You're out of usage credits. Switch models to continue.",
            vec![],
        );
        assert!(ids(&evaluate_turn(&t)).contains(&"usage_limit_hit"));
    }

    #[test]
    fn limit_mentions_in_successful_tool_output_do_not_count() {
        let t = turn(
            "read the code",
            "Here is how the module works.",
            vec![tool(
                "a",
                ToolStatus::Ok,
                "Self::RateLimit => \"rate-limit\", // retry on HTTP 429",
            )],
        );
        assert!(!ids(&evaluate_turn(&t)).contains(&"usage_limit_hit"));
        let t = turn(
            "x",
            "y",
            vec![tool(
                "a",
                ToolStatus::Error,
                "429 Too Many Requests: rate limited",
            )],
        );
        assert!(ids(&evaluate_turn(&t)).contains(&"usage_limit_hit"));
    }

    #[test]
    fn mood_runs_both_ways_and_ignores_agent_prompts() {
        assert!(mood("that's perfect, thank you").unwrap().0 > 0.0);
        assert!(
            mood("why are you not doing this, it still doesn't work")
                .unwrap()
                .0
                < 0.0
        );
        assert!(mood("add a button to the header").is_none());
        let mut t = turn("this is wrong again", "ok", vec![]);
        t.prompt_author = Some("/root".into());
        assert!(!ids(&evaluate_turn(&t)).contains(&"mood"));
    }

    #[test]
    fn correction_spiral_fires_once_on_the_third_corrective_prompt() {
        let mut turns = vec![
            turn("build the page", "done", vec![]),
            turn("no, that's not what I asked", "ok", vec![]),
            turn("still wrong", "ok", vec![]),
            turn("why are you doing that again", "ok", vec![]),
        ];
        evaluate_session(&mut turns);
        assert!(ids(&turns[3].signals).contains(&"correction_spiral"));
        assert!(!ids(&turns[2].signals).contains(&"correction_spiral"));
    }

    #[test]
    fn stripping_content_keeps_signals_but_drops_text() {
        let mut t = turn(
            "x",
            "You'll need to sign in manually.",
            vec![tool("a", ToolStatus::Error, "not signed in")],
        );
        t.signals = evaluate_turn(&t);
        t.strip_content();
        assert!(t.content.is_none());
        assert!(!t.signals.is_empty());
        assert!(
            t.signals
                .iter()
                .flat_map(|s| &s.evidence)
                .all(|e| e.excerpt.is_none())
        );
        assert!(t.tools.iter().all(|x| x.output.is_none()));
    }

    #[test]
    fn every_emitted_signal_is_catalogued() {
        let mut turns = vec![turn(
            "WHY IS THIS STILL BROKEN wtf",
            "All done. You'll need to approve it manually. Rate limit reached.",
            vec![
                tool("a", ToolStatus::Error, "permission denied"),
                tool("b", ToolStatus::Error, ""),
                tool("c", ToolStatus::Error, ""),
            ],
        )];
        turns[0].compacted = true;
        turns[0].termination = Termination::UserCancel;
        evaluate_session(&mut turns);
        for h in &turns[0].signals {
            assert!(spec(&h.signal).is_some(), "{} not in CATALOGUE", h.signal);
            assert!(h.rule_version > 0);
        }
    }

    #[test]
    fn config_disables_signals_and_tunes_thresholds() {
        let tools = vec![
            tool("a", ToolStatus::Error, ""),
            tool("b", ToolStatus::Error, ""),
        ];
        let t = turn("this is wrong", "y", tools);
        // Default: a streak needs 3.
        assert!(!ids(&evaluate_turn(&t)).contains(&"tool_error_streak"));
        let mut cfg = SignalConfig::default();
        cfg.options
            .entry("tool_error_streak".into())
            .or_default()
            .insert("min_streak".into(), 2.0);
        assert!(ids(&evaluate_turn_with(&t, &cfg)).contains(&"tool_error_streak"));
        // Out-of-range values are clamped to the option's range (min 2).
        cfg.options
            .get_mut("tool_error_streak")
            .unwrap()
            .insert("min_streak".into(), 0.0);
        assert_eq!(cfg.option("tool_error_streak", "min_streak"), 2.0);
        // Disabled signals are never emitted.
        cfg.disabled.insert("mood".into());
        assert!(!ids(&evaluate_turn_with(&t, &cfg)).contains(&"mood"));
        assert!(ids(&evaluate_turn(&t)).contains(&"mood"));
    }

    #[test]
    fn every_catalogue_option_default_is_in_range() {
        for s in CATALOGUE {
            for o in s.options {
                assert!(
                    o.min <= o.default && o.default <= o.max,
                    "{}.{}",
                    s.id,
                    o.name
                );
            }
        }
    }

    #[test]
    fn validate_rejects_unknown_names_and_out_of_range_values() {
        let mut c = SignalConfig::default();
        assert!(c.validate().is_ok());
        c.disabled.insert("mood".into());
        c.options
            .entry("tool_error_streak".into())
            .or_default()
            .insert("min_streak".into(), 5.0);
        assert!(c.validate().is_ok());

        let mut bad = c.clone();
        bad.disabled.insert("no_such_signal".into());
        assert!(bad.validate().unwrap_err().contains("no_such_signal"));

        let mut bad = c.clone();
        bad.options
            .entry("mood".into())
            .or_default()
            .insert("x".into(), 1.0);
        assert!(bad.validate().unwrap_err().contains("no option"));

        let mut bad = c;
        bad.options
            .entry("tool_error_streak".into())
            .or_default()
            .insert("min_streak".into(), 99.0);
        assert!(bad.validate().unwrap_err().contains("outside"));
    }
}
