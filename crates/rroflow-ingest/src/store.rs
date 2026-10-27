//! Where devspace data lives. [`Store`] picks one backend from the config:
//! [`PgStore`](crate::pg::PgStore) when `database_url` is set (the hosted service), else
//! [`JsonlStore`] (a single-user local service with no database).

use crate::pg::PgStore;
use rroflow_core::TurnRecord;
use rroflow_core::signals::SignalConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// Counts returned by an upsert.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Upserted {
    pub new: usize,
    pub replaced: usize,
}

/// Totals for one devspace.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub turns: usize,
    pub by_agent: BTreeMap<String, usize>,
    pub by_project: BTreeMap<String, usize>,
    pub signals: BTreeMap<String, usize>,
    /// `agent -> signal -> hits`, so Claude Code and Codex can be compared.
    pub signals_by_agent: BTreeMap<String, BTreeMap<String, usize>>,
    pub tool_calls: usize,
    pub tool_errors: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    /// Turns whose usage the provider did not report (counted, never priced as zero).
    pub turns_without_usage: usize,
}

/// Which turns a list returns.
#[derive(Debug, Default, Clone, Copy)]
pub struct TurnFilter<'a> {
    /// Only this agent (`claude-code`, `codex`).
    pub agent: Option<&'a str>,
    /// Only turns where this signal fired.
    pub signal: Option<&'a str>,
    pub project: Option<&'a str>,
    pub session: Option<&'a str>,
    pub search: Option<&'a str>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub before: Option<(Option<chrono::DateTime<chrono::Utc>>, &'a str)>,
    pub event: Option<&'a str>,
}

fn project(r: &TurnRecord) -> &str {
    r.project
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| (!r.cwd.is_empty()).then_some(r.cwd.as_str()))
        .unwrap_or("(none)")
}

/// The configured backend.
pub enum Store {
    Jsonl(JsonlStore),
    Postgres(PgStore),
}

impl Store {
    /// Connectivity/readability only; does not claim free disk, backup health or write success.
    pub async fn ready(&self) -> anyhow::Result<()> {
        match self {
            Store::Jsonl(store) => anyhow::ensure!(
                std::fs::metadata(&store.root)?.is_dir(),
                "store directory unavailable"
            ),
            Store::Postgres(store) => {
                sqlx::query("SELECT event_id FROM turns LIMIT 0")
                    .execute(&store.pool)
                    .await?;
            }
        }
        Ok(())
    }
    /// Make sure each devspace exists (devspaces registered by key in the config).
    pub async fn ensure_devspaces(&self, ids: &[String]) -> anyhow::Result<()> {
        match self {
            Store::Jsonl(_) => Ok(()),
            Store::Postgres(p) => p.ensure_devspaces(ids).await,
        }
    }

    /// The devspace a Nomen account owns, created on its first sign-in. Postgres only:
    /// the config check refuses `[nomen]` without a database.
    pub async fn devspace_for_owner(
        &self,
        issuer: &str,
        subject: &crate::Subject,
    ) -> anyhow::Result<String> {
        match self {
            Store::Jsonl(_) => anyhow::bail!("Nomen sign-in needs the Postgres store"),
            Store::Postgres(p) => p.devspace_for_owner(issuer, subject).await,
        }
    }

    /// Insert or replace records by `event_id`. Within one batch the last record for an
    /// `event_id` wins.
    pub async fn upsert(&self, devspace: &str, recs: &[TurnRecord]) -> anyhow::Result<Upserted> {
        match self {
            Store::Jsonl(s) => s.upsert(devspace, recs),
            Store::Postgres(p) => p.upsert(devspace, recs).await,
        }
    }

    /// Newest first; only turns from `filter.agent` and with `filter.signal` when given.
    pub async fn list(
        &self,
        devspace: &str,
        limit: usize,
        filter: &TurnFilter<'_>,
    ) -> anyhow::Result<Vec<TurnRecord>> {
        match self {
            Store::Jsonl(s) => Ok(s.list(devspace, limit, filter)),
            Store::Postgres(p) => p.list(devspace, limit, filter).await,
        }
    }

    pub async fn summary(&self, devspace: &str) -> anyhow::Result<Summary> {
        match self {
            Store::Jsonl(s) => Ok(s.summary(devspace)),
            Store::Postgres(p) => p.summary(devspace).await,
        }
    }

    /// The devspace's signal choices; catalogue defaults when none were saved.
    pub async fn signal_config(&self, devspace: &str) -> anyhow::Result<SignalConfig> {
        match self {
            Store::Jsonl(s) => s.signal_config(devspace),
            Store::Postgres(p) => p.signal_config(devspace).await,
        }
    }

