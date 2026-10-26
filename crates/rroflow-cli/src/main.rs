//! rroflow: the rroflow.ai collector (the first SDK).
//!
//! Reads the session files your coding agents already write, Claude Code
//! (`~/.claude/projects`) and Codex (`~/.codex/sessions`), turns each prompt into one
//! `rroflow.turn/v1` record, evaluates behaviour signals on this machine, and sends:
//!
//! - `--sink rroflow` (default): records to your devspace's rroflow.ai ingest. Content
//!   (prompt, response and tool text) is sent only with `--content`; signal results and
//!   metrics are always sent.
//!   `RROFLOW_INGEST_URL` (your devspace ingest), `RROFLOW_INGEST_KEY` (decides which
//!   devspace receives the data), `RROFLOW_PROJECT` (project within the devspace).
//! - `--sink raindrop`: Raindrop events plus OTLP spans.
//!   `RAINDROP_WRITE_KEY`, `RAINDROP_PROJECT`, `RAINDROP_USER_ID`, `RAINDROP_API_URL`.
//!
//! `rroflow signals` reports the signals locally and sends nothing. `rroflow doctor` checks
//! what Raindrop stored (RAINDROP_QUERY_KEY, RAINDROP_QUERY_URL).
//!
//! Ids are derived from transcript ids, so re-sending replaces and never duplicates.
//! RROFLOW_REDACT adds literal secrets to the always-on redaction.

mod api;
mod codex;
mod doctor;
mod otlp;
mod record;
mod redact;
mod transcript;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use otlp::{Built, build, trace_body};
use redact::Redactor;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use transcript::{collect, parse_transcript, ts_nanos};

#[derive(Parser)]
#[command(
    name = "rroflow",
    version,
    about = "rroflow.ai collector: Sentry for your AI agents (Claude Code, Codex)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Sink {
    /// rroflow.ai ingest for your devspace.
    Rroflow,
    /// Raindrop (events + OTLP traces).
    Raindrop,
}

