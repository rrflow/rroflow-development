//! Bounded, user-authored rules over the projection available to cloud ingest.
//! These are explicit rules, not learned classifiers or provider instructions.
use crate::record::{Evidence, EvidenceKind, Severity, SignalHit, ToolStatus, TurnRecord};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    ResponseContains { text: String },
    PromptContains { text: String },
    ToolNamed { name: String },
    ToolErrors { at_least: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomSignal {
    pub id: String,
    pub name: String,
    pub group: String,
    pub enabled: bool,
    pub condition: Condition,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

pub enum Evaluation {
    OutOfScope,
    Unavailable,
    NoMatch,
    Match(SignalHit),
}

impl CustomSignal {
    pub fn validate(&self) -> Result<(), String> {
        if !self.id.starts_with("custom_")
            || !(12..=64).contains(&self.id.len())
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("Custom signal ID must use the custom_ namespace.".into());
        }
        for (label, value, max) in [
            ("Name", self.name.as_str(), 80),
            ("Group", self.group.as_str(), 40),
        ] {
            if value.trim().is_empty()
                || value.chars().count() > max
                || value.chars().any(char::is_control)
            {
                return Err(format!("{label} must be 1–{max} visible characters."));
            }
        }
        for value in [&self.agent, &self.project].into_iter().flatten() {
            if value.is_empty() || value.len() > 1024 {
                return Err("Scope must be nonempty and at most 1024 bytes.".into());
            }
        }
        match &self.condition {
            Condition::ResponseContains { text }
            | Condition::PromptContains { text }
            | Condition::ToolNamed { name: text } => {
                if text.trim().is_empty()
                    || text.chars().count() > 160
                    || text.chars().any(char::is_control)
                {
                    return Err("Match text must be 1–160 visible characters.".into());
                }
            }
            Condition::ToolErrors { at_least } if !(1..=100).contains(at_least) => {
                return Err("Choose 1–100 failed tools.".into());
            }
            _ => {}
        }
        Ok(())
    }

    /// Preview deliberately ignores enabled; the ingestion caller checks it.
    pub fn evaluate(&self, t: &TurnRecord) -> Evaluation {
        let project = t
            .project
            .as_deref()
            .filter(|p| !p.is_empty())
            .unwrap_or(if t.cwd.is_empty() { "(none)" } else { &t.cwd });
        if self.agent.as_deref().is_some_and(|a| a != t.agent)
            || self.project.as_deref().is_some_and(|p| p != project)
        {
            return Evaluation::OutOfScope;
        }
        let mut evidence = vec![];
        match &self.condition {
            Condition::ResponseContains { text } | Condition::PromptContains { text } => {
                let Some(content) = &t.content else {
                    return Evaluation::Unavailable;
                };
                let prompt = matches!(self.condition, Condition::PromptContains { .. });
                let value = if prompt {
                    &content.input
                } else {
                    &content.output
                };
                if !value.to_lowercase().contains(&text.to_lowercase()) {
                    return Evaluation::NoMatch;
                }
                // A small source excerpt, never interpreted as HTML. Full text stays on the turn.
                evidence.push(Evidence {
                    kind: if prompt {
                        EvidenceKind::Prompt
                    } else {
                        EvidenceKind::Response
                    },
                    tool_id: None,
                    excerpt: Some(text.clone()),
                });
            }
            Condition::ToolNamed { name } => {
                evidence = t
                    .tools
                    .iter()
                    .filter(|tool| tool.name.eq_ignore_ascii_case(name))
                    .take(100)
                    .map(|tool| Evidence {
                        kind: EvidenceKind::Tool,
                        tool_id: Some(tool.id.clone()),
                        excerpt: None,
                    })
                    .collect();
                if evidence.is_empty() {
                    return Evaluation::NoMatch;
                }
            }
            Condition::ToolErrors { at_least } => {
                let failed: Vec<_> = t
                    .tools
                    .iter()
                    .filter(|tool| tool.status == ToolStatus::Error)
                    .collect();
                if failed.len() < *at_least as usize {
                    return Evaluation::NoMatch;
                }
                evidence = failed
                    .into_iter()
                    .take(100)
                    .map(|tool| Evidence {
                        kind: EvidenceKind::Tool,
                        tool_id: Some(tool.id.clone()),
                        excerpt: None,
                    })
                    .collect();
            }
        }
        Evaluation::Match(SignalHit {
            signal: self.id.clone(),
            rule_version: 1,
            severity: Severity::Info,
            summary: format!("Your rule ‘{}’ matched this turn.", self.name),
            score: None,
            evidence,
        })
    }
}