    /// Save the devspace's signal choices. The caller validates them first.
    pub async fn set_signal_config(
        &self,
        devspace: &str,
        cfg: &SignalConfig,
    ) -> anyhow::Result<()> {
        match self {
            Store::Jsonl(s) => s.set_signal_config(devspace, cfg),
            Store::Postgres(p) => p.set_signal_config(devspace, cfg).await,
        }
    }
}

/// Keep the last record for each `event_id`, in first-seen order.
pub(crate) fn last_per_event(recs: &[TurnRecord]) -> Vec<&TurnRecord> {
    let mut at: HashMap<&str, usize> = HashMap::new();
    let mut out: Vec<&TurnRecord> = Vec::with_capacity(recs.len());
    for r in recs {
        match at.get(r.event_id.as_str()) {
            Some(&i) => out[i] = r,
            None => {
                at.insert(&r.event_id, out.len());
                out.push(r);
            }
        }
    }
    out
}

/// Append-only JSONL per devspace (`<data_dir>/<devspace>/turns.jsonl`), indexed in memory.
/// The last line for an `event_id` wins, so a re-send replaces the earlier record. Signal
/// settings live beside it in `signal_settings.json`.
pub struct JsonlStore {
    pub(crate) root: PathBuf,
    index: Mutex<HashMap<String, HashMap<String, TurnRecord>>>,
    capture_index: Mutex<HashMap<String, Vec<rroflow_core::capture::CaptureEvent>>>,
}

const SETTINGS_FILE: &str = "signal_settings.json";

impl JsonlStore {
    pub(crate) fn review_turn(&self, ds: &str, id: &str) -> Option<TurnRecord> {
        self.index
            .lock()
            .expect("store lock")
            .get(ds)?
            .get(id)
            .cloned()
    }

