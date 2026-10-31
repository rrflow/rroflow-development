//! Durable local SDK outbox. Recording never requires network or provider keys.
use crate::{account, redact::Redactor, retry};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use rroflow_core::capture::CaptureEvent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

const MAX_EVENT: usize = 4 * 1024 * 1024;

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Accept one JSON event per stdin line; acknowledge only after local fsync.
    Record {
        #[arg(long)]
        spool: PathBuf,
        /// Destination binding. HTTPS, or loopback HTTP for local operation.
        #[arg(long)]
        endpoint: String,
        /// Expected devspace from rroflow whoami; never reassigned during replay.
        #[arg(long)]
        devspace: String,
        /// Retain scrubbed event payloads. Off by default, including locally.
        #[arg(long)]
        content: bool,
    },
    /// Replay unacknowledged events using rroflow login or the headless ingest key.
    Flush {
        #[arg(long)]
        spool: PathBuf,
    },
    /// Report local delivery counts without reading credentials or using network.
    Status {
        #[arg(long)]
        spool: PathBuf,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Queued {
    endpoint: String,
    devspace: String,
    event: CaptureEvent,
}

pub(crate) fn endpoint(s: &str) -> Result<String> {
    let uri: ureq::http::Uri = s.parse().context("invalid ingest endpoint")?;
    ensure!(
        uri.authority().is_some_and(|a| !a.as_str().contains('@'))
            && uri.query().is_none()
            && !s.contains('#')
            && uri.path() == "/",
        "endpoint must be an origin without credentials, path, query or fragment"
    );
    ensure!(
        uri.scheme_str() == Some("https")
            || (uri.scheme_str() == Some("http")
                && matches!(uri.host(), Some("127.0.0.1" | "localhost" | "[::1]"))),
        "ingest requires HTTPS except loopback HTTP"
    );
    Ok(s.trim_end_matches('/').into())
}

pub(crate) fn private_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    ensure!(
        !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "spool must not be a symlink"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn write_once(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        ensure!(
            std::fs::read(path)? == bytes,
            "local event identity conflict"
        );
        return Ok(());
    }
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    match std::fs::hard_link(&tmp, path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => ensure!(
            std::fs::read(path)? == bytes,
            "local event identity conflict"
        ),
        Err(e) => return Err(e.into()),
    }
    std::fs::remove_file(tmp)?;
    #[cfg(unix)]
    std::fs::File::open(path.parent().context("spool parent")?)?.sync_all()?;
    Ok(())
}

fn scrub(v: &mut Value, r: &Redactor) {
    match v {
        Value::String(s) => *s = r.apply(s),
        Value::Array(a) => a.iter_mut().for_each(|v| scrub(v, r)),
        Value::Object(m) => {
            for (k, v) in m {
                let key = k.to_ascii_lowercase().replace(['-', '_'], "");
                if [
                    "authorization",
                    "apikey",
                    "accesskey",
                    "password",
                    "secret",
                    "clientsecret",
                    "token",
                    "accesstoken",
                    "refreshtoken",
                    "cookie",
                    "setcookie",
                    "headers",
                    "environment",
                    "env",
                ]
                .contains(&key.as_str())
                {
                    *v = Value::String("[REDACTED]".into());
                } else {
                    scrub(v, r)
                }
            }
        }
        _ => {}
    }
}

fn record<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    spool: &Path,
    base: &str,
    devspace: &str,
    content: bool,
) -> Result<()> {
    let base = endpoint(base)?;
    ensure!(
        !devspace.is_empty()
            && devspace.len() <= 64
            && devspace
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "invalid devspace binding"
    );
    bind(spool, &base, devspace)?;
    let redactor = Redactor::new();
    loop {
        let mut line = Vec::new();
        let n = input
            .by_ref()
            .take((MAX_EVENT + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        ensure!(
            n <= MAX_EVENT && line.last() == Some(&b'\n'),
            "event too large or unterminated JSON line"
        );
        let mut event: CaptureEvent =
            serde_json::from_slice(&line).context("invalid capture event JSON")?;
        event.validate().map_err(anyhow::Error::msg)?;
        chrono::DateTime::parse_from_rfc3339(&event.observed_at)
            .context("invalid capture timestamp")?;
        if content {
            if let Some(p) = &mut event.payload {
                scrub(p, &redactor)
            }
        } else {
            event.payload = None;
        }
        // Correlation fields must never carry a known credential, even in metadata-only mode.
        let mut metadata = event.clone();
        metadata.payload = None;
        ensure!(
            !crate::redact::leaks(&redactor, &serde_json::to_string(&metadata)?),
            "credential-shaped capture metadata refused"
        );
        let id = event.event_id.clone();
        let queued = Queued {
            endpoint: base.clone(),
            devspace: devspace.into(),
            event,
        };
        let encoded = serde_json::to_vec(&queued)?;
        ensure!(
            encoded.len() <= MAX_EVENT,
            "scrubbed event exceeds queue size limit"
        );
        write_once(&spool.join(format!("{id}.json")), &encoded)?;
        writeln!(
            output,
            "{}",
            json!({"event_id": id, "durable": true, "delivery": "queued"})
        )?;
        output.flush()?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Destination {
    schema: String,
    endpoint: String,
    devspace: String,
}

pub(crate) fn bind(spool: &Path, base: &str, devspace: &str) -> Result<()> {
    private_dir(spool)?;
    let expected = Destination {
        schema: "rroflow.sdk-destination/v1".into(),
        endpoint: endpoint(base)?,
        devspace: devspace.into(),
    };
    let path = spool.join("destination.state");
    if path.try_exists()? {
        ensure!(
            std::fs::symlink_metadata(&path)?.file_type().is_file(),
            "invalid SDK destination file"
        );
        ensure!(
            std::fs::metadata(&path)?.len() <= 4096,
            "invalid SDK destination size"
        );
        let actual: Destination = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            actual == expected,
            "SDK queue belongs to another endpoint or account; use a separate queue"
        );
        return Ok(());
    }
    // Adopt a legacy queue only after checking every retained record, including receipts.
    for entry in std::fs::read_dir(spool)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            let row = read_event(&path)?;
            ensure!(
                row.endpoint == expected.endpoint && row.devspace == expected.devspace,
                "legacy SDK queue contains another destination; nothing reassigned"
            );
        }
    }
    write_once(&path, &serde_json::to_vec(&expected)?)
}

fn read_event(path: &Path) -> Result<Queued> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= MAX_EVENT as u64,
        "invalid or oversized queued event"
    );
    let queued: Queued = serde_json::from_slice(&std::fs::read(path)?)
        .context("damaged queue; nothing discarded")?;
    queued.event.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        path.file_stem().and_then(|s| s.to_str()) == Some(&queued.event.event_id),
        "queue filename mismatch"
    );
    Ok(queued)
}

