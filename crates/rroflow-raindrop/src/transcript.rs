//! Claude Code session transcripts (JSONL) parsed into one `Turn` per real user prompt.

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// User-message prefixes written by the harness, not typed by a person.
pub(crate) const HARNESS_PREFIXES: [&str; 4] = [
    "<task-notification>",
    "<local-command-",
    "<command-name>",
    "<command-message>",
];

/// Input recorded when a compaction summary opens a turn and no prompt text survives.
pub(crate) const COMPACTION_MARKER: &str = "[continued after context compaction]";

pub(crate) fn strip_reminders(s: &str) -> String {
    let re = Regex::new(r"(?s)<system-reminder>.*?</system-reminder>").unwrap();
    re.replace_all(s, "").trim().to_string()
}

pub(crate) fn ts_nanos(ts: &str) -> Option<i128> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.timestamp_nanos_opt().unwrap_or(0) as i128)
}

#[derive(Default)]
pub(crate) struct Tool {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) input: String,
    pub(crate) start: String,
    pub(crate) end: Option<String>,
    pub(crate) output: String,
    pub(crate) error: bool,
}

#[derive(Default)]
pub(crate) struct Turn {
    pub(crate) key: String,
    pub(crate) session: String,
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) input: String,
    pub(crate) output: Vec<String>,
    pub(crate) model: Option<String>,
    pub(crate) in_tokens: u64,
    pub(crate) out_tokens: u64,
    pub(crate) tools: Vec<Tool>,
    pub(crate) cwd: String,
    pub(crate) branch: String,
    pub(crate) version: String,
    pub(crate) entrypoint: String,
    /// Set when the turn resumes after a context-compaction summary.
    pub(crate) compacted: bool,
}

fn text_of(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(blocks) => {
            if blocks.iter().any(|b| b["type"] == "tool_result") {
                return None;
            }
            let t: Vec<&str> = blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            if t.is_empty() {
                None
            } else {
                Some(t.join("\n"))
            }
        }
        _ => None,
    }
}

