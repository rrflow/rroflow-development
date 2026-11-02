//! Claude Code session transcripts (JSONL) parsed into one `Turn` per real user prompt.

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

mod usage;

/// User-message prefixes written by the harness, not typed by a person.
pub(crate) const HARNESS_PREFIXES: [&str; 5] = [
    "<task-notification>",
    "<local-command-",
    "<command-name>",
    "<command-message>",
    "Caveat: The messages below were generated",
];

/// Text Claude Code writes as a user message when the person interrupts a turn
/// (the same marker DevPulse's transcript extractor drops: platform_devpulse
/// docs/chat-modes/_raw/extract-prompts.js).
pub(crate) const INTERRUPT_PREFIX: &str = "[Request interrupted by user";

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
    /// Cached input read (Anthropic `cache_read_input_tokens`, OpenAI `cached_input_tokens`).
    pub(crate) cache_read: u64,
    /// Cache writes (Anthropic `cache_creation_input_tokens`).
    pub(crate) cache_write: u64,
    /// Completeness of each hosted usage total, independent of model availability.
    /// The legacy numeric totals above remain available to the Raindrop sink.
    pub(crate) usage_known: [bool; 4],
    pub(crate) usage_seen: bool,
    /// Identity-bound provider usage supersedes legacy notification totals for
    /// hosted records, without changing the Raindrop compatibility projection.
    pub(crate) hosted_usage: Option<rroflow_core::Usage>,
    pub(crate) tools: Vec<Tool>,
    pub(crate) cwd: String,
    pub(crate) branch: String,
    pub(crate) version: String,
    pub(crate) entrypoint: String,
    /// Set when the turn resumes after a context-compaction summary.
    pub(crate) compacted: bool,
    /// Which coding agent wrote the transcript: `claude-code` or `codex`.
    pub(crate) agent: &'static str,
    /// Model provider, used as `gen_ai.system` (e.g. `anthropic`, `openai`).
    pub(crate) provider: String,
    /// Set when the prompt came from another agent (a parent handing a subagent a task),
    /// not a person: the sender's agent path, e.g. `/root`.
    pub(crate) prompt_author: Option<String>,
    /// The subagent this transcript belongs to, e.g. `/root/storage_plan_review`.
    pub(crate) subagent: Option<String>,
    /// The turn was interrupted (Codex `turn_aborted`).
    pub(crate) aborted: bool,
}