// Keep only paths and ordering keys in memory, never an entire backlog of payloads.
fn pending_index(spool: &Path) -> Result<Vec<(PathBuf, String, u64)>> {
    let mut index = vec![];
    for entry in std::fs::read_dir(spool)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") && !path.with_extension("acked").exists() {
            let row = read_event(&path)?;
            index.push((path, row.event.capture_id, row.event.sequence));
        }
    }
    index.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
    Ok(index)
}
#[cfg(test)]
fn pending(spool: &Path) -> Result<Vec<(PathBuf, Queued)>> {
    pending_index(spool)?
        .into_iter()
        .map(|(p, _, _)| read_event(&p).map(|q| (p, q)))
        .collect()
}
#[derive(Default, Serialize)]
pub(crate) struct QueueStats {
    pub pending_events: u64,
    pub pending_bytes: u64,
    pub retained_bytes: u64,
}
pub(crate) fn stats(spool: &Path) -> Result<QueueStats> {
    private_dir(spool)?;
    let mut stats = QueueStats::default();
    for entry in std::fs::read_dir(spool)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            let meta = std::fs::symlink_metadata(&path)?;
            ensure!(
                meta.file_type().is_file() && meta.len() <= MAX_EVENT as u64,
                "invalid SDK queue entry"
            );
            stats.retained_bytes += meta.len();
            if !path.with_extension("acked").exists() {
                stats.pending_events += 1;
                stats.pending_bytes += meta.len();
            }
        }
    }
    Ok(stats)
}
fn flush_lock(spool: &Path) -> Result<std::fs::File> {
    let path = spool.join("flush.lock");
    if path.try_exists()? {
        ensure!(
            std::fs::symlink_metadata(&path)?.file_type().is_file(),
            "SDK flush lock must be a regular file"
        );
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    lock.try_lock().context("another flush is running")?;
    Ok(lock)
}
fn deliver(spool: &Path) -> Result<usize> {
    let _lock = flush_lock(spool)?;
    retry::attempt(spool, || {
        let sender = account::sender()?.context("sign in with rroflow login before flushing")?;
        deliver_ready(spool, &sender, usize::MAX, None, None)
    })
}
pub(crate) fn deliver_managed(
    spool: &Path,
    sender: impl FnOnce() -> Result<account::Sender>,
    limit: usize,
    seconds: u64,
    owner: &str,
) -> Result<usize> {
    let _lock = flush_lock(spool)?;
    retry::attempt(spool, || {
        deliver_ready(spool, &sender()?, limit, Some(seconds), Some(owner))
    })
}
fn deliver_ready(
    spool: &Path,
    sender: &account::Sender,
    limit: usize,
    seconds: Option<u64>,
    owner: Option<&str>,
) -> Result<usize> {
    let rows = pending_index(spool)?;
    if rows.is_empty() {
        return Ok(0);
    }
    let base = endpoint(&sender.base)?;
    let client: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .timeout_connect(Some(std::time::Duration::from_secs(5)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut who = client
        .get(&format!("{base}/v1/whoami"))
        .header("Authorization", &format!("Bearer {}", sender.token))
        .call()?;
    retry::require_ok(&who)?;
    let who: Value = who.body_mut().read_json()?;
    ensure!(
        owner.is_none_or(|ds| who["devspace"].as_str() == Some(ds)),
        "SDK source belongs to another account; queue retained"
    );
    // Validate the whole pending backlog before sending any event.
    for (path, _, _) in &rows {
        let q = read_event(path)?;
        ensure!(
            q.endpoint == base && who["devspace"].as_str() == Some(&q.devspace),
            "queue belongs to a different endpoint or devspace; no events uploaded"
        );
    }
    let mut count = 0;
    let started = std::time::Instant::now();
    for (path, _, _) in rows.into_iter().take(limit) {
        if count > 0 && seconds.is_some_and(|n| started.elapsed().as_secs() >= n) {
            break;
        }
        let q = read_event(&path)?;
        ensure!(
            q.endpoint == base && who["devspace"].as_str() == Some(&q.devspace),
            "queue destination changed; record retained"
        );
        let mut response = client
            .post(&format!("{base}/v1/events"))
            .header("Authorization", &format!("Bearer {}", sender.token))
            .send_json(json!({"events":[&q.event]}))?;
        retry::require_ok(&response)?;
        let receipt: Value = response.body_mut().read_json()?;
        ensure!(
            receipt["devspace"] == q.devspace
                && receipt["accepted_ids"] == json!([q.event.event_id]),
            "invalid capture receipt; queue retained"
        );
        write_once(
            &path.with_extension("acked"),
            &serde_json::to_vec(&receipt)?,
        )?;
        count += 1;
    }
    Ok(count)
}

pub(crate) fn run(command: Command) -> Result<()> {
    match command {
        Command::Record {
            spool,
            endpoint,
            devspace,
            content,
        } => record(
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            &spool,
            &endpoint,
            &devspace,
            content,
        ),
        Command::Flush { spool } => {
            private_dir(&spool)?;
            let n = deliver(&spool)?;
            println!("{}", json!({"acknowledged": n}));
            Ok(())
        }
        Command::Status { spool } => {
            private_dir(&spool)?;
            let state = stats(&spool)?;
            println!(
                "{}",
                json!({"pending": state.pending_events, "bytes": state, "retry": retry::read(&spool)?, "note": "queued is not yet server-acknowledged"})
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event() -> Value {
        json!({"schema":"rroflow.capture/v1","event_id":uuid::Uuid::new_v4().to_string(),"capture_id":uuid::Uuid::new_v4().to_string(),"sequence":0,"agent":"grok","session_id":"s","operation_id":"op","parent_operation_id":null,"observed_at":"2026-10-26T00:00:00Z","kind":"stream.event","native_type":"response.created","payload":{"api_key":"private","text":"xai-ABCDEFGHIJKLMNOPQRSTUV"}})
    }
    #[test]
    fn durable_record_replays_without_loss_and_content_is_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        let e = event();
        let line = format!("{e}\n");
        for _ in 0..2 {
            let mut ack = vec![];
            record(
                line.as_bytes(),
                &mut ack,
                dir.path(),
                "https://api.example.test",
                "owner",
                false,
            )
            .unwrap();
            assert!(String::from_utf8(ack).unwrap().contains("\"durable\":true"));
        }
        let rows = pending(dir.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1.event.payload.is_none());
        let mut changed = e;
        changed["kind"] = "call.failed".into();
        assert!(
            record(
                format!("{changed}\n").as_bytes(),
                vec![],
                dir.path(),
                "https://api.example.test",
                "owner",
                false
            )
            .is_err()
        );
    }
    #[test]
    fn content_is_scrubbed_and_malformed_events_never_acknowledge() {
        let dir = tempfile::tempdir().unwrap();
        let e = event();
        record(
            format!("{e}\n").as_bytes(),
            vec![],
            dir.path(),
            "https://api.example.test",
            "owner",
            true,
        )
        .unwrap();
        let raw = serde_json::to_string(&pending(dir.path()).unwrap()[0].1).unwrap();
        assert!(!raw.contains("ABCDEFGHIJKLMNOP"));
        assert!(!raw.contains("private"));
        let mut ack = vec![];
        assert!(
            record(
                b"{bad}\n".as_slice(),
                &mut ack,
                dir.path(),
                "https://api.example.test",
                "owner",
                false
            )
            .is_err()
        );
        assert!(ack.is_empty());
        assert!(endpoint("http://external.example").is_err());
        assert!(endpoint("https://u:p@example.test").is_err());
    }

    #[test]
    fn queue_binding_survives_receipts_and_legacy_adoption_checks_all_records() {
        let dir = tempfile::tempdir().unwrap();
        let e = event();
        record(
            format!("{e}\n").as_bytes(),
            vec![],
            dir.path(),
            "https://api.example.test",
            "owner",
            false,
        )
        .unwrap();
        let receipt = dir
            .path()
            .join(format!("{}.acked", e["event_id"].as_str().unwrap()));
        std::fs::write(receipt, b"{}").unwrap();
        assert_eq!(stats(dir.path()).unwrap().pending_events, 0);
        assert!(bind(dir.path(), "https://api.example.test", "other").is_err());
        assert!(bind(dir.path(), "https://other.example.test", "owner").is_err());
        // Simulate the previous collector's queue without a destination file.
        std::fs::remove_file(dir.path().join("destination.state")).unwrap();
        assert!(bind(dir.path(), "https://api.example.test", "other").is_err());
        assert!(!dir.path().join("destination.state").exists());
        bind(dir.path(), "https://api.example.test", "owner").unwrap();
        assert!(stats(dir.path()).unwrap().retained_bytes > 0);
        let held = flush_lock(dir.path()).unwrap();
        assert!(flush_lock(dir.path()).is_err());
        drop(held);
        assert!(flush_lock(dir.path()).is_ok());
    }
}