pub(crate) fn parse_transcript(path: &PathBuf) -> Result<Vec<Turn>> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut turns: Vec<Turn> = Vec::new();
    let mut open: HashMap<String, (usize, usize)> = HashMap::new();
    // promptId -> prompt text, so a compaction summary can point back at the prompt it continues.
    let mut prompts: HashMap<String, String> = HashMap::new();
    // Claude Code writes `last-prompt` records; they survive compaction when the prompt itself
    // does not, and they may be written after the summary, so collect them first.
    let last_prompts: Vec<(usize, String)> = raw
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let d = serde_json::from_str::<Value>(l).ok()?;
            (d["type"] == "last-prompt")
                .then(|| d["lastPrompt"].as_str().map(str::to_string))
                .flatten()
                .filter(|p| !p.trim().is_empty())
                .map(|p| (i, p))
        })
        .collect();
    let last_prompt_near = |line: usize| -> Option<String> {
        last_prompts
            .iter()
            .rev()
            .find(|(i, _)| *i < line)
            .or_else(|| last_prompts.iter().find(|(i, _)| *i > line))
            .map(|(_, p)| p.clone())
    };
    for (line_no, line) in raw.lines().enumerate() {
        let Ok(d) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ty = d["type"].as_str().unwrap_or("");
        let ts = d["timestamp"].as_str().unwrap_or("").to_string();
        let msg = &d["message"];
        match ty {
            "user" => {
                let is_meta = d["isMeta"].as_bool().unwrap_or(false);
                if let Some(text) = text_of(&msg["content"]) {
                    let text = strip_reminders(&text);
                    let harness = HARNESS_PREFIXES.iter().any(|p| text.starts_with(p));
                    if is_meta || text.is_empty() || harness {
                        continue;
                    }
                    // A compaction summary is harness text, not a prompt: the turn it opens
                    // continues the prompt named in summarizedTurnPromptUuids.
                    let compacted = d["isCompactSummary"].as_bool().unwrap_or(false);
                    let text = if compacted {
                        d["summarizedTurnPromptUuids"][0]
                            .as_str()
                            .and_then(|id| prompts.get(id).cloned())
                            .or_else(|| last_prompt_near(line_no))
                            .unwrap_or_else(|| COMPACTION_MARKER.into())
                    } else {
                        if let Some(id) = d["promptId"].as_str() {
                            prompts.insert(id.to_string(), text.clone());
                        }
                        text
                    };
                    turns.push(Turn {
                        key: format!(
                            "{}:{}",
                            d["agentId"].as_str().unwrap_or("main"),
                            d["promptId"].as_str().or(d["uuid"].as_str()).unwrap_or("")
                        ),
                        session: d["sessionId"].as_str().unwrap_or("").to_string(),
                        start: ts.clone(),
                        end: ts,
                        input: text,
                        cwd: d["cwd"].as_str().unwrap_or("").to_string(),
                        branch: d["gitBranch"].as_str().unwrap_or("").to_string(),
                        version: d["version"].as_str().unwrap_or("").to_string(),
                        entrypoint: d["entrypoint"].as_str().unwrap_or("").to_string(),
                        compacted,
                        ..Default::default()
                    });
                } else if let Some(blocks) = msg["content"].as_array() {
                    let n = turns.len();
                    let Some(t) = turns.last_mut() else { continue };
                    for b in blocks.iter().filter(|b| b["type"] == "tool_result") {
                        let id = b["tool_use_id"].as_str().unwrap_or("");
                        if let Some(&(ti, k)) = open.get(id)
                            && ti + 1 == n
                        {
                            let tool = &mut t.tools[k];
                            tool.end = Some(ts.clone());
                            tool.error = b["is_error"].as_bool().unwrap_or(false);
                            tool.output = match &b["content"] {
                                Value::String(s) => s.clone(),
                                v => text_of(v).unwrap_or_else(|| v.to_string()),
                            };
                        }
                        if !ts.is_empty() {
                            t.end = ts.clone();
                        }
                    }
                }
            }
            "assistant" => {
                let n = turns.len();
                let Some(t) = turns.last_mut() else { continue };
                if let Some(m) = msg["model"].as_str() {
                    t.model = Some(m.to_string());
                }
                t.in_tokens += msg["usage"]["input_tokens"].as_u64().unwrap_or(0);
                t.out_tokens += msg["usage"]["output_tokens"].as_u64().unwrap_or(0);
                if !ts.is_empty() {
                    t.end = ts.clone();
                }
                for b in msg["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => t.output.push(b["text"].as_str().unwrap_or("").to_string()),
                        Some("tool_use") => {
                            let id = b["id"].as_str().unwrap_or("").to_string();
                            open.insert(id.clone(), (n - 1, t.tools.len()));
                            t.tools.push(Tool {
                                id,
                                name: b["name"].as_str().unwrap_or("tool").to_string(),
                                input: b["input"].to_string(),
                                start: ts.clone(),
                                ..Default::default()
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(turns)
}

/// Expand files and directories into the `.jsonl` transcripts they contain, sorted.
pub(crate) fn collect(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut sub: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
                sub.sort();
                out.extend(collect(&sub));
            }
        } else if p.extension().is_some_and(|e| e == "jsonl") {
            out.push(p.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compaction_summary_continues_the_original_prompt() {
        let lines = [
            r#"{"type":"user","promptId":"p1","sessionId":"s","timestamp":"2026-10-25T00:00:00Z","message":{"content":"build the adapter"}}"#,
            r#"{"type":"user","promptId":"p2","sessionId":"s","timestamp":"2026-10-25T01:00:00Z","isCompactSummary":true,"summarizedTurnPromptUuids":["p1"],"message":{"content":"This session is being continued from a previous conversation..."}}"#,
        ];
        let path = std::env::temp_dir().join("rroflow-raindrop-compact-test.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        let turns = parse_transcript(&path).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].input, "build the adapter");
        assert!(turns[1].compacted && !turns[0].compacted);
    }
    #[test]
    fn compaction_falls_back_to_last_prompt_record() {
        // Claude Code writes the last-prompt record after the summary.
        let lines = [
            r#"{"type":"user","promptId":"p2","sessionId":"s","timestamp":"2026-10-25T01:00:00Z","isCompactSummary":true,"summarizedTurnPromptUuids":["gone"],"message":{"content":"This session is being continued..."}}"#,
            r#"{"type":"last-prompt","lastPrompt":"absorb raindrop","sessionId":"s"}"#,
        ];
        let path = std::env::temp_dir().join("rroflow-raindrop-compact-fallback-test.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        let turns = parse_transcript(&path).unwrap();
        assert_eq!(turns[0].input, "absorb raindrop");
    }
}
