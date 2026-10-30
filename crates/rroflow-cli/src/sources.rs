//! Source-resident session collection. Persist revisions before attempting auth or
//! upload. A source is bound once; changing the signed-in account cannot redirect it.
use crate::{account, capture, codex, record, redact::Redactor, retry, transcript};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use rroflow_core::source_health::{SCHEMA as HEALTH_SCHEMA, SourceReport};
use rroflow_core::{TurnRecord, signals::SignalConfig};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const BATCH_LIMIT: usize = 4 * 1024 * 1024;
pub(crate) mod service;

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(hide = true, name = "managed-run")]
    Worker {
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Manage automatic startup for this user's linked session collector.
    Service {
        #[arg(long)]
        store: Option<PathBuf>,
        #[command(subcommand)]
        command: service::Action,
    },
    /// Bind session roots to the signed-in devspace. Existing bindings are immutable.
    Link {
        /// Local journal directory, separate from the SDK event spool.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Claude session root; defaults to CLAUDE_CONFIG_DIR/projects or ~/.claude/projects.
        #[arg(long)]
        claude_root: Option<PathBuf>,
        /// Codex session root; defaults to CODEX_HOME/sessions or ~/.codex/sessions.
        #[arg(long)]
        codex_root: Option<PathBuf>,
        /// Store and upload scrubbed prompt, response and tool text. Default is metrics only.
        #[arg(long)]
        content: bool,
    },
    /// Watch all linked roots and retry queued uploads. Keep this process running.
    Run {
        #[arg(long)]
        store: Option<PathBuf>,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u64).range(1..=60))]
        interval_secs: u64,
        /// One scan/replay pass; reports errors with a nonzero exit code.
        #[arg(long)]
        once: bool,
        /// Collect to disk without reading credentials or making network requests.
        #[arg(long)]
        offline: bool,
        /// Use this saved account file only, ignoring temporary ingest-key/URL overrides.
        #[arg(long)]
        account_file: Option<PathBuf>,
        /// Emit occasional counts instead of local paths and detailed scan errors.
        #[arg(long)]
        quiet: bool,
    },
    /// Read delivery counts and linked roots from disk. Does not claim the process is alive.
    Status {
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Retry up to sixteen queued batches without rescanning session files.
    Flush {
        #[arg(long)]
        store: Option<PathBuf>,
    },
}

#[derive(Serialize, Deserialize)]
struct Binding {
    schema: String,
    source_id: String,
    endpoint: String,
    devspace: String,
    claude_root: PathBuf,
    codex_root: PathBuf,
    content: bool,
    signals: SignalConfig,
}

#[derive(Serialize, Deserialize)]
struct Batch {
    source_id: String,
    sequence: u64,
    turns: Vec<TurnRecord>,
}

struct Journal {
    store: PathBuf,
    binding: Binding,
    next: u64,
    latest: BTreeMap<String, uuid::Uuid>,
    files: BTreeMap<PathBuf, (u64, SystemTime)>,
}

fn fingerprint(turn: &TurnRecord) -> Result<uuid::Uuid> {
    Ok(uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        &serde_json::to_vec(turn)?,
    ))
}

