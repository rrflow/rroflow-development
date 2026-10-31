//! Explicit private SDK queue links. No filesystem-wide discovery or credential storage.
use super::{Journal, client, get};
use crate::{account, capture, retry};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use rroflow_core::source_health::SdkReport;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub(crate) enum Action {
    /// Link this private SDK queue to the source's verified account. No service is installed.
    Link {
        #[arg(long)]
        spool: PathBuf,
        /// Maximum events from this queue per selected collector pass.
        #[arg(long,default_value_t=16,value_parser=clap::value_parser!(u32).range(1..=256))]
        batch_size: u32,
        /// Stop starting requests after this budget; an in-flight request may finish.
        #[arg(long, default_value_t=2, value_parser=clap::value_parser!(u64).range(1..=60))]
        time_budget_secs: u64,
    },
    /// Stop future managed passes for this queue; retain all events and receipts.
    Unlink {
        #[arg(long)]
        spool: PathBuf,
    },
    /// Show explicit local links and queue counts without sending anything.
    Status,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    schema: String,
    source_id: String,
    endpoint: String,
    devspace: String,
    spool: PathBuf,
    batch_size: u32,
    time_budget_secs: u64,
}
fn registry(journal: &Journal) -> PathBuf {
    journal.store.join("sdk-links")
}
fn link_path(dir: &Path, spool: &Path) -> PathBuf {
    let id = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        spool.to_string_lossy().as_bytes(),
    );
    dir.join(format!("{id}.json"))
}
fn links(journal: &Journal) -> Result<Vec<Link>> {
    let dir = registry(journal);
    if !dir.try_exists()? {
        return Ok(vec![]);
    }
    ensure!(
        fs::symlink_metadata(&dir)?.file_type().is_dir(),
        "SDK registry must be a regular directory"
    );
    let mut paths = vec![];
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            paths.push(path);
        }
    }
    ensure!(paths.len() <= 16, "too many SDK links");
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let meta = fs::symlink_metadata(&path)?;
            ensure!(
                meta.file_type().is_file() && meta.len() <= 16384,
                "invalid SDK link file"
            );
            let link: Link = serde_json::from_slice(&fs::read(&path)?)?;
            ensure!(
                link.schema == "rroflow.sdk-link/v1"
                    && link.source_id == journal.binding.source_id
                    && link.endpoint == journal.binding.endpoint
                    && link.devspace == journal.binding.devspace
                    && link.spool.is_absolute()
                    && (1..=256).contains(&link.batch_size)
                    && (1..=60).contains(&link.time_budget_secs)
                    && path == link_path(&dir, &link.spool),
                "SDK link identity mismatch"
            );
            Ok(link)
        })
        .collect()
}
fn check(link: &Link) -> Result<()> {
    ensure!(
        fs::symlink_metadata(&link.spool)?.file_type().is_dir(),
        "SDK queue missing or replaced"
    );
    ensure!(
        link.spool.join("destination.state").try_exists()?,
        "SDK queue destination missing; nothing recreated"
    );
    capture::bind(&link.spool, &link.endpoint, &link.devspace)
}
pub(super) fn inspect(journal: &Journal) -> Option<SdkReport> {
    let entries = match links(journal) {
        Ok(v) => v,
        Err(_) => {
            return Some(SdkReport {
                queue_errors: 1,
                ..SdkReport::default()
            });
        }
    };
    if entries.is_empty() {
        return None;
    }
    let mut report = SdkReport {
        linked_queues: entries.len() as u32,
        ..SdkReport::default()
    };
    for link in entries {
        let result = (|| -> Result<()> {
            check(&link)?;
            let stats = capture::stats(&link.spool)?;
            let retry = retry::read(&link.spool)?;
            report.pending_events += stats.pending_events;
            report.pending_bytes += stats.pending_bytes;
            report.retained_bytes += stats.retained_bytes;
            if retry.attempts_since_success > 0 {
                report.retrying_queues += 1;
            }
            Ok(())
        })();
        if result.is_err() {
            report.queue_errors += 1;
        }
    }
    Some(report)
}
pub(super) fn cycle(
    journal: &Journal,
    cycle: u64,
    sender: impl FnOnce() -> Result<Option<account::Sender>>,
) -> Result<usize> {
    let entries = links(journal)?;
    if entries.is_empty() {
        return Ok(0);
    }
    // Round-robin one queue per pass. An offline/busy queue cannot starve other links.
    let link = &entries[(cycle % entries.len() as u64) as usize];
    check(link)?;
    if capture::stats(&link.spool)?.pending_events == 0 {
        return Ok(0);
    }
    capture::deliver_managed(
        &link.spool,
        || {
            let sender = sender()?.context("sign-in unavailable; SDK queue retained")?;
            ensure!(
                capture::endpoint(&sender.base)? == link.endpoint,
                "SDK source endpoint changed; queue retained"
            );
            Ok(sender)
        },
        link.batch_size as usize,
        link.time_budget_secs,
        &link.devspace,
    )
}
fn canonical_missing(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path;
    let mut tail = vec![];
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut base) => {
                for part in tail.into_iter().rev() {
                    base.push(part);
                }
                return Ok(base);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tail.push(
                    ancestor
                        .file_name()
                        .context("missing queue has no resolvable parent")?
                        .to_os_string(),
                );
                ancestor = ancestor
                    .parent()
                    .context("missing queue parent unavailable")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(super) fn run(journal: &Journal, action: Action) -> Result<()> {
    if matches!(action, Action::Status) {
        let links = links(journal)?;
        println!(
            "{}",
            serde_json::json!({"links":links,"health":inspect(journal),"note":"Local queue state, not process liveness"})
        );
        return Ok(());
    }
    let dir = registry(journal);
    capture::private_dir(&dir)?;
    let lock_path = dir.join("registry.lock");
    if lock_path.try_exists()? {
        ensure!(
            fs::symlink_metadata(&lock_path)?.file_type().is_file(),
            "invalid registry lock"
        );
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock()
        .context("another SDK link change is running")?;
    let (spool, batch_size, time_budget_secs) = match &action {
        Action::Link {
            spool,
            batch_size,
            time_budget_secs,
        } => (spool, *batch_size, *time_budget_secs),
        Action::Unlink { spool } => (spool, 16, 2),
        Action::Status => unreachable!(),
    };
    ensure!(spool.is_absolute(), "SDK queue must be an absolute path");
    if matches!(action, Action::Link { .. }) {
        capture::private_dir(spool)?;
    }
    let spool = match fs::canonicalize(spool) {
        Ok(path) => path,
        Err(error) if matches!(action, Action::Unlink { .. }) => {
            // Resolve a surviving parent first: Windows temp paths may use an
            // 8.3 alias while the saved link contains its canonical long name.
            let requested = canonical_missing(spool).context(error)?;
            links(journal)?
                .into_iter()
                .find(|l| l.spool == requested)
                .map(|l| l.spool)
                .context("SDK link not found for unavailable queue")?
        }
        Err(error) => return Err(error).context("SDK queue path unavailable"),
    };
    let path = link_path(&dir, &spool);
    if matches!(action, Action::Unlink { .. }) {
        // Delete only this exact owned registry file; queue contents are never removed.
        let entries = links(journal)?;
        if entries.iter().any(|l| l.spool == spool) {
            fs::remove_file(&path)?;
        }
        #[cfg(unix)]
        fs::File::open(&dir)?.sync_all()?;
        println!(
            "{}",
            serde_json::json!({"linked":false,"queue":"retained","in_flight":"A pass already in progress may finish"})
        );
        return Ok(());
    }
    let entries = links(journal)?;
    ensure!(
        entries.len() < 16 || entries.iter().any(|l| l.spool == spool),
        "SDK link limit reached (16)"
    );
    let sender = account::sender()?.context("sign in before linking an SDK queue")?;
    ensure!(
        capture::endpoint(&sender.base)? == journal.binding.endpoint,
        "source belongs to another endpoint"
    );
    let who = get(&client(), &sender, "/v1/whoami")?;
    ensure!(
        who["devspace"].as_str() == Some(&journal.binding.devspace),
        "source belongs to another account"
    );
    capture::bind(&spool, &journal.binding.endpoint, &journal.binding.devspace)?;
    let link = Link {
        schema: "rroflow.sdk-link/v1".into(),
        source_id: journal.binding.source_id.clone(),
        endpoint: journal.binding.endpoint.clone(),
        devspace: journal.binding.devspace.clone(),
        spool,
        batch_size,
        time_budget_secs,
    };
    capture::write_once(&path, &serde_json::to_vec(&link)?)?;
    println!(
        "{}",
        serde_json::json!({"linked":true,"queue":link.spool,"batch_size":batch_size,"startup":"Use sources service install or the existing running collector"})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_queue_keeps_canonical_parent_identity() {
        let dir = tempfile::tempdir().unwrap();
        let requested = dir.path().join(".").join("removed-parent").join("queue");
        let expected = fs::canonicalize(dir.path())
            .unwrap()
            .join("removed-parent")
            .join("queue");
        assert_eq!(canonical_missing(&requested).unwrap(), expected);
        assert!(!requested.exists());
    }
}
