//! rroflow-raindrop: send real agent runs to Raindrop.
//!
//! `ingest-claude` (Claude Code) and `ingest-codex` (Codex CLI) turn coding-agent
//! session transcripts into Raindrop events (one per user turn) plus OTLP spans
//! (one LLM span per turn, one tool span per finished tool call). Event and span ids are derived from transcript ids, so
//! re-ingesting a transcript is idempotent on Raindrop's side.
//!
//! Configuration is environment-first so every project deploys it the same way:
//!   RAINDROP_WRITE_KEY   write key (required unless --dry-run)
//!   RAINDROP_PROJECT     Raindrop project slug (default: "default")
//!   RAINDROP_USER_ID     user id stamped on events (default: "operator")
//!   RAINDROP_API_URL     base URL (default: https://api.raindrop.ai/v1)
//!   RROFLOW_REDACT       extra comma-separated literal values to redact
//!
//! `doctor` reads one event back through the Query API and checks what Raindrop stored:
//!   RAINDROP_QUERY_KEY   Query API key (RAINDROP_QUERY_API_KEY is also accepted)
//!   RAINDROP_QUERY_URL   base URL (default: https://query.raindrop.ai/v1)

mod api;
mod codex;
mod doctor;
mod otlp;
mod redact;
mod transcript;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use otlp::{Built, build, trace_body};
use redact::Redactor;
use serde_json::{Value, json};
use std::path::PathBuf;
use transcript::{collect, parse_transcript, ts_nanos};

#[derive(Parser)]
#[command(version, about = "Send real agent runs to Raindrop")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args)]
struct IngestArgs {
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
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest Claude Code transcripts (~/.claude/projects, .jsonl files or directories).
    IngestClaude(IngestArgs),
    /// Ingest Codex CLI rollouts (~/.codex/sessions, .jsonl or .jsonl.zst, files or directories).
    IngestCodex(IngestArgs),
    /// Read back the newest event (or --event) and check what Raindrop stored.
    ///
    /// Exit code: 0 nothing failed, 1 a check failed, 2 a secret reached Raindrop.
    Doctor {
        #[arg(long, env = "RAINDROP_PROJECT", default_value = "default")]
        project: String,
        /// Event id to check; defaults to the project's newest event.
        #[arg(long)]
        event: Option<String>,
        /// Print the checks as JSON instead of a report.
        #[arg(long)]
        json: bool,
        /// Seconds after receipt during which missing spans count as still ingesting.
        #[arg(long, default_value_t = 300)]
        ingest_lag_secs: i64,
        /// Hours after which the newest event counts as stale.
        #[arg(long, default_value_t = 24)]
        stale_hours: i64,
        /// Share of finished tool calls that must carry readable payloads.
        #[arg(long, default_value_t = 0.95)]
        payload_pass_ratio: f64,
        /// How many recent events to scan for the newest AI interaction.
        #[arg(long, default_value_t = 50)]
        scan_events: usize,
    },
}

