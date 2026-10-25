//! rroflow-raindrop: send real agent runs to Raindrop.
//!
//! `ingest-claude` turns Claude Code session transcripts (JSONL) into Raindrop
//! events (one per user turn) plus OTLP spans (one LLM span per turn, one tool
//! span per tool call). Event and span ids are derived from transcript ids, so
//! re-ingesting a transcript is idempotent on Raindrop's side.
//!
//! Configuration is environment-first so every project deploys it the same way:
//!   RAINDROP_WRITE_KEY   write key (required unless --dry-run)
//!   RAINDROP_PROJECT     Raindrop project slug (default: "default")
//!   RAINDROP_USER_ID     user id stamped on events (default: "operator")
//!   RAINDROP_API_URL     base URL (default: https://api.raindrop.ai/v1)
//!   RROFLOW_REDACT       extra comma-separated literal values to redact

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use regex::Regex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

const NS: Uuid = Uuid::from_u128(0x7f1c_2d3e_4a5b_46c7_8d9e_0f1a_2b3c_4d5e);

#[derive(Parser)]
#[command(version, about = "Send real agent runs to Raindrop")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest Claude Code transcripts (.jsonl files or directories of them).
    IngestClaude {
        paths: Vec<PathBuf>,
        #[arg(long, env = "RAINDROP_PROJECT", default_value = "default")]
        project: String,
        #[arg(long, env = "RAINDROP_USER_ID", default_value = "operator")]
        user: String,
        /// Free-form tag stored as properties.workspace (e.g. shippinai, rrflow, clyffy).
        #[arg(long, env = "RROFLOW_WORKSPACE")]
        workspace: Option<String>,
        /// Include (redacted, truncated) tool inputs and outputs on spans.
        #[arg(long)]
        payloads: bool,
        /// Parse and print a summary; send nothing.
        #[arg(long)]
        dry_run: bool,
        /// Write the exact payloads that would be sent to this JSONL file.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Only turns at or after this RFC 3339 timestamp.
        #[arg(long)]
        since: Option<String>,
    },
}

struct Redactor {
    patterns: Vec<Regex>,
    literals: Vec<String>,
}

impl Redactor {
    fn new() -> Self {
        let pats = [
            r"(?i)bearer\s+[A-Za-z0-9._~+/=-]{8,}",
            r"sk-[A-Za-z0-9_-]{16,}",
            r"gh[pousr]_[A-Za-z0-9]{20,}",
            r"xox[abprs]-[A-Za-z0-9-]{10,}",
            r"AKIA[0-9A-Z]{16}",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            r"\b[a-f0-9]{48,}\b",
        ];
        let mut literals: Vec<String> = [
            "RAINDROP_WRITE_KEY",
            "RAINDROP_QUERY_KEY",
            "RAINDROP_QUERY_API_KEY",
        ]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .collect();
        if let Ok(extra) = std::env::var("RROFLOW_REDACT") {
            literals.extend(
                extra
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            );
        }
        literals.retain(|s| s.len() >= 8);
        Self {
            patterns: pats
                .iter()
                .map(|p| Regex::new(p).expect("valid regex"))
                .collect(),
            literals,
        }
    }
    fn apply(&self, s: &str) -> String {
        let mut out = s.to_string();
        for l in &self.literals {
            out = out.replace(l.as_str(), "[REDACTED]");
        }
        for p in &self.patterns {
            out = p.replace_all(&out, "[REDACTED]").into_owned();
        }
        out
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}...[truncated by rroflow-raindrop]")
}

fn strip_reminders(s: &str) -> String {
    let re = Regex::new(r"(?s)<system-reminder>.*?</system-reminder>").unwrap();
    re.replace_all(s, "").trim().to_string()
}

fn ts_nanos(ts: &str) -> Option<i128> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.timestamp_nanos_opt().unwrap_or(0) as i128)
}

#[derive(Default)]
struct Tool {
    id: String,
    name: String,
    input: String,
    start: String,
    end: Option<String>,
    output: String,
    error: bool,
}

