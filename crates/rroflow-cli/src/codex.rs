//! Codex CLI session rollouts (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl[.zst]`)
//! parsed into one `Turn` per user prompt.
//!
//! Format pinned from `openai/codex` at 970111f (2026-10-23): every line is
//! `{timestamp, type, payload}` with `type` one of `session_meta`, `turn_context`,
//! `response_item`, `event_msg`, `compacted`, `token_usage_record`, …
//! (`codex-rs/history/src/rollout_payload.rs`). Unknown types are ignored, so newer
//! Codex versions degrade to fewer fields rather than failing.
//!
//! - Turns are bounded by `event_msg` `task_started` / `task_complete` (`turn_id`).
//!   Older rollouts lack them; a new prompt after assistant output then opens a turn.
//! - The person's prompt: `event_msg` `user_message` (legacy history mode) or
//!   `item_completed` with a `UserMessage` item (paginated mode). A `response_item`
//!   user message is the fallback, skipping the context Codex injects
//!   (`<environment_context>`, `<user_instructions>`, AGENTS.md).
//! - Tools: `function_call`, `custom_tool_call`, `local_shell_call` paired with
//!   `function_call_output` / `custom_tool_call_output` by `call_id`; `web_search_call`
//!   has no output record. Codex persists no success flag, so a failure is read from
//!   the output: `Exit code: N` (text form) or `metadata.exit_code` (JSON form).
//! - Model from `turn_context.model`; tokens summed from `token_count`
//!   `info.last_token_usage` (OpenAI's `input_tokens` already includes cached input).

use crate::transcript::{Tool, Turn, read_text};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Text Codex injects as user-role messages; never the person's prompt.
const INJECTED_PREFIXES: [&str; 5] = [
    "<environment_context>",
    "<user_instructions>",
    "<permissions instructions>",
    "# AGENTS.md instructions",
    "<turn_aborted>",
];

fn is_injected(text: &str) -> bool {
    let t = text.trim_start();
    INJECTED_PREFIXES.iter().any(|p| t.starts_with(p))
}

