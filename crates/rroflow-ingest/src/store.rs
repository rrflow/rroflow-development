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
}

/// The configured backend.
pub enum Store {
    Jsonl(JsonlStore),
    Postgres(PgStore),
}

impl Store {
    /// Make sure each devspace exists (devspaces registered by key in the config).
    pub async fn ensure_devspaces(&self, ids: &[String]) -> anyhow::Result<()> {
        match self {
            Store::Jsonl(_) => Ok(()),
            Store::Postgres(p) => p.ensure_devspaces(ids).await,
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
    root: PathBuf,
    index: Mutex<HashMap<String, HashMap<String, TurnRecord>>>,
}

const SETTINGS_FILE: &str = "signal_settings.json";

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
                    .filter(|r| {
                        f.signal
                            .is_none_or(|s| r.signals.iter().any(|h| h.signal == s))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v.truncate(limit);
        v
    }

    fn summary(&self, devspace: &str) -> Summary {
        let index = self.index.lock().expect("store lock");
        let mut s = Summary::default();
        for r in index.get(devspace).into_iter().flat_map(|m| m.values()) {
            s.turns += 1;
            *s.by_agent.entry(r.agent.clone()).or_default() += 1;
            *s.by_project
                .entry(r.project.clone().unwrap_or_else(|| "(none)".into()))
                .or_default() += 1;
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