#[derive(Default)]
struct Turn {
    key: String,
    session: String,
    start: String,
    end: String,
    input: String,
    output: Vec<String>,
    model: Option<String>,
    in_tokens: u64,
    out_tokens: u64,
    tools: Vec<Tool>,
    cwd: String,
    branch: String,
    version: String,
    entrypoint: String,
    /// Set when the turn resumes after a context-compaction summary.
    compacted: bool,
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

fn parse_transcript(path: &PathBuf) -> Result<Vec<Turn>> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut turns: Vec<Turn> = Vec::new();
    let mut open: HashMap<String, (usize, usize)> = HashMap::new();
    // promptId -> prompt text, so a compaction summary can point back at the prompt it continues.
    let mut prompts: HashMap<String, String> = HashMap::new();
    for line in raw.lines() {
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
                    let harness = [
                        "<task-notification>",
                        "<local-command-",
                        "<command-name>",
                        "<command-message>",
                    ]
                    .iter()
                    .any(|p| text.starts_with(p));
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
                            .unwrap_or_else(|| "[continued after context compaction]".into())
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

fn hex_id(seed: &str, bytes: usize) -> String {
    let u = Uuid::new_v5(&NS, seed.as_bytes());
    u.simple().to_string()[..bytes * 2].to_string()
}

fn attr(k: &str, v: Value) -> Value {
    let val = match v {
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({"intValue": n.to_string()}),
        Value::Number(n) => json!({"doubleValue": n}),
        Value::Bool(b) => json!({"boolValue": b}),
        other => {
            json!({"stringValue": other.as_str().map(String::from).unwrap_or_else(|| other.to_string())})
        }
    };
    json!({"key": k, "value": val})
}

struct Built {
    event: Value,
    spans: Vec<Value>,
}

fn build(t: &Turn, user: &str, workspace: Option<&str>, payloads: bool, r: &Redactor) -> Built {
    let event_id = Uuid::new_v5(&NS, format!("{}:{}", t.session, t.key).as_bytes()).to_string();
    let trace_id = hex_id(&format!("trace:{event_id}"), 16);
    let root = hex_id(&format!("root:{event_id}"), 8);
    let errors = t.tools.iter().filter(|x| x.error).count();
    let mut tool_names: Vec<&str> = t.tools.iter().map(|x| x.name.as_str()).collect();
    tool_names.sort();
    tool_names.dedup();
    let output = r.apply(&truncate(&t.output.join("\n\n"), 20_000));
    let mut props = json!({
        "source": "claude-code",
        "session_id": t.session,
        "cwd": t.cwd,
        "git_branch": t.branch,
        "claude_code_version": t.version,
        "entrypoint": t.entrypoint,
        "compacted": t.compacted,
        "tool_calls": t.tools.len(),
        "tool_errors": errors,
        "tool_names": tool_names.join(","),
        "ingested_by": concat!("rroflow-raindrop/", env!("CARGO_PKG_VERSION")),
    });
    if let Some(w) = workspace {
        props["workspace"] = json!(w);
    }
    let mut ai = json!({"input": r.apply(&truncate(&t.input, 20_000)), "convo_id": t.session});
    if !output.is_empty() {
        ai["output"] = json!(output);
    }
    if let Some(m) = &t.model {
        ai["model"] = json!(m);
    }
    let event = json!({"event_id": event_id, "user_id": user, "event": "claude_code.turn", "timestamp": t.start, "ai_data": ai, "properties": props});

    let start = ts_nanos(&t.start).unwrap_or(0);
    let end = ts_nanos(&t.end).unwrap_or(start).max(start);
    let assoc = |k: &str, v: &str| attr(&format!("traceloop.association.properties.{k}"), json!(v));
    let model = t.model.clone().unwrap_or_else(|| "unknown".into());
    let mut spans = vec![json!({
        "traceId": trace_id, "spanId": root, "name": "claude_code.turn", "kind": 1,
        "startTimeUnixNano": start.to_string(), "endTimeUnixNano": end.to_string(),
        "status": {"code": if errors > 0 { 2 } else { 1 }},
        "attributes": [
            assoc("event_id", &event_id), assoc("user_id", user), assoc("convo_id", &t.session),
            attr("ai.operationId", json!("ai.generateText")), attr("traceloop.span.kind", json!("llm")),
            attr("gen_ai.system", json!("anthropic")), attr("gen_ai.request.model", json!(model)),
            attr("gen_ai.response.model", json!(model)),
            attr("gen_ai.usage.input_tokens", json!(t.in_tokens)), attr("gen_ai.usage.output_tokens", json!(t.out_tokens)),
        ]
    })];
    for tool in &t.tools {
        let s = ts_nanos(&tool.start).unwrap_or(start);
        let e = tool.end.as_deref().and_then(ts_nanos).unwrap_or(s).max(s);
        let mut attrs = vec![
            assoc("event_id", &event_id),
            attr("ai.operationId", json!("ai.toolCall")),
            attr("traceloop.span.kind", json!("tool")),
            attr("traceloop.entity.name", json!(tool.name)),
            attr(
                "traceloop.entity.duration_ms",
                json!(((e - s) / 1_000_000) as i64),
            ),
        ];
        if payloads {
            attrs.push(attr(
                "traceloop.entity.input",
                json!(r.apply(&truncate(&tool.input, 2_000))),
            ));
            attrs.push(attr(
                "traceloop.entity.output",
                json!(r.apply(&truncate(&tool.output, 2_000))),
            ));
        }
        let status = if tool.end.is_none() {
            json!({"code": 0})
        } else if tool.error {
            json!({"code": 2, "message": r.apply(&truncate(&tool.output, 300))})
        } else {
            json!({"code": 1})
        };
        spans.push(json!({
            "traceId": trace_id, "spanId": hex_id(&format!("tool:{}", tool.id), 8), "parentSpanId": root,
            "name": tool.name, "kind": 1, "startTimeUnixNano": s.to_string(), "endTimeUnixNano": e.to_string(),
            "status": status, "attributes": attrs
        }));
    }
    Built { event, spans }
}

fn post(base: &str, path: &str, key: &str, project: &str, body: &Value) -> Result<u16> {
    let mut last = String::new();
    for attempt in 0..3 {
        match ureq::post(&format!("{base}/{path}"))
            .header("Authorization", &format!("Bearer {key}"))
            .header("X-Raindrop-Project-Id", project)
            .send_json(body)
        {
            Ok(resp) => return Ok(resp.status().as_u16()),
            Err(ureq::Error::StatusCode(code)) if code != 429 && code < 500 => {
                bail!("{path} rejected with HTTP {code}")
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
    }
    bail!("{path} failed after 3 attempts: {last}")
}

fn collect(paths: &[PathBuf]) -> Vec<PathBuf> {
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::IngestClaude {
            paths,
            project,
            user,
            workspace,
            payloads,
            dry_run,
            dump,
            since,
        } => {
            let key = std::env::var("RAINDROP_WRITE_KEY").unwrap_or_default();
            if key.is_empty() && !dry_run {
                bail!("RAINDROP_WRITE_KEY is not set (use --dry-run to only parse)");
            }
            let base = std::env::var("RAINDROP_API_URL")
                .unwrap_or_else(|_| "https://api.raindrop.ai/v1".into());
            let since_ns = since.as_deref().and_then(ts_nanos);
            let red = Redactor::new();
            let (mut sent, mut spans_sent) = (0usize, 0usize);
            for file in collect(&paths) {
                let turns = parse_transcript(&file)?;
                let built: Vec<Built> = turns
                    .iter()
                    .filter(|t| since_ns.is_none_or(|s| ts_nanos(&t.start).unwrap_or(0) >= s))
                    .map(|t| build(t, &user, workspace.as_deref(), payloads, &red))
                    .collect();
                let nspans: usize = built.iter().map(|b| b.spans.len()).sum();
                println!(
                    "{}: {} turns, {} spans",
                    file.display(),
                    built.len(),
                    nspans
                );
                if let Some(d) = &dump {
                    use std::io::Write;
                    let mut f = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(d)?;
                    for b in &built {
                        writeln!(f, "{}", json!({"event": b.event, "spans": b.spans}))?;
                    }
                }
                if dry_run {
                    continue;
                }
                for chunk in built.chunks(20) {
                    let events: Vec<Value> = chunk.iter().map(|b| b.event.clone()).collect();
                    post(&base, "events/track", &key, &project, &Value::Array(events))?;
                    let spans: Vec<Value> = chunk.iter().flat_map(|b| b.spans.clone()).collect();
                    for sc in spans.chunks(200) {
                        let body = json!({"resourceSpans": [{
                            "resource": {"attributes": [attr("service.name", json!("claude-code"))]},
                            "scopeSpans": [{"scope": {"name": "rroflow-raindrop", "version": env!("CARGO_PKG_VERSION")}, "spans": sc}]
                        }]});
                        post(&base, "traces", &key, &project, &body)?;
                    }
                    sent += chunk.len();
                    spans_sent += spans.len();
                }
            }
            println!("sent {sent} events, {spans_sent} spans to project {project}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_known_secret_shapes() {
        let r = Redactor {
            patterns: Redactor::new().patterns,
            literals: vec!["25b1106c-d759-49c9".into()],
        };
        let s = r.apply(
            "Authorization: Bearer abcdefghijklmnop key=25b1106c-d759-49c9 sk-ABCDEFGHIJKLMNOPQRST",
        );
        assert!(
            !s.contains("abcdefghijklmnop") && !s.contains("25b1106c") && !s.contains("sk-ABC"),
            "{s}"
        );
    }
    #[test]
    fn event_ids_are_deterministic() {
        let t = Turn {
            session: "s".into(),
            key: "k".into(),
            start: "2026-10-24T00:00:00Z".into(),
            end: "2026-10-24T00:00:01Z".into(),
            input: "hi".into(),
            ..Default::default()
        };
        let r = Redactor::new();
        assert_eq!(
            build(&t, "u", None, false, &r).event["event_id"],
            build(&t, "u", None, false, &r).event["event_id"]
        );
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
}