fn text_items(content: &Value, kinds: &[&str]) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| kinds.contains(&c["type"].as_str().unwrap_or("")))
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tool output body: a string, or a list of content items.
fn output_text(output: &Value) -> String {
    match output {
        Value::String(s) => s.clone(),
        Value::Array(_) => text_items(output, &["input_text", "output_text", "text"]),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// Codex records no success flag for tool calls; the exit code is in the output.
fn failed(output: &str) -> bool {
    if let Some(rest) = output.trim_start().strip_prefix("Exit code:") {
        return rest
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<i64>().ok())
            .is_some_and(|n| n != 0);
    }
    serde_json::from_str::<Value>(output)
        .ok()
        .and_then(|v| v["metadata"]["exit_code"].as_i64())
        .is_some_and(|n| n != 0)
        // The scripted `exec` tool (observed in Codex 0.164 rollouts) wraps each inner command's
        // result as JSON inside its text output: `{"chunk_id":…,"exit_code":1,…}`.
        || EMBEDDED_EXIT
            .captures_iter(output)
            .any(|c| c[1].parse::<i64>().is_ok_and(|n| n != 0))
}

static EMBEDDED_EXIT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r#""exit_code"\s*:\s*(-?\d+)"#).unwrap());

/// Milliseconds encoded in a UUIDv7 (Codex thread and turn ids), or `None` for other ids.
fn uuid7_ms(id: &str) -> Option<u64> {
    let hex: String = id.chars().filter(|c| *c != '-').take(12).collect();
    (hex.len() == 12 && id.len() >= 15 && &id[14..15] == "7")
        .then(|| u64::from_str_radix(&hex, 16).ok())
        .flatten()
}

#[derive(Default)]
struct Session {
    id: String,
    cwd: String,
    version: String,
    branch: String,
    originator: String,
    provider: String,
    model: Option<String>,
    subagent: Option<String>,
}

pub(crate) fn parse_rollout(path: &Path) -> Result<Vec<Turn>> {
    let raw = read_text(path)?;
    let mut s = Session {
        provider: "openai".into(),
        ..Default::default()
    };
    let mut turns: Vec<Turn> = Vec::new();
    // Whether the last turn has finished (task_complete) or produced output since its prompt.
    let mut closed = true;
    // call_id -> (turn index, tool index)
    let mut open: HashMap<String, (usize, usize)> = HashMap::new();
    let mut ordinal = 0usize;
    // A forked (subagent) rollout starts with its own session_meta, then a copy of the parent's
    // session_meta and history. That copy is the parent's work, already in the parent's own
    // rollout, so it is skipped until the child's first own turn.
    let mut seen_meta = false;
    let mut inherited = false;

    let new_turn = |turns: &mut Vec<Turn>, s: &Session, key: String, ts: &str| {
        turns.push(Turn {
            key,
            session: s.id.clone(),
            start: ts.to_string(),
            end: ts.to_string(),
            model: s.model.clone(),
            cwd: s.cwd.clone(),
            branch: s.branch.clone(),
            version: s.version.clone(),
            entrypoint: s.originator.clone(),
            agent: "codex",
            provider: s.provider.clone(),
            subagent: s.subagent.clone(),
            ..Default::default()
        });
    };

    for line in raw.lines() {
        let Ok(d) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ts = d["timestamp"].as_str().unwrap_or("").to_string();
        let p = &d["payload"];
        let kind = d["type"].as_str().unwrap_or("");
        if inherited {
            let child_turn = kind == "event_msg"
                && matches!(p["type"].as_str(), Some("task_started" | "turn_started"))
                && match (p["turn_id"].as_str().and_then(uuid7_ms), uuid7_ms(&s.id)) {
                    (Some(turn), Some(thread)) => turn >= thread,
                    _ => false,
                };
            let marker = (kind == "event_msg" && p["type"] == "thread_settings_applied")
                || (kind == "response_item"
                    && p["role"] == "developer"
                    && text_items(&p["content"], &["input_text"])
                        .starts_with("You are now a newly spawned child agent"));
            if marker || child_turn {
                inherited = false;
            }
            if !child_turn {
                continue;
            }
        }
        match kind {
            "session_meta" if seen_meta => {
                // The parent's meta: inherited history follows.
                inherited = true;
            }
            "session_meta" => {
                seen_meta = true;
                s.subagent = p["agent_path"].as_str().map(str::to_string);
                s.id = p["id"].as_str().unwrap_or("").to_string();
                s.cwd = p["cwd"].as_str().unwrap_or("").to_string();
                s.version = p["cli_version"].as_str().unwrap_or("").to_string();
                s.originator = p["originator"].as_str().unwrap_or("").to_string();
                s.branch = p["git"]["branch"].as_str().unwrap_or("").to_string();
                if let Some(pr) = p["model_provider"].as_str().filter(|x| !x.is_empty()) {
                    s.provider = pr.to_string();
                }
            }
            "turn_context" => {
                if let Some(m) = p["model"].as_str() {
                    s.model = Some(m.to_string());
                    if let Some(t) = turns.last_mut().filter(|_| !closed) {
                        t.model = s.model.clone();
                    }
                }
            }
            "event_msg" => match p["type"].as_str().unwrap_or("") {
                "task_started" | "turn_started" => {
                    let id = p["turn_id"].as_str().unwrap_or("").to_string();
                    ordinal += 1;
                    let key = if id.is_empty() {
                        format!("t{ordinal}")
                    } else {
                        id
                    };
                    new_turn(&mut turns, &s, key, &ts);
                    closed = false;
                }
                "turn_aborted" => {
                    if let Some(t) = turns.last_mut().filter(|_| !closed) {
                        t.aborted = true;
                        if !ts.is_empty() {
                            t.end = ts.clone();
                        }
                    }
                    closed = true;
                }
                "task_complete" | "turn_complete" => {
                    if let Some(t) = turns.last_mut() {
                        if t.output.is_empty()
                            && let Some(m) = p["last_agent_message"].as_str()
                        {
                            t.output.push(m.to_string());
                        }
                        if !ts.is_empty() {
                            t.end = ts.clone();
                        }
                    }
                    closed = true;
                }
                "user_message" => {
                    let text = p["message"].as_str().unwrap_or("").trim().to_string();
                    set_prompt(
                        &mut turns,
                        &mut closed,
                        &mut ordinal,
                        &s,
                        &ts,
                        text,
                        &new_turn,
                    );
                }
                "item_completed" if p["item"]["type"] == "UserMessage" => {
                    let text = text_items(&p["item"]["content"], &["text"])
                        .trim()
                        .to_string();
                    set_prompt(
                        &mut turns,
                        &mut closed,
                        &mut ordinal,
                        &s,
                        &ts,
                        text,
                        &new_turn,
                    );
                }
                "token_count" => {
                    let u = &p["info"]["last_token_usage"];
                    if let Some(t) = turns.last_mut() {
                        t.in_tokens += u["input_tokens"].as_u64().unwrap_or(0);
                        t.out_tokens += u["output_tokens"].as_u64().unwrap_or(0);
                        t.cache_read += u["cached_input_tokens"].as_u64().unwrap_or(0);
                    }
                }
                _ => {}
            },
            "response_item" => {
                let n = turns.len();
                match p["type"].as_str().unwrap_or("") {
                    "message" if p["role"] == "user" => {
                        let text = text_items(&p["content"], &["input_text"])
                            .trim()
                            .to_string();
                        if !is_injected(&text) {
                            set_prompt(
                                &mut turns,
                                &mut closed,
                                &mut ordinal,
                                &s,
                                &ts,
                                text,
                                &new_turn,
                            );
                        }
                    }
                    // A task handed to this (sub)agent by another agent.
                    "agent_message" => {
                        let text = text_items(&p["content"], &["input_text", "text"])
                            .trim()
                            .to_string();
                        if set_prompt(
                            &mut turns,
                            &mut closed,
                            &mut ordinal,
                            &s,
                            &ts,
                            text,
                            &new_turn,
                        ) && let Some(t) = turns.last_mut()
                        {
                            t.prompt_author = p["author"].as_str().map(str::to_string);
                        }
                    }
                    "message" if p["role"] == "assistant" => {
                        let text = text_items(&p["content"], &["output_text"]);
                        if let Some(t) = turns.last_mut()
                            && !text.trim().is_empty()
                        {
                            t.output.push(text);
                            t.end = ts.clone();
                        }
                    }
                    kind @ ("function_call" | "custom_tool_call" | "local_shell_call"
                    | "web_search_call") => {
                        let Some(t) = turns.last_mut() else { continue };
                        let (name, input) = match kind {
                            "function_call" => (
                                p["name"].as_str().unwrap_or("tool").to_string(),
                                p["arguments"].as_str().unwrap_or("").to_string(),
                            ),
                            "custom_tool_call" => (
                                p["name"].as_str().unwrap_or("tool").to_string(),
                                p["input"].as_str().unwrap_or("").to_string(),
                            ),
                            "local_shell_call" => ("local_shell".into(), p["action"].to_string()),
                            _ => ("web_search".into(), p["action"].to_string()),
                        };
                        let call_id = p["call_id"].as_str().unwrap_or("").to_string();
                        // A web search has no output record: it is complete when logged.
                        let done = kind == "web_search_call";
                        let id = if call_id.is_empty() {
                            format!("{}:{}:{}", t.key, kind, t.tools.len())
                        } else {
                            call_id.clone()
                        };
                        if !call_id.is_empty() {
                            open.insert(call_id, (n - 1, t.tools.len()));
                        }
                        t.tools.push(Tool {
                            id,
                            name,
                            input,
                            start: ts.clone(),
                            end: done.then(|| ts.clone()),
                            ..Default::default()
                        });
                        t.end = ts.clone();
                    }
                    "function_call_output" | "custom_tool_call_output" => {
                        let call_id = p["call_id"].as_str().unwrap_or("");
                        if let Some(&(ti, k)) = open.get(call_id)
                            && let Some(t) = turns.get_mut(ti)
                        {
                            let out = output_text(&p["output"]);
                            let tool = &mut t.tools[k];
                            tool.error = failed(&out);
                            tool.output = out;
                            tool.end = Some(ts.clone());
                            t.end = ts.clone();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // A turn with no prompt (e.g. a review or compaction task) is not a user turn.
    turns.retain(|t| !t.input.is_empty());
    Ok(turns)
}

/// Record a prompt: fill the open turn's empty input, or open a new turn when the
/// previous one is closed or already answered (older rollouts have no task events).
#[allow(clippy::too_many_arguments)]
fn set_prompt(
    turns: &mut Vec<Turn>,
    closed: &mut bool,
    ordinal: &mut usize,
    s: &Session,
    ts: &str,
    text: String,
    new_turn: &impl Fn(&mut Vec<Turn>, &Session, String, &str),
) -> bool {
    if text.is_empty() {
        return false;
    }
    let answered = turns
        .last()
        .is_some_and(|t| !t.input.is_empty() && (!t.output.is_empty() || !t.tools.is_empty()));
    if *closed || turns.is_empty() || answered {
        *ordinal += 1;
        new_turn(turns, s, format!("p{ordinal}"), ts);
        *closed = false;
    }
    let t = turns.last_mut().expect("a turn was just ensured");
    if t.input.is_empty() {
        t.input = text;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, lines: &[&str]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, lines.join("\n")).unwrap();
        path
    }

    const META: &str = r#"{"timestamp":"2026-10-25T10:00:00.000Z","type":"session_meta","payload":{"id":"019a-thread","session_id":"019a-thread","timestamp":"2026-10-25T10:00:00Z","cwd":"/work/shippinai","originator":"codex_cli_rs","cli_version":"0.130.0","model_provider":"openai","git":{"branch":"main"}}}"#;

    #[test]
    fn paginated_rollout_with_task_events() {
        let path = write(
            "codex-paginated.jsonl",
            &[
                META,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"turn_context","payload":{"turn_id":"T1","cwd":"/work","model":"gpt-5.3-codex"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"T1"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n<cwd>/work</cwd>\n</environment_context>"}]}}"#,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix the failing test"}]}}"#,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"event_msg","payload":{"type":"item_completed","thread_id":"019a-thread","turn_id":"T1","item":{"type":"UserMessage","id":"u1","content":[{"type":"text","text":"fix the failing test","text_elements":[]}]}}}"#,
                r#"{"timestamp":"2026-10-25T10:00:02Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"cargo\",\"test\"]}","call_id":"c1"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:09Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"Exit code: 101\nWall time: 7.0 seconds\nOutput:\ntest failed"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:10Z","type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch","call_id":"c2"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:11Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c2","output":"{\"output\":\"Success\",\"metadata\":{\"exit_code\":0}}"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:12Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":9000,"output_tokens":300},"last_token_usage":{"input_tokens":5000,"cached_input_tokens":4000,"output_tokens":200}}}}"#,
                r#"{"timestamp":"2026-10-25T10:00:13Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fixed: the assertion used the wrong field."}]}}"#,
                r#"{"timestamp":"2026-10-25T10:00:14Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"T1","last_agent_message":"Fixed: the assertion used the wrong field."}}"#,
                r#"{"timestamp":"2026-10-25T10:01:00Z","type":"some_future_type","payload":{}}"#,
            ],
        );
        let turns = parse_rollout(&path).unwrap();
        assert_eq!(turns.len(), 1);
        let t = &turns[0];
        assert_eq!(t.key, "T1");
        assert_eq!(t.session, "019a-thread");
        assert_eq!(t.input, "fix the failing test");
        assert_eq!(t.model.as_deref(), Some("gpt-5.3-codex"));
        assert_eq!(t.output, vec!["Fixed: the assertion used the wrong field."]);
        assert_eq!((t.in_tokens, t.out_tokens, t.cache_read), (5000, 200, 4000));
        assert_eq!(t.tools.len(), 2);
        assert!(t.tools[0].error, "exit code 101 is a failure");
        assert!(!t.tools[1].error, "metadata.exit_code 0 is a success");
        assert!(t.tools.iter().all(|x| x.end.is_some()));
        assert_eq!((t.agent, t.provider.as_str()), ("codex", "openai"));
        assert_eq!(t.branch, "main");
    }

    #[test]
    fn legacy_rollout_without_task_events_splits_on_new_prompt() {
        let path = write(
            "codex-legacy.jsonl",
            &[
                META,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first ask"}]}}"#,
                r#"{"timestamp":"2026-10-25T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"first ask","kind":"plain"}}"#,
                r#"{"timestamp":"2026-10-25T10:00:02Z","type":"response_item","payload":{"type":"local_shell_call","call_id":"s1","status":"completed","action":{"type":"exec","command":["ls"]}}}"#,
                r#"{"timestamp":"2026-10-25T10:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}}"#,
                r#"{"timestamp":"2026-10-25T10:01:00Z","type":"event_msg","payload":{"type":"user_message","message":"second ask","kind":"plain"}}"#,
                r#"{"timestamp":"2026-10-25T10:01:01Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{}","call_id":"c9"}}"#,
            ],
        );
        let turns = parse_rollout(&path).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].input, "first ask");
        assert_eq!(turns[1].input, "second ask");
        assert!(
            turns[0].tools[0].end.is_none(),
            "no output record yet: held back by build"
        );
        assert!(turns[1].tools[0].end.is_none());
    }

    /// Structure observed in real Codex 0.164 subagent rollouts (content replaced): the child's
    /// own meta, then the parent's meta and inherited history, then the child's own turns,
    /// whose prompts arrive as `agent_message` from the parent.
    #[test]
    fn forked_subagent_skips_inherited_parent_history() {
        let path = write(
            "codex-fork.jsonl",
            &[
                r#"{"timestamp":"2026-10-25T23:46:46.563Z","type":"session_meta","payload":{"id":"01a17575-a412-7fe1-b5bc-2aa506a6aeec","session_id":"01a17157-64d6-7751-9e49-9ce3cf718b8d","forked_from_id":"01a17157-64d6-7751-9e49-9ce3cf718b8d","agent_path":"/root/review","cwd":"/w","originator":"codex_vscode","cli_version":"0.164.0","model_provider":"openai"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.563Z","type":"session_meta","payload":{"id":"01a17157-64d6-7751-9e49-9ce3cf718b8d","session_id":"01a17157-64d6-7751-9e49-9ce3cf718b8d","cwd":"/w","originator":"codex_vscode","cli_version":"0.164.0"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.600Z","type":"event_msg","payload":{"type":"task_started","turn_id":"01a17157-66d6-7541-9902-ded33c93a291"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.600Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"parent prompt"}]}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.600Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"01a17157-66d6-7541-9902-ded33c93a291"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.698Z","type":"event_msg","payload":{"type":"task_started","turn_id":"01a17575-1868-73e3-a324-572af06b6619"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.698Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"yes"}]}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.699Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"01a17575-1868-73e3-a324-572af06b6619"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.699Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"You are now a newly spawned child agent."}]}}"#,
                r#"{"timestamp":"2026-10-25T23:46:46.862Z","type":"event_msg","payload":{"type":"task_started","turn_id":"01a17575-a5b0-7123-964a-df7c48c97d76"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:49.254Z","type":"response_item","payload":{"type":"agent_message","author":"/root","recipient":"/root/review","content":[{"type":"input_text","text":"Message Type: NEW_TASK\nTask name: /root/review"}]}}"#,
                r#"{"timestamp":"2026-10-25T23:46:52.507Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"…","call_id":"c1"}}"#,
                r#"{"timestamp":"2026-10-25T23:46:56.735Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c1","output":[{"type":"input_text","text":"Script completed\nWall time 4.6 seconds\nOutput:\n"},{"type":"input_text","text":"{\"chunk_id\":\"a\",\"exit_code\":1,\"output\":\"no match\"}"}]}}"#,
                r#"{"timestamp":"2026-10-25T23:47:38.277Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"01a17575-a5b0-7123-964a-df7c48c97d76"}}"#,
            ],
        );
        let turns = parse_rollout(&path).unwrap();
        assert_eq!(turns.len(), 1, "only the child's own turn");
        let t = &turns[0];
        assert_eq!(t.session, "01a17575-a412-7fe1-b5bc-2aa506a6aeec");
        assert_eq!(t.key, "01a17575-a5b0-7123-964a-df7c48c97d76");
        assert_eq!(t.prompt_author.as_deref(), Some("/root"));
        assert_eq!(t.subagent.as_deref(), Some("/root/review"));
        assert!(t.tools[0].error, "embedded exit_code 1");
    }

    #[test]
    fn compressed_rollouts_are_read() {
        let plain = [
            META,
            r#"{"timestamp":"2026-10-25T10:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"compressed ask"}}"#,
        ]
        .join("\n");
        let zst = ruzstd::encoding::compress_to_vec(
            plain.as_bytes(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        let path = std::env::temp_dir().join("codex-compressed.jsonl.zst");
        std::fs::write(&path, zst).unwrap();
        let turns = parse_rollout(&path).unwrap();
        assert_eq!(turns[0].input, "compressed ask");
    }

    #[test]
    fn exit_code_forms() {
        assert!(failed("Exit code: 1\nWall time: 0.1 seconds"));
        assert!(!failed("Exit code: 0\nWall time: 0.1 seconds"));
        assert!(failed(r#"{"output":"x","metadata":{"exit_code":2}}"#));
        assert!(!failed("plain output"));
        assert!(!failed(
            r#"Script completed\n{"chunk_id":"a","exit_code":0}"#
        ));
        assert!(failed(
            r#"Script completed\n{"chunk_id":"a","exit_code":0}\n{"exit_code":2}"#
        ));
    }
}