#[derive(clap::Args)]
struct IngestArgs {
    paths: Vec<PathBuf>,
    /// Where turns go.
    #[arg(long, value_enum, env = "RROFLOW_SINK", default_value = "rroflow")]
    sink: Sink,
    /// Project within your devspace (rroflow sink) or Raindrop project slug (raindrop sink;
    /// falls back to RAINDROP_PROJECT, then "default").
    #[arg(long, env = "RROFLOW_PROJECT")]
    project: Option<String>,
    /// rroflow sink: also send prompt, response and tool text (redacted). Off by default.
    #[arg(long)]
    content: bool,
    /// Raindrop sink: user id stamped on events.
    #[arg(long, env = "RAINDROP_USER_ID", default_value = "operator")]
    user: String,
    /// Raindrop sink: free-form tag stored as properties.workspace.
    #[arg(long, env = "RROFLOW_WORKSPACE")]
    workspace: Option<String>,
    /// Raindrop sink: include (redacted, truncated) tool inputs and outputs on spans.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Agent {
    Claude,
    Codex,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest Claude Code transcripts (~/.claude/projects, .jsonl files or directories).
    IngestClaude(IngestArgs),
    /// Ingest Codex CLI rollouts (~/.codex/sessions, .jsonl or .jsonl.zst, files or directories).
    IngestCodex(IngestArgs),
    /// Report behaviour signals found in local sessions. Sends nothing.
    Signals {
        /// Which agent wrote the sessions.
        #[arg(long, value_enum)]
        agent: Agent,
        paths: Vec<PathBuf>,
        /// Print every hit with its evidence, not only the totals.
        #[arg(long)]
        details: bool,
        /// Only turns at or after this RFC 3339 timestamp.
        #[arg(long)]
        since: Option<String>,
    },
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

type Parse = fn(&std::path::Path) -> Result<Vec<transcript::Turn>>;

fn since_filter(since: Option<&str>) -> impl Fn(&transcript::Turn) -> bool {
    let since_ns = since.and_then(ts_nanos);
    move |t: &transcript::Turn| since_ns.is_none_or(|s| ts_nanos(&t.start).unwrap_or(0) >= s)
}

fn open_dump(dump: &Option<PathBuf>) -> Result<Option<std::fs::File>> {
    // One run, one dump: start the file empty.
    dump.as_ref()
        .map(|d| std::fs::File::create(d).with_context(|| format!("create {}", d.display())))
        .transpose()
}

/// Parse every transcript under `args.paths`, then send (or dump) the turns to the sink.
fn ingest(args: IngestArgs, service: &str, parse: Parse) -> Result<()> {
    match args.sink {
        Sink::Rroflow => ingest_rroflow(args, parse),
        Sink::Raindrop => ingest_raindrop(args, service, parse),
    }
}

/// Most records per POST to the rroflow ingest.
const RROFLOW_BATCH: usize = 100;

fn ingest_rroflow(args: IngestArgs, parse: Parse) -> Result<()> {
    use std::io::Write;
    let base = std::env::var("RROFLOW_INGEST_URL").unwrap_or_default();
    let key = std::env::var("RROFLOW_INGEST_KEY").unwrap_or_default();
    if !args.dry_run && (base.is_empty() || key.is_empty()) {
        bail!(
            "RROFLOW_INGEST_URL and RROFLOW_INGEST_KEY must be set: they come from your devspace on rroflow.ai (use --dry-run to only parse, or --sink raindrop)"
        );
    }
    let keep = since_filter(args.since.as_deref());
    let red = Redactor::new();
    let mut dump = open_dump(&args.dump)?;
    let (mut sent, mut hits) = (0usize, 0usize);
    for file in collect(&args.paths) {
        let turns: Vec<_> = parse(&file)?.into_iter().filter(|t| keep(t)).collect();
        let recs = record::records(&turns, args.project.as_deref(), args.content, &red);
        let n_hits: usize = recs.iter().map(|r| r.signals.len()).sum();
        hits += n_hits;
        println!(
            "{}: {} turns, {} signal hits",
            file.display(),
            recs.len(),
            n_hits
        );
        if let Some(f) = dump.as_mut() {
            for r in &recs {
                writeln!(f, "{}", serde_json::to_string(r)?)?;
            }
        }
        if args.dry_run {
            continue;
        }
        for chunk in recs.chunks(RROFLOW_BATCH) {
            api::post_rroflow(&base, &key, &json!({ "turns": chunk }))?;
            sent += chunk.len();
        }
    }
    println!(
        "sent {sent} turns ({hits} signal hits) to rroflow{}",
        if args.content {
            " with content"
        } else {
            ", content kept local"
        }
    );
    Ok(())
}

fn ingest_raindrop(args: IngestArgs, service: &str, parse: Parse) -> Result<()> {
    use std::io::Write;
    let project = args
        .project
        .clone()
        .or_else(|| std::env::var("RAINDROP_PROJECT").ok())
        .unwrap_or_else(|| "default".into());
    let key = std::env::var("RAINDROP_WRITE_KEY").unwrap_or_default();
    if key.is_empty() && !args.dry_run {
        bail!("RAINDROP_WRITE_KEY is not set (use --dry-run to only parse)");
    }
    let base =
        std::env::var("RAINDROP_API_URL").unwrap_or_else(|_| "https://api.raindrop.ai/v1".into());
    let keep = since_filter(args.since.as_deref());
    let red = Redactor::new();
    let (mut sent, mut spans_sent) = (0usize, 0usize);
    let mut dump = open_dump(&args.dump)?;
    for file in collect(&args.paths) {
        let turns = parse(&file)?;
        let built: Vec<Built> = turns
            .iter()
            .filter(|t| keep(t))
            .map(|t| {
                build(
                    t,
                    &args.user,
                    args.workspace.as_deref(),
                    args.payloads,
                    &red,
                )
            })
            .collect();
        let nspans: usize = built.iter().map(|b| b.spans.len()).sum();
        println!(
            "{}: {} turns, {} spans",
            file.display(),
            built.len(),
            nspans
        );
        if let Some(f) = dump.as_mut() {
            for b in &built {
                writeln!(f, "{}", json!({"event": b.event, "spans": b.spans}))?;
            }
        }
        if args.dry_run {
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
    println!("sent {sent} events, {spans_sent} spans to Raindrop project {project}");
    Ok(())
}

/// `rroflow signals`: evaluate locally and print totals (and evidence with --details).
fn report_signals(
    agent: Agent,
    paths: &[PathBuf],
    details: bool,
    since: Option<&str>,
) -> Result<()> {
    let parse: Parse = match agent {
        Agent::Claude => parse_transcript,
        Agent::Codex => codex::parse_rollout,
    };
    let keep = since_filter(since);
    let red = Redactor::new();
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut turns_seen = 0usize;
    for file in collect(paths) {
        let turns: Vec<_> = parse(&file)?.into_iter().filter(|t| keep(t)).collect();
        turns_seen += turns.len();
        for r in record::records(&turns, None, true, &red) {
            for h in &r.signals {
                *totals.entry(h.signal.clone()).or_default() += 1;
                if details {
                    let prompt = r
                        .content
                        .as_ref()
                        .map(|c| {
                            c.input
                                .split_whitespace()
                                .take(12)
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_default();
                    println!(
                        "{} {} [{:?}] {}  (turn {} · \"{}…\")",
                        r.started_at.get(..16).unwrap_or(&r.started_at),
                        h.signal,
                        h.severity,
                        h.summary,
                        r.turn_key.get(..13).unwrap_or(&r.turn_key),
                        prompt
                    );
                    for e in &h.evidence {
                        if let Some(x) = &e.excerpt {
                            println!("      evidence ({:?}): {x}", e.kind);
                        }
                    }
                }
            }
        }
    }
    println!("\n{turns_seen} turns checked");
    for (id, version, meaning) in rroflow_core::signals::CATALOGUE {
        println!(
            "  {:>4}  {id} (v{version}): {meaning}",
            totals.get(*id).copied().unwrap_or(0)
        );
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::IngestClaude(args) => ingest(args, "claude-code", parse_transcript)?,
        Cmd::IngestCodex(args) => ingest(args, "codex", codex::parse_rollout)?,
        Cmd::Signals {
            agent,
            paths,
            details,
            since,
        } => report_signals(agent, &paths, details, since.as_deref())?,
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
                            "project {project} has no events yet. Send one first, e.g. `rroflow ingest-claude ~/.claude/projects --sink raindrop --project {project} --payloads`"
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
                    "rroflow doctor · Raindrop project {project} · event {} ({}{age})",
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