fn batch_paths(store: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![];
    for entry in fs::read_dir(store.join("turns"))? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|s| s == "json") {
            ensure!(
                entry.file_type()?.is_file(),
                "journal entries must be regular files"
            );
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

impl Journal {
    fn open(store: &Path) -> Result<Self> {
        let mut binding: Binding = serde_json::from_slice(&fs::read(store.join("source.json"))?)?;
        ensure!(
            binding.schema == "rroflow.source/v1",
            "unknown source binding version"
        );
        capture::endpoint(&binding.endpoint)?;
        let mut cached = vec![];
        for entry in fs::read_dir(store)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("settings-") && name.ends_with(".json") {
                ensure!(
                    entry.file_type()?.is_file(),
                    "settings cache must be a regular file"
                );
                cached.push(entry.path());
            }
        }
        cached.sort();
        if let Some(path) = cached.last() {
            let cfg: SignalConfig = serde_json::from_slice(&fs::read(path)?)?;
            cfg.validate().map_err(anyhow::Error::msg)?;
            binding.signals = cfg;
        }
        let mut journal = Self {
            store: store.into(),
            binding,
            next: 0,
            latest: BTreeMap::new(),
            files: BTreeMap::new(),
        };
        for path in batch_paths(store)? {
            let batch = journal.read_batch(&path)?;
            ensure!(
                batch.sequence == journal.next,
                "journal sequence gap; nothing discarded"
            );
            journal.next += 1;
            for turn in batch.turns {
                journal
                    .latest
                    .insert(turn.event_id.clone(), fingerprint(&turn)?);
            }
        }
        Ok(journal)
    }

    fn accept_settings(&mut self, response: Value) -> Result<()> {
        ensure!(
            response["devspace"].as_str() == Some(&self.binding.devspace),
            "settings belong to another devspace"
        );
        let cfg: SignalConfig = serde_json::from_value(response["settings"].clone())?;
        cfg.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            cfg.revision >= self.binding.signals.revision,
            "server settings revision moved backwards; cached choices retained"
        );
        capture::write_once(
            &self
                .store
                .join(format!("settings-{:020}.json", cfg.revision)),
            &serde_json::to_vec(&cfg)?,
        )?;
        if cfg.disabled != self.binding.signals.disabled
            || cfg.options != self.binding.signals.options
        {
            self.files.clear();
        }
        self.binding.signals = cfg;
        Ok(())
    }

    fn refresh_settings(&mut self, sender: &account::Sender) -> Result<()> {
        ensure!(
            capture::endpoint(&sender.base)? == self.binding.endpoint,
            "signed-in endpoint changed; settings not fetched"
        );
        let response = get(&client(), sender, "/v1/signals/settings")?;
        self.accept_settings(response)
    }

    fn read_batch(&self, path: &Path) -> Result<Batch> {
        ensure!(
            fs::metadata(path)?.len() <= (BATCH_LIMIT + 4096) as u64,
            "oversized journal batch"
        );
        let batch: Batch = serde_json::from_slice(&fs::read(path)?)
            .context("damaged journal; nothing discarded")?;
        ensure!(
            batch.source_id == self.binding.source_id
                && path.file_name().and_then(|s| s.to_str())
                    == Some(&format!("{:020}.json", batch.sequence)),
            "journal identity mismatch"
        );
        Ok(batch)
    }

    fn persist(&mut self, turns: Vec<TurnRecord>) -> Result<usize> {
        if turns.is_empty() {
            return Ok(0);
        }
        let count = turns.len();
        let batch = Batch {
            source_id: self.binding.source_id.clone(),
            sequence: self.next,
            turns,
        };
        let bytes = serde_json::to_vec(&batch)?;
        ensure!(
            bytes.len() <= BATCH_LIMIT + 4096,
            "batch too large; not acknowledged"
        );
        capture::write_once(
            &self
                .store
                .join("turns")
                .join(format!("{:020}.json", self.next)),
            &bytes,
        )?;
        // Advance only after durable write. Restart reconstructs these values from the journal.
        for turn in batch.turns {
            self.latest
                .insert(turn.event_id.clone(), fingerprint(&turn)?);
        }
        self.next += 1;
        Ok(count)
    }

    fn scan(&mut self, full: bool) -> Result<Value> {
        if full {
            self.files.clear();
        }
        let mut failures = vec![];
        let mut queued = 0;
        let mut seen = 0;
        let mut changed = 0;
        let redactor = Redactor::new();
        for (root, parse) in [
            (
                self.binding.claude_root.clone(),
                transcript::collect_transcript as crate::Parse,
            ),
            (
                self.binding.codex_root.clone(),
                codex::collect_rollout as crate::Parse,
            ),
        ] {
            let mut paths = vec![];
            discover(&root, &mut paths, &mut failures);
            paths.sort();
            for path in paths {
                seen += 1;
                let result = (|| -> Result<()> {
                    let metadata = fs::metadata(&path)?;
                    let stamp = (metadata.len(), metadata.modified()?);
                    if self.files.get(&path) == Some(&stamp) {
                        return Ok(());
                    }
                    let turns = parse(&path)?;
                    let records = record::records(
                        &turns,
                        None,
                        self.binding.content,
                        &redactor,
                        &self.binding.signals,
                    );
                    let mut batch = vec![];
                    let mut size = 0;
                    for turn in records {
                        if self.latest.get(&turn.event_id) == Some(&fingerprint(&turn)?) {
                            continue;
                        }
                        let bytes = serde_json::to_vec(&turn)?.len() + 1;
                        ensure!(
                            bytes <= BATCH_LIMIT,
                            "turn is {bytes} bytes, exceeding upload bound {BATCH_LIMIT}; coverage incomplete"
                        );
                        if batch.len() == 100 || size + bytes > BATCH_LIMIT {
                            queued += self.persist(std::mem::take(&mut batch))?;
                            size = 0;
                        }
                        batch.push(turn);
                        size += bytes;
                    }
                    queued += self.persist(batch)?;
                    // A concurrently appended file has a new stamp and is scanned next pass.
                    self.files.insert(path.clone(), stamp);
                    changed += 1;
                    Ok(())
                })();
                if let Err(error) = result {
                    failures.push(format!("{}: {error:#}", path.display()));
                }
            }
        }
        Ok(
            json!({"observed_at":chrono::Utc::now().to_rfc3339(), "source_id":self.binding.source_id,
            "files_seen":seen,"files_parsed":changed,"queued_revisions":queued,"errors":failures}),
        )
    }

    fn status(&self) -> Result<Value> {
        let mut pending = 0;
        let mut bytes = 0;
        let mut pending_bytes = 0;
        let mut oldest: Option<SystemTime> = None;
        for path in batch_paths(&self.store)? {
            let metadata = fs::metadata(&path)?;
            bytes += metadata.len();
            if !path.with_extension("acked").exists() {
                pending += self.read_batch(&path)?.turns.len();
                pending_bytes += metadata.len();
                if let Ok(time) = metadata.modified() {
                    oldest = Some(oldest.map_or(time, |old| old.min(time)));
                }
            }
        }
        Ok(
            json!({"source_id":self.binding.source_id,"devspace":self.binding.devspace,
            "endpoint":self.binding.endpoint,"claude_root":self.binding.claude_root,
            "codex_root":self.binding.codex_root,"content":self.binding.content,
            "pending_revisions":pending,"journal_bytes":bytes,"distinct_turns":self.latest.len(),"retry":retry::read(&self.store)?,
            "pending_bytes":pending_bytes,"oldest_pending_at":oldest.map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
            "note":"disk state only; not proof of source liveness. Acknowledged revisions are retained."}),
        )
    }

    /// Persist a monotonic sequence before any request. Caller holds the source lock.
    fn health_report(
        &self,
        capture_errors: u32,
        delivery_ok: Option<bool>,
    ) -> Result<SourceReport> {
        use std::io::Write;
        let path = self.store.join("health.json");
        let previous = match fs::symlink_metadata(&path) {
            Ok(meta) => {
                ensure!(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "health state must be a regular file"
                );
                let old: SourceReport = serde_json::from_slice(&fs::read(&path)?)?;
                old.validate().map_err(anyhow::Error::msg)?;
                ensure!(
                    old.source_id == self.binding.source_id,
                    "health state belongs to another source"
                );
                old.sequence
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => return Err(e.into()),
        };
        let state = self.status()?;
        let report = SourceReport {
            schema: HEALTH_SCHEMA.into(),
            source_id: self.binding.source_id.clone(),
            sequence: previous + 1,
            observed_at: chrono::Utc::now().to_rfc3339(),
            collector_version: env!("CARGO_PKG_VERSION").into(),
            pending_revisions: state["pending_revisions"]
                .as_u64()
                .context("pending count")?,
            pending_bytes: state["pending_bytes"].as_u64().context("pending bytes")?,
            journal_bytes: state["journal_bytes"].as_u64().context("journal bytes")?,
            oldest_pending_at: state["oldest_pending_at"].as_str().map(str::to_owned),
            capture_errors,
            delivery_ok,
            signal_settings_revision: self.binding.signals.revision,
        };
        report.validate().map_err(anyhow::Error::msg)?;
        let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        file.write_all(&serde_json::to_vec(&report)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(tmp, path)?;
        #[cfg(unix)]
        fs::File::open(&self.store)?.sync_all()?;
        Ok(report)
    }

    fn send_health(&self, sender: &account::Sender, report: &SourceReport) -> Result<()> {
        ensure!(
            capture::endpoint(&sender.base)? == self.binding.endpoint,
            "source health endpoint changed"
        );
        let client = client();
        let who = get(&client, sender, "/v1/whoami")?;
        ensure!(
            who["devspace"].as_str() == Some(&self.binding.devspace),
            "source health belongs to another devspace"
        );
        let mut response = client
            .post(&format!("{}/v1/sources/health", self.binding.endpoint))
            .header("Authorization", &format!("Bearer {}", sender.token))
            .send_json(report)?;
        retry::require_ok(&response)?;
        let receipt: Value = response.body_mut().read_json()?;
        ensure!(
            response.status().as_u16() == 200
                && receipt["devspace"].as_str() == Some(&self.binding.devspace)
                && receipt["observation"]["report"] == serde_json::to_value(report)?,
            "source health receipt invalid"
        );
        Ok(())
    }

    fn deliver(&self, sender: &account::Sender) -> Result<usize> {
        ensure!(
            capture::endpoint(&sender.base)? == self.binding.endpoint,
            "signed-in endpoint changed; queue retained without upload"
        );
        let client = client();
        let who = get(&client, sender, "/v1/whoami")?;
        ensure!(
            who["devspace"].as_str() == Some(&self.binding.devspace),
            "signed-in devspace changed; queue retained without upload"
        );
        let mut delivered = 0;
        // Bound work per pass so a large backlog does not starve new local capture.
        for path in batch_paths(&self.store)?
            .into_iter()
            .filter(|p| !p.with_extension("acked").exists())
            .take(16)
        {
            let batch = self.read_batch(&path)?;
            let mut response = client
                .post(&format!("{}/v1/turns", self.binding.endpoint))
                .header("Authorization", &format!("Bearer {}", sender.token))
                .send_json(json!({"turns":batch.turns}))?;
            retry::require_ok(&response)?;
            let receipt: Value = response.body_mut().read_json()?;
            ensure!(
                receipt["devspace"].as_str() == Some(&self.binding.devspace)
                    && receipt["accepted"].as_u64() == Some(batch.turns.len() as u64),
                "invalid upload receipt; queue retained"
            );
            capture::write_once(
                &path.with_extension("acked"),
                &serde_json::to_vec(&receipt)?,
            )?;
            delivered += batch.turns.len();
        }
        Ok(delivered)
    }
}

fn discover(path: &Path, paths: &mut Vec<PathBuf>, failures: &mut Vec<String>) {
    let result = (|| -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "symlink skipped; link its actual root explicitly"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                discover(&entry?.path(), paths, failures);
            }
        } else if metadata.is_file()
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".jsonl") || s.ends_with(".jsonl.zst"))
        {
            paths.push(path.into());
        }
        Ok(())
    })();
    if let Err(error) = result {
        failures.push(format!("{}: {error:#}", path.display()));
    }
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .timeout_connect(Some(Duration::from_secs(5)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn get(client: &ureq::Agent, sender: &account::Sender, path: &str) -> Result<Value> {
    let mut response = client
        .get(&format!("{}{path}", capture::endpoint(&sender.base)?))
        .header("Authorization", &format!("Bearer {}", sender.token))
        .call()?;
    retry::require_ok(&response)?;
    Ok(response.body_mut().read_json()?)
}

pub(crate) fn default_root(env: &str, hidden: &str, leaf: &str) -> Result<PathBuf> {
    let base = std::env::var_os(env)
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|s| PathBuf::from(s).join(hidden))
        })
        .context("home directory unavailable; pass explicit session roots")?;
    ensure!(base.is_absolute(), "session home must be absolute");
    Ok(base.join(leaf))
}