/// Errors print as one plain line (no backtrace): this tool is run by people, not CI only.
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Parse every transcript under `args.paths` with `parse`, then send (or dump) the turns.
fn ingest(
    args: IngestArgs,
    service: &str,
    parse: fn(&std::path::Path) -> Result<Vec<transcript::Turn>>,
) -> Result<()> {
    let IngestArgs {
        paths,
        project,
        user,
        workspace,
        payloads,
        dry_run,
        dump,
        since,
    } = args;
    let key = std::env::var("RAINDROP_WRITE_KEY").unwrap_or_default();
    if key.is_empty() && !dry_run {
        bail!("RAINDROP_WRITE_KEY is not set (use --dry-run to only parse)");
    }
    let base =
        std::env::var("RAINDROP_API_URL").unwrap_or_else(|_| "https://api.raindrop.ai/v1".into());
    let since_ns = since.as_deref().and_then(ts_nanos);
    let red = Redactor::new();
    let (mut sent, mut spans_sent) = (0usize, 0usize);
    // One run, one dump: start the file empty, then append per transcript.
    if let Some(d) = &dump {
        std::fs::File::create(d)?;
    }
    for file in collect(&paths) {
        let turns = parse(&file)?;
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
            api::post(&base, "events/track", &key, &project, &Value::Array(events))?;
            let spans: Vec<Value> = chunk.iter().flat_map(|b| b.spans.clone()).collect();
            for sc in spans.chunks(200) {
                api::post(&base, "traces", &key, &project, &trace_body(service, sc))?;
            }
            sent += chunk.len();
            spans_sent += spans.len();
        }
    }
    println!("sent {sent} events, {spans_sent} spans to project {project}");
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::IngestClaude(args) => ingest(args, "claude-code", parse_transcript)?,
        Cmd::IngestCodex(args) => ingest(args, "codex", codex::parse_rollout)?,
        Cmd::Doctor {
            project,
            event,
            json,
            ingest_lag_secs,
            stale_hours,
            payload_pass_ratio,
            scan_events,
        } => {
            let key = ["RAINDROP_QUERY_KEY", "RAINDROP_QUERY_API_KEY"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
            let Some(key) = key else {
                bail!(
                    "RAINDROP_QUERY_KEY is not set: doctor reads through Raindrop's Query API (Settings → API keys)"
                );
            };
            let base = std::env::var("RAINDROP_QUERY_URL")
                .unwrap_or_else(|_| "https://query.raindrop.ai/v1".into());
            // Default: the newest AI interaction among recent events. Plain telemetry
            // (no prompt, response or tool calls) is valid Raindrop data but has nothing to check.
            let id = match event {
                Some(id) => id,
                None => {
                    let list = api::get(
                        &base,
                        &format!("events?limit={scan_events}"),
                        &key,
                        &project,
                    )?;
                    let rows = list["data"].as_array().cloned().unwrap_or_default();
                    if rows.is_empty() {
                        bail!(
                            "project {project} has no events yet. Send one first, e.g. `rroflow-raindrop ingest-claude ~/.claude/projects --project {project} --payloads`"
                        );
                    }
                    match rows.iter().find(|r| doctor::is_ai_interaction(r)) {
                        Some(r) => r["id"].as_str().unwrap_or_default().to_string(),
                        None => bail!(
                            "the newest {} events in project {project} are plain telemetry (e.g. `{}`), with no AI interaction to check. Pass --event <id> to check one anyway",
                            rows.len(),
                            rows[0]["event_name"].as_str().unwrap_or("event")
                        ),
                    }
                }
            };
            let ev = api::get(&base, &format!("events/{id}"), &key, &project)
                .with_context(|| format!("event {id} in project {project} (the project is selected by --project / RAINDROP_PROJECT)"))?["data"]
                .take();
            let spans = api::get(
                &base,
                &format!("traces?event_id={id}&limit={}", doctor::TRACE_PAGE_MAX),
                &key,
                &project,
            )?["data"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let lim = doctor::Limits {
                ingest_lag_secs,
                stale_hours,
                payload_pass_ratio,
            };
            let now = chrono::Utc::now();
            let checks = doctor::run(&ev, &spans, now, &lim, &Redactor::new());
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "project": project, "event_id": id, "checks": checks,
                        "repair_prompt": doctor::repair_prompt(&checks),
                    }))?
                );
            } else {
                let age =
                    chrono::DateTime::parse_from_rfc3339(ev["timestamp"].as_str().unwrap_or(""))
                        .map(|t| {
                            format!(
                                ", {} ago",
                                doctor::human_age(now - t.with_timezone(&chrono::Utc))
                            )
                        })
                        .unwrap_or_default();
                let header = format!(
                    "rroflow-raindrop doctor · project {project} · event {} ({}{age})",
                    &id[..id.len().min(8)],
                    ev["event_name"].as_str().unwrap_or("event")
                );
                print!("{}", doctor::render(&header, &checks));
            }
            std::process::exit(doctor::exit_code(&checks));
        }
    }
    Ok(())
}
