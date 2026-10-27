//! Durable local SDK outbox. Recording never requires network or provider keys.
use crate::{account, redact::Redactor};
use anyhow::{Context, Result, bail, ensure};
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

fn endpoint(s: &str) -> Result<String> {
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

fn private_dir(path: &Path) -> Result<()> {
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

fn write_once(path: &Path, bytes: &[u8]) -> Result<()> {
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
    private_dir(spool)?;
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

fn pending(spool: &Path) -> Result<Vec<(PathBuf, Queued)>> {
    let mut rows = vec![];
    for entry in std::fs::read_dir(spool)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") && !path.with_extension("acked").exists() {
            ensure!(
                std::fs::symlink_metadata(&path)?.file_type().is_file(),
                "spool entries must be files"
            );
            let bytes = std::fs::read(&path)?;
            ensure!(bytes.len() <= MAX_EVENT, "oversized queued event");
            let queued: Queued =
                serde_json::from_slice(&bytes).context("damaged queue; nothing discarded")?;
            queued.event.validate().map_err(anyhow::Error::msg)?;
            ensure!(
                path.file_stem().and_then(|s| s.to_str()) == Some(&queued.event.event_id),
                "queue filename mismatch"
            );
            rows.push((path, queued));
        }
    }
    rows.sort_by(|a, b| {
        a.1.event
            .capture_id
            .cmp(&b.1.event.capture_id)
            .then(a.1.event.sequence.cmp(&b.1.event.sequence))
    });
    Ok(rows)
}

fn deliver(spool: &Path, sender: &account::Sender) -> Result<usize> {
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(spool.join("flush.lock"))?;
    lock.try_lock().context("another flush is running")?;
    let rows = pending(spool)?;
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
    ensure!(
        who.status().as_u16() == 200,
        "cannot verify capture destination (HTTP {})",
        who.status().as_u16()
    );
    let who: Value = who.body_mut().read_json()?;
    for (_, q) in &rows {
        ensure!(
            q.endpoint == base && who["devspace"].as_str() == Some(&q.devspace),
            "queue belongs to a different endpoint or devspace; no events uploaded"
        );
    }
    let mut count = 0;
    for (path, q) in rows {
        let mut response = client
            .post(&format!("{base}/v1/events"))
            .header("Authorization", &format!("Bearer {}", sender.token))
            .send_json(json!({"events": [&q.event]}))?;
        ensure!(
            response.status().as_u16() == 200,
            "capture upload HTTP {}; queue retained",
            response.status().as_u16()
        );
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
            let Some(sender) = account::sender()? else {
                bail!("sign in with rroflow login before flushing")
            };
            let n = deliver(&spool, &sender)?;
            println!("{}", json!({"acknowledged": n}));
            Ok(())
        }
        Command::Status { spool } => {
            private_dir(&spool)?;
            let rows = pending(&spool)?;
            println!(
                "{}",
                json!({"pending": rows.len(), "note": "queued is not yet server-acknowledged"})
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
}