impl Turn {
    /// Merge one usage-bearing message. A missing/malformed field makes that
    /// turn total unknown, even when other messages reported the field. Explicit
    /// zero remains known; arithmetic overflow never becomes a priced total.
    pub(crate) fn observe_usage(&mut self, values: [Option<u64>; 4]) {
        let totals = [
            &mut self.in_tokens,
            &mut self.out_tokens,
            &mut self.cache_read,
            &mut self.cache_write,
        ];
        for (i, (total, value)) in totals.into_iter().zip(values).enumerate() {
            let sum = total.checked_add(value.unwrap_or(0));
            self.usage_known[i] =
                (!self.usage_seen || self.usage_known[i]) && value.is_some() && sum.is_some();
            // Preserve the legacy sink's numeric projection for valid inputs.
            // Invalid overflow is bounded, while hosted usage remains unknown.
            *total = sum.unwrap_or(u64::MAX);
        }
        self.usage_seen = true;
    }
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

pub(crate) fn parse_transcript(path: &std::path::Path) -> Result<Vec<Turn>> {
    parse(path, false)
}

pub(crate) fn collect_transcript(path: &std::path::Path) -> Result<Vec<Turn>> {
    parse(path, true)
}

fn parse(path: &std::path::Path, bounded_text: bool) -> Result<Vec<Turn>> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut message_usage = usage::MessageUsage::default();
    let mut open: HashMap<String, (usize, usize)> = HashMap::new();
    // promptId -> prompt text, so a compaction summary can point back at the prompt it continues.
    let mut prompts: HashMap<String, String> = HashMap::new();
    // Claude Code writes `last-prompt` records; they survive compaction when the prompt itself
    // does not, and they may be written after the summary, so collect them first.
    let mut last_prompts = Vec::new();
    for entry in read_records(path, bounded_text)? {
        let (i, d) = entry?;
        if d["type"] == "last-prompt"
            && let Some(prompt) = d["lastPrompt"].as_str().filter(|p| !p.trim().is_empty())
        {
            last_prompts.push((i, prompt.to_string()));
        }
    }
    let last_prompt_near = |line: usize| -> Option<String> {
        last_prompts
            .iter()
            .rev()
            .find(|(i, _)| *i < line)
            .or_else(|| last_prompts.iter().find(|(i, _)| *i > line))
            .map(|(_, p)| p.clone())
    };
    for entry in read_records(path, bounded_text)? {
        let (line_no, d) = entry?;
        let ty = d["type"].as_str().unwrap_or("");
        let ts = d["timestamp"].as_str().unwrap_or("").to_string();
        let msg = &d["message"];
        match ty {
            "user" => {
                let is_meta = d["isMeta"].as_bool().unwrap_or(false);
                if let Some(text) = text_of(&msg["content"]) {
                    let text = strip_reminders(&text);
                    if text.starts_with(INTERRUPT_PREFIX) {
                        if let Some(t) = turns.last_mut() {
                            t.aborted = true;
                        }
                        continue;
                    }
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
                        // A sidechain (subagent) transcript opens with the parent agent's
                        // prompt, not a person's.
                        prompt_author: d["isSidechain"]
                            .as_bool()
                            .unwrap_or(false)
                            .then(|| "parent agent".to_string()),
                        agent: "claude-code",
                        provider: "anthropic".into(),
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
                message_usage.observe(n - 1, &t.session, &d);
                if let Some(m) = msg["model"].as_str() {
                    t.model = Some(m.to_string());
                }
                t.observe_usage([
                    msg["usage"]["input_tokens"].as_u64(),
                    msg["usage"]["output_tokens"].as_u64(),
                    msg["usage"]["cache_read_input_tokens"].as_u64(),
                    msg["usage"]["cache_creation_input_tokens"].as_u64(),
                ]);
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
    message_usage.apply(&mut turns);
    Ok(turns)
}

/// A `.jsonl` transcript, or a `.jsonl.zst` one (Codex compresses older rollouts).
fn is_transcript(p: &std::path::Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".jsonl") || name.ends_with(".jsonl.zst")
}

/// Stream records, including compressed rollouts. The bound is per JSON record,
/// never per session file. Large tool strings are bounded only for source collection;
/// the existing one-shot/Raindrop parser keeps its original text semantics.
pub(crate) fn read_records(
    path: &std::path::Path,
    bounded_text: bool,
) -> Result<impl Iterator<Item = Result<(usize, Value)>>> {
    use std::io::{BufRead, Read};
    const LINE_LIMIT: u64 = 64 * 1024 * 1024;
    let file = std::fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
    let mut reader: Box<dyn BufRead> = if path.extension().is_some_and(|e| e == "zst") {
        let dec = ruzstd::decoding::StreamingDecoder::new(std::io::BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("decompress {}: {e}", path.display()))?;
        Box::new(std::io::BufReader::new(dec))
    } else {
        Box::new(std::io::BufReader::new(file))
    };
    let mut number = 0;
    let mut done = false;
    Ok(std::iter::from_fn(move || {
        while !done {
            let result = (|| -> Result<Option<(usize, Value)>> {
                let mut bytes = Vec::new();
                if reader
                    .as_mut()
                    .take(LINE_LIMIT + 1)
                    .read_until(b'\n', &mut bytes)?
                    == 0
                {
                    done = true;
                    return Ok(None);
                }
                anyhow::ensure!(
                    bytes.len() as u64 <= LINE_LIMIT,
                    "JSON record exceeds 64 MiB; coverage incomplete"
                );
                let line = std::str::from_utf8(&bytes).context("session is not UTF-8")?;
                let value = parse_line(line, number, !line.ends_with('\n'))?;
                let index = number;
                number += 1;
                Ok(value.map(|mut v| {
                    if bounded_text {
                        bound_strings(&mut v);
                    }
                    (index, v)
                }))
            })();
            match result {
                Ok(Some(value)) => return Some(Ok(value)),
                Ok(None) => {}
                Err(error) => {
                    done = true;
                    return Some(Err(error));
                }
            }
        }
        None
    }))
}

fn bound_strings(value: &mut Value) {
    match value {
        Value::String(s) if s.len() > 20_000 => *s = crate::redact::truncate(s, 20_000),
        Value::Array(values) => values.iter_mut().for_each(bound_strings),
        Value::Object(fields) => fields.values_mut().for_each(bound_strings),
        _ => {}
    }
}

/// A writer may be halfway through the final JSON line. Corruption anywhere else
/// is an error, not permission to report a silently incomplete session.
pub(crate) fn parse_line(line: &str, line_no: usize, partial_tail: bool) -> Result<Option<Value>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str(line) {
        Ok(value) => Ok(Some(value)),
        Err(error) if partial_tail && error.is_eof() => Ok(None),
        Err(_) => anyhow::bail!("invalid JSON at session line {}", line_no + 1),
    }
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
        } else if is_transcript(p) {
            out.push(p.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_completeness_survives_both_transcript_readers() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("usage.jsonl");
        let lines = [
            serde_json::json!({"type":"user","promptId":"p","sessionId":"s","message":{"content":"work"}}),
            serde_json::json!({"type":"assistant","message":{"id":"message-1","content":[],"usage":{"input_tokens":0,"output_tokens":"unknown","cache_read_input_tokens":0}}}),
            serde_json::json!({"type":"assistant","message":{"id":"message-2","content":[],"usage":{"input_tokens":7,"output_tokens":4,"cache_read_input_tokens":0,"cache_creation_input_tokens":9}}}),
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
            let turns = parse(&path, bounded).unwrap();
            assert_eq!(turns.len(), 1);
            let record = crate::record::to_record(&turns[0], None, &crate::redact::Redactor::new());
            assert_eq!(record.model, None);
            assert_eq!(record.usage.input_tokens, Some(7));
            assert_eq!(record.usage.output_tokens, None);
            assert_eq!(record.usage.cache_read_tokens, Some(0));
            assert_eq!(record.usage.cache_write_tokens, None);
        }
    }

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