fn lock(store: &Path) -> Result<fs::File> {
    capture::private_dir(store)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.join("source.lock"))?;
    lock.try_lock()
        .context("another collector is using this source store")?;
    Ok(lock)
}

pub(crate) fn run(command: Command) -> Result<()> {
    if let Command::Worker { manifest } = &command {
        return service::worker(manifest);
    }
    let dir = match &command {
        Command::Link { store, .. }
        | Command::Run { store, .. }
        | Command::Service { store, .. }
        | Command::Status { store }
        | Command::Flush { store } => store.clone(),
        Command::Worker { .. } => unreachable!(),
    }
    .or_else(|| account::config_dir().map(|p| p.join("sources")))
    .context("source store unavailable")?;
    match command {
        Command::Worker { .. } => unreachable!(),
        Command::Service { command, .. } => return service::run(&dir, command),
        Command::Link {
            claude_root,
            codex_root,
            content,
            ..
        } => {
            let _lock = lock(&dir)?;
            ensure!(
                !dir.join("source.json").exists(),
                "source already linked; use sources status or run"
            );
            let claude_root = claude_root
                .map(Ok)
                .unwrap_or_else(|| default_root("CLAUDE_CONFIG_DIR", ".claude", "projects"))?;
            let codex_root = codex_root
                .map(Ok)
                .unwrap_or_else(|| default_root("CODEX_HOME", ".codex", "sessions"))?;
            ensure!(
                claude_root.is_absolute() && codex_root.is_absolute(),
                "session roots must be absolute"
            );
            ensure!(claude_root != codex_root, "agent roots must be distinct");
            let sender = account::sender()?.context("run rroflow login before linking sources")?;
            let endpoint = capture::endpoint(&sender.base)?;
            let client = client();
            let who = get(&client, &sender, "/v1/whoami")?;
            let devspace = who["devspace"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("missing devspace identity")?
                .into();
            let settings = get(&client, &sender, "/v1/signals/settings")?;
            ensure!(
                settings["devspace"] == who["devspace"],
                "signal settings belong to another devspace"
            );
            let signals: SignalConfig = serde_json::from_value(settings["settings"].clone())?;
            let binding = Binding {
                schema: "rroflow.source/v1".into(),
                source_id: uuid::Uuid::new_v4().to_string(),
                endpoint,
                devspace,
                claude_root,
                codex_root,
                content,
                signals,
            };
            capture::private_dir(&dir.join("turns"))?;
            capture::write_once(
                &dir.join("source.json"),
                &serde_json::to_vec_pretty(&binding)?,
            )?;
            println!("{}", Journal::open(&dir)?.status()?);
        }
        Command::Status { .. } => println!("{}", Journal::open(&dir)?.status()?),
        Command::Flush { .. } => {
            let _lock = lock(&dir)?;
            let journal = Journal::open(&dir)?;
            let acknowledged = retry::attempt(&dir, || {
                let sender =
                    account::sender()?.context("sign-in unavailable; local queue retained")?;
                journal.deliver(&sender)
            })?;
            println!(
                "{}",
                json!({"acknowledged_revisions":acknowledged,"state":journal.status()?})
            );
        }
        Command::Run {
            interval_secs,
            once,
            offline,
            account_file,
            quiet,
            ..
        } => {
            if let Some(path) = &account_file {
                ensure!(path.is_absolute(), "account file must be absolute");
            }
            let sender = || match &account_file {
                Some(path) => account::saved_sender(path),
                None => account::sender(),
            };
            let _lock = lock(&dir)?;
            let mut journal = Journal::open(&dir)?;
            let mut cycle = 0u64;
            let mut retry_at = std::time::Instant::now();
            let mut settings_at = std::time::Instant::now();
            let mut health_at = std::time::Instant::now();
            let mut last_delivery_ok = None;
            loop {
                let mut report = journal.scan(cycle.is_multiple_of(60))?;
                let capture_ok = report["errors"].as_array().is_some_and(|a| a.is_empty());
                let mut upload_ok = true;
                let mut health_ok = true;
                if !offline && std::time::Instant::now() >= retry_at {
                    let result = retry::attempt(&dir, || -> Result<(usize, Option<String>)> {
                        let sender =
                            sender()?.context("sign-in unavailable; local queue retained")?;
                        let count = journal.deliver(&sender)?;
                        let settings_error = if std::time::Instant::now() >= settings_at {
                            settings_at = std::time::Instant::now() + Duration::from_secs(60);
                            match journal.refresh_settings(&sender) {
                                Ok(()) => None,
                                Err(error)
                                    if error.downcast_ref::<retry::HttpFailure>().is_some() =>
                                {
                                    return Err(error);
                                }
                                Err(error) => Some(format!("{error:#}")),
                            }
                        } else {
                            None
                        };
                        Ok((count, settings_error))
                    });
                    match result {
                        Ok((count, settings_error)) => {
                            report["acknowledged_revisions"] = count.into();
                            report["signal_settings_revision"] =
                                journal.binding.signals.revision.into();
                            if let Some(error) = settings_error {
                                report["settings_error"] = error.into();
                            }
                        }
                        Err(error) => {
                            report["delivery_error"] = format!("{error:#}").into();
                            upload_ok = false;
                        }
                    }
                    retry_at = std::time::Instant::now() + Duration::from_secs(interval_secs);
                    last_delivery_ok = Some(upload_ok);
                }
                if !offline && std::time::Instant::now() >= health_at {
                    let result = retry::attempt(&dir, || -> Result<()> {
                        let errors = report["errors"].as_array().map_or(0, Vec::len);
                        let health = journal.health_report(
                            u32::try_from(errors).unwrap_or(u32::MAX),
                            last_delivery_ok,
                        )?;
                        let sender = sender()?.context("sign-in unavailable for source health")?;
                        journal.send_health(&sender, &health)
                    });
                    if let Err(error) = result {
                        health_ok = false;
                        report["health_error"] = format!("{error:#}").into();
                    }
                    health_at = std::time::Instant::now() + Duration::from_secs(30);
                }
                report["delivery"] = if offline {
                    "disabled"
                } else {
                    "automatic retry enabled"
                }
                .into();
                report["state"] = journal.status()?;
                if !quiet {
                    println!("{report}");
                } else if cycle.is_multiple_of(12) || !capture_ok || !upload_ok || !health_ok {
                    println!(
                        "{}",
                        json!({"source_id":journal.binding.source_id,"capture_errors":report["errors"].as_array().map_or(0,Vec::len),"pending_revisions":report["state"]["pending_revisions"],"delivery_ok":last_delivery_ok,"health_ok":health_ok,"retry":report["state"]["retry"]})
                    );
                }
                if once {
                    ensure!(
                        capture_ok && upload_ok && health_ok,
                        "capture, delivery or source reporting incomplete; inspect reported errors; queued data retained"
                    );
                    return Ok(());
                }
                cycle += 1;
                std::thread::sleep(Duration::from_secs(interval_secs));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn health_sequences_survive_restart_without_changing_capture_or_offline_behavior() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), "http://127.0.0.1:1");
        claude(&journal.binding.claude_root, 1);
        journal.scan(true).unwrap();
        let first = journal.health_report(2, Some(false)).unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(first.pending_revisions, 1);
        assert!(first.pending_bytes > 0);
        assert_eq!(first.pending_bytes, first.journal_bytes);
        assert!(first.oldest_pending_at.is_some());
        let text = serde_json::to_string(&first).unwrap();
        assert!(
            !text.contains("claude_root") && !text.contains("endpoint") && !text.contains("token")
        );
        drop(journal);
        run(Command::Run {
            store: Some(dir.path().into()),
            interval_secs: 1,
            once: true,
            offline: true,
            account_file: None,
            quiet: false,
        })
        .unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let next = journal.health_report(0, None).unwrap();
        assert_eq!(
            next.sequence, 2,
            "offline capture sends no heartbeat and consumes no report sequence"
        );
        assert_eq!(journal.status().unwrap()["pending_revisions"], 1);
        fs::write(dir.path().join("health.json"), b"partial").unwrap();
        assert!(journal.health_report(0, Some(true)).is_err());
        assert_eq!(journal.status().unwrap()["pending_revisions"], 1);
    }

    fn setup(root: &Path, endpoint: &str) -> Journal {
        let binding = Binding {
            schema: "rroflow.source/v1".into(),
            source_id: uuid::Uuid::new_v4().to_string(),
            endpoint: endpoint.into(),
            devspace: "owner".into(),
            claude_root: root.join("claude"),
            codex_root: root.join("codex"),
            content: false,
            signals: SignalConfig::default(),
        };
        for dir in [
            &binding.claude_root,
            &binding.codex_root,
            &root.join("turns"),
        ] {
            capture::private_dir(dir).unwrap();
        }
        capture::write_once(
            &root.join("source.json"),
            &serde_json::to_vec(&binding).unwrap(),
        )
        .unwrap();
        Journal::open(root).unwrap()
    }

    #[test]
    fn refreshed_signal_choices_are_bound_durable_and_monotonic() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), "http://127.0.0.1:1");
        let response =
            json!({"devspace":"owner","settings":{"revision":2,"disabled":["mood"],"options":{}}});
        journal.accept_settings(response.clone()).unwrap();
        journal.accept_settings(response).unwrap();
        assert!(
            !Journal::open(dir.path())
                .unwrap()
                .binding
                .signals
                .enabled("mood")
        );
        assert!(
            journal
                .accept_settings(json!({"devspace":"other","settings":{"revision":3}}))
                .is_err()
        );
        assert!(
            journal
                .accept_settings(json!({"devspace":"owner","settings":{"revision":1}}))
                .is_err()
        );
        assert_eq!(journal.binding.signals.revision, 2);
        assert!(
            journal
                .accept_settings(
                    json!({"devspace":"owner","settings":{"revision":2,"disabled":[]}})
                )
                .is_err()
        );
    }

    fn claude(root: &Path, id: usize) -> PathBuf {
        let project = root.join(format!("project-{}", id % 2));
        fs::create_dir_all(&project).unwrap();
        let path = project.join(format!("session-{id}.jsonl"));
        fs::write(
            &path,
            format!(
                "{}\n",
                json!({"type":"user","promptId":format!("p{id}"),
            "sessionId":format!("s{id}"),"timestamp":"2026-10-25T00:00:00Z",
            "cwd":format!("/projects/{}",id % 2),"message":{"content":"private prompt"}})
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn all_projects_offline_restart_and_changed_turn_are_durable() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), "http://127.0.0.1:1");
        // More than the old 12-file sample, with two working directories.
        for id in 0..25 {
            claude(&journal.binding.claude_root, id);
        }
        let codex = [
            json!({"type":"session_meta","payload":{"id":"codex-session","cwd":"/projects/third"}}),
            json!({"timestamp":"2026-10-25T00:00:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}),
            json!({"timestamp":"2026-10-25T00:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"private Codex prompt"}}),
        ].map(|v| v.to_string()).join("\n");
        fs::write(journal.binding.codex_root.join("session.jsonl"), codex).unwrap();
        let report = journal.scan(true).unwrap();
        assert_eq!(report["files_seen"], 26);
        assert_eq!(report["queued_revisions"], 26);
        assert_eq!(report["errors"], json!([]));
        assert_eq!(journal.status().unwrap()["pending_revisions"], 26);
        drop(journal);
        let mut journal = Journal::open(dir.path()).unwrap();
        assert_eq!(journal.scan(true).unwrap()["queued_revisions"], 0);
        let mut projects = std::collections::BTreeSet::new();
        for path in batch_paths(dir.path()).unwrap() {
            let bytes = fs::read(&path).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("private prompt"));
            for turn in journal.read_batch(&path).unwrap().turns {
                projects.insert(turn.project.unwrap());
            }
        }
        assert_eq!(projects.len(), 3);
        let path = journal
            .binding
            .claude_root
            .join("project-0/session-0.jsonl");
        writeln!(
            fs::OpenOptions::new().append(true).open(path).unwrap(),
            "{}",
            json!({"type":"assistant","sessionId":"s0","timestamp":"2026-10-25T00:00:02Z",
                "message":{"model":"claude-test","content":[{"type":"text","text":"finished"}]}})
        )
        .unwrap();
        assert_eq!(journal.scan(false).unwrap()["queued_revisions"], 1);
        assert_eq!(
            Journal::open(dir.path()).unwrap().status().unwrap()["distinct_turns"],
            26
        );
        assert_eq!(journal.status().unwrap()["pending_revisions"], 27);
        let _lock = lock(dir.path()).unwrap();
        assert!(lock(dir.path()).is_err());
    }

    #[test]
    fn missing_roots_and_damaged_journal_are_visible_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), "http://127.0.0.1:1");
        fs::remove_dir(&journal.binding.codex_root).unwrap();
        assert_eq!(
            journal.scan(true).unwrap()["errors"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        fs::write(
            dir.path().join("turns/00000000000000000000.json"),
            b"broken",
        )
        .unwrap();
        assert!(Journal::open(dir.path()).is_err());
    }

    #[test]
    fn multi_megabyte_turn_preserves_all_finished_tools() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), "http://127.0.0.1:1");
        let turn = transcript::Turn {
            key: "large".into(),
            session: "s".into(),
            agent: "codex",
            input: "inspect".into(),
            tools: (0..1300)
                .map(|i| transcript::Tool {
                    id: i.to_string(),
                    name: "shell".into(),
                    input: "inspect the source file\n".repeat(50),
                    output: "observed a tool result\n".repeat(50),
                    end: Some("2026-10-25T00:00:01Z".into()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let records = record::records(
            &[turn],
            None,
            true,
            &Redactor::new(),
            &SignalConfig::default(),
        );
        assert!(serde_json::to_vec(&records).unwrap().len() > 2 * 1024 * 1024);
        assert_eq!(journal.persist(records).unwrap(), 1);
        let recovered = Journal::open(dir.path()).unwrap();
        let path = batch_paths(dir.path()).unwrap().remove(0);
        assert_eq!(
            recovered.read_batch(&path).unwrap().turns[0].tools.len(),
            1300
        );
    }

    #[test]
    fn offline_delivery_and_partial_writes_preserve_captured_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), "http://127.0.0.1:1");
        let path = claude(&journal.binding.claude_root, 1);
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"type\":")
            .unwrap();
        assert_eq!(journal.scan(true).unwrap()["queued_revisions"], 1);
        let sender = account::Sender {
            base: "http://127.0.0.1:1".into(),
            token: "unused".into(),
            via: "test".into(),
        };
        assert!(journal.deliver(&sender).is_err());
        assert_eq!(
            Journal::open(dir.path()).unwrap().status().unwrap()["pending_revisions"],
            1
        );
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"broken}\n")
            .unwrap();
        assert_eq!(
            journal.scan(false).unwrap()["errors"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            Journal::open(dir.path()).unwrap().status().unwrap()["pending_revisions"],
            1
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn receipt_failure_restart_and_account_change_never_lose_or_redirect_records() {
        use axum::{
            Json, Router,
            extract::State,
            routing::{get, post},
        };
        use std::sync::{Arc, Mutex};
        #[derive(Default)]
        struct Server {
            wrong_account: bool,
            bad_receipt: bool,
            calls: usize,
            health_calls: usize,
            turns: BTreeMap<String, Value>,
        }
        type Shared = Arc<Mutex<Server>>;
        async fn who(State(s): State<Shared>) -> Json<Value> {
            Json(json!({"devspace": if s.lock().unwrap().wrong_account {"other"} else {"owner"}}))
        }
        async fn upload(State(s): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
            let mut state = s.lock().unwrap();
            state.calls += 1;
            let turns = body["turns"].as_array().unwrap();
            for turn in turns {
                state
                    .turns
                    .insert(turn["event_id"].as_str().unwrap().into(), turn.clone());
            }
            Json(
                json!({"devspace":"owner", "accepted":if state.bad_receipt {0} else {turns.len()}}),
            )
        }
        async fn health(State(s): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
            let mut state = s.lock().unwrap();
            state.health_calls += 1;
            Json(
                json!({"devspace":if state.bad_receipt {"other"} else {"owner"},"observation":{"report":body}}),
            )
        }
        let state = Shared::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/v1/whoami", get(who))
            .route("/v1/turns", post(upload))
            .route("/v1/sources/health", post(health))
            .with_state(state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let mut journal = setup(dir.path(), &endpoint);
        claude(&journal.binding.claude_root, 1);
        journal.scan(true).unwrap();
        let sender = account::Sender {
            base: endpoint,
            token: "test-only".into(),
            via: "key".into(),
        };
        state.lock().unwrap().wrong_account = true;
        let report = journal.health_report(1, Some(false)).unwrap();
        assert!(journal.send_health(&sender, &report).is_err());
        assert_eq!(state.lock().unwrap().health_calls, 0);
        assert!(journal.deliver(&sender).is_err());
        assert_eq!(state.lock().unwrap().calls, 0);
        {
            let mut state = state.lock().unwrap();
            state.wrong_account = false;
            state.bad_receipt = true;
        }
        // Server committed but client cannot trust receipt. Restart must replay.
        assert!(journal.deliver(&sender).is_err());
        assert_eq!(journal.status().unwrap()["pending_revisions"], 1);
        drop(journal);
        let journal = Journal::open(dir.path()).unwrap();
        state.lock().unwrap().bad_receipt = false;
        assert_eq!(journal.deliver(&sender).unwrap(), 1);
        assert_eq!(journal.deliver(&sender).unwrap(), 0);
        assert_eq!(journal.status().unwrap()["pending_revisions"], 0);
        assert_eq!(state.lock().unwrap().turns.len(), 1);
        assert_eq!(state.lock().unwrap().calls, 2);
        let report = journal.health_report(0, Some(true)).unwrap();
        assert_eq!(report.pending_revisions, 0);
        assert_eq!(report.pending_bytes, 0);
        assert!(report.oldest_pending_at.is_none());
        assert!(report.journal_bytes > 0);
        journal.send_health(&sender, &report).unwrap();
        state.lock().unwrap().bad_receipt = true;
        assert!(journal.send_health(&sender, &report).is_err());
        assert_eq!(
            journal.status().unwrap()["pending_revisions"],
            0,
            "a failed health receipt cannot undo acknowledged turns"
        );
        server.abort();
    }
}