    fn capture_read(&self, ds: &str) -> anyhow::Result<Vec<rroflow_core::capture::CaptureEvent>> {
        let dir = self.root.join(ds).join("capture");
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut out = vec![];
        for file in std::fs::read_dir(dir)? {
            let path = file?.path();
            if path.extension().is_some_and(|e| e == "json") {
                out.push(serde_json::from_slice(&std::fs::read(path)?)?);
            }
        }
        Ok(out)
    }
    pub(crate) fn capture_insert(
        &self,
        ds: &str,
        events: &[rroflow_core::capture::CaptureEvent],
    ) -> anyhow::Result<()> {
        let mut index = self.capture_index.lock().expect("capture lock");
        if !index.contains_key(ds) {
            index.insert(ds.into(), self.capture_read(ds)?);
        }
        let known = index.get_mut(ds).expect("loaded capture");
        let mut fresh: Vec<&rroflow_core::capture::CaptureEvent> = vec![];
        for e in events {
            if let Some(old) = known.iter().chain(fresh.iter().copied()).find(|old| {
                old.event_id == e.event_id
                    || (old.capture_id == e.capture_id && old.sequence == e.sequence)
            }) {
                if old != e {
                    return Err(crate::capture::Conflict.into());
                }
            } else {
                fresh.push(e);
            }
        }
        let dir = self.root.join(ds).join("capture");
        std::fs::create_dir_all(&dir)?;
        for e in fresh {
            let path = dir.join(format!("{}.json", e.event_id));
            let tmp = dir.join(format!("{}.{}.tmp", e.event_id, rand::random::<u64>()));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            file.write_all(&serde_json::to_vec(e)?)?;
            file.sync_all()?;
            drop(file);
            std::fs::hard_link(&tmp, &path)?;
            std::fs::remove_file(tmp)?;
            known.push(e.clone());
        }
        // On Unix persist directory entries before acknowledging durable ingestion.
        #[cfg(unix)]
        for directory in [&dir, &self.root.join(ds), &self.root] {
            std::fs::File::open(directory)?.sync_all()?;
        }
        Ok(())
    }
    pub(crate) fn capture_list(
        &self,
        ds: &str,
        agent: Option<&str>,
        capture: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<rroflow_core::capture::CaptureEvent>> {
        let mut index = self.capture_index.lock().expect("capture lock");
        if !index.contains_key(ds) {
            index.insert(ds.into(), self.capture_read(ds)?);
        }
        let mut events = index.get(ds).expect("loaded capture").clone();
        events.retain(|e| {
            agent.is_none_or(|a| a == e.agent) && capture.is_none_or(|c| c == e.capture_id)
        });
        events.sort_by(|a, b| {
            b.observed_at
                .cmp(&a.observed_at)
                .then(a.capture_id.cmp(&b.capture_id))
                .then(b.sequence.cmp(&a.sequence))
        });
        events.truncate(limit);
        Ok(events)
    }
}

impl JsonlStore {
    /// Open the store and load every devspace's records.
    pub fn open(root: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&root)?;
        let mut index: HashMap<String, HashMap<String, TurnRecord>> = HashMap::new();
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let file = entry.path().join("turns.jsonl");
            if !file.is_file() {
                continue;
            }
            let ds = entry.file_name().to_string_lossy().to_string();
            let map = index.entry(ds).or_default();
            for line in std::io::BufReader::new(std::fs::File::open(&file)?).lines() {
                if let Ok(rec) = serde_json::from_str::<TurnRecord>(&line?) {
                    map.insert(rec.event_id.clone(), rec);
                }
            }
        }
        Ok(Self {
            root,
            index: Mutex::new(index),
            capture_index: Mutex::new(HashMap::new()),
        })
    }

    fn upsert(&self, devspace: &str, recs: &[TurnRecord]) -> anyhow::Result<Upserted> {
        let dir = self.root.join(devspace);
        std::fs::create_dir_all(&dir)?;
        let mut index = self.index.lock().expect("store lock");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("turns.jsonl"))?;
        let map = index.entry(devspace.to_string()).or_default();
        let mut out = Upserted::default();
        for rec in last_per_event(recs) {
            writeln!(f, "{}", serde_json::to_string(rec)?)?;
            if map.insert(rec.event_id.clone(), rec.clone()).is_some() {
                out.replaced += 1;
            } else {
                out.new += 1;
            }
        }
        f.flush()?;
        Ok(out)
    }

    fn list(&self, devspace: &str, limit: usize, f: &TurnFilter<'_>) -> Vec<TurnRecord> {
        let index = self.index.lock().expect("store lock");
        let mut v: Vec<TurnRecord> = index
            .get(devspace)
            .map(|m| {
                m.values()
                    .filter(|r| f.agent.is_none_or(|a| r.agent == a))
                    .filter(|r| f.project.is_none_or(|p| project(r) == p))
                    .filter(|r| f.session.is_none_or(|s| r.session_id == s))
                    .filter(|r| f.event.is_none_or(|id| r.event_id == id))
                    .filter(|r| {
                        f.since.is_none_or(|since| {
                            crate::pg::timestamp(&r.started_at).is_some_and(|at| at >= since)
                        })
                    })
                    .filter(|r| {
                        f.before.is_none_or(|(at, id)| {
                            let time = crate::pg::timestamp(&r.started_at);
                            time < at || (time == at && r.event_id.as_str() > id)
                        })
                    })
                    .filter(|r| {
                        f.search.is_none_or(|query| {
                            let content = r.content.as_ref();
                            [
                                r.session_id.as_str(),
                                r.model.as_deref().unwrap_or(""),
                                content.map_or("", |c| c.input.as_str()),
                                content.map_or("", |c| c.output.as_str()),
                            ]
                            .join("\n")
                            .to_lowercase()
                            .contains(&query.to_lowercase())
                        })
                    })
                    .filter(|r| {
                        f.signal
                            .is_none_or(|s| r.signals.iter().any(|h| h.signal == s))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| {
            crate::pg::timestamp(&b.started_at)
                .cmp(&crate::pg::timestamp(&a.started_at))
                .then_with(|| a.event_id.cmp(&b.event_id))
        });
        v.truncate(limit);
        v
    }

    fn summary(&self, devspace: &str) -> Summary {
        let index = self.index.lock().expect("store lock");
        let mut s = Summary::default();
        for r in index.get(devspace).into_iter().flat_map(|m| m.values()) {
            s.turns += 1;
            *s.by_agent.entry(r.agent.clone()).or_default() += 1;
            *s.by_project.entry(project(r).into()).or_default() += 1;
            for h in &r.signals {
                *s.signals.entry(h.signal.clone()).or_default() += 1;
                *s.signals_by_agent
                    .entry(r.agent.clone())
                    .or_default()
                    .entry(h.signal.clone())
                    .or_default() += 1;
            }
            s.tool_calls += r.tools.len();
            s.tool_errors += r
                .tools
                .iter()
                .filter(|t| t.status == rroflow_core::ToolStatus::Error)
                .count();
            match (r.usage.input_tokens, r.usage.output_tokens) {
                (Some(i), Some(o)) => {
                    s.input_tokens += i;
                    s.output_tokens += o;
                    s.cache_read_tokens += r.usage.cache_read_tokens.unwrap_or(0);
                }
                _ => s.turns_without_usage += 1,
            }
        }
        s
    }

    fn signal_config(&self, devspace: &str) -> anyhow::Result<SignalConfig> {
        let path = self.root.join(devspace).join(SETTINGS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SignalConfig::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn set_signal_config(&self, devspace: &str, cfg: &SignalConfig) -> anyhow::Result<()> {
        let dir = self.root.join(devspace);
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(cfg)?)?;
        std::fs::rename(tmp, dir.join(SETTINGS_FILE))?;
        Ok(())
    }
}
