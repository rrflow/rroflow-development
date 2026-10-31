//! Durable delivery pacing. Callers hold their journal/flush lock; capture is independent.
use anyhow::{Context, Result, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path, time::UNIX_EPOCH};

const FILE: &str = "delivery-retry.state";
const MAX_UNIX: u64 = 253_402_300_799;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    schema: String,
    pub attempts_since_success: u32,
    pub next_attempt_unix: u64,
    pub http_status: Option<u16>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema: "rroflow.retry/v1".into(),
            attempts_since_success: 0,
            next_attempt_unix: 0,
            http_status: None,
        }
    }
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn read(store: &Path) -> Result<State> {
    let path = store.join(FILE);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(e.into()),
    };
    ensure!(
        meta.file_type().is_file() && meta.len() <= 4096,
        "invalid delivery retry state; queue retained"
    );
    let state: State = serde_json::from_slice(&fs::read(path)?)
        .context("damaged delivery retry state; queue retained")?;
    ensure!(
        state.schema == "rroflow.retry/v1"
            && state.attempts_since_success <= 32
            && state.next_attempt_unix <= MAX_UNIX
            && state.http_status.is_none_or(|s| (100..=599).contains(&s)),
        "invalid delivery retry counters"
    );
    Ok(state)
}

fn write(store: &Path, state: &State) -> Result<()> {
    let path = store.join(FILE);
    let tmp = store.join(format!("delivery-retry-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    let result = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec(state)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &path)?;
        #[cfg(unix)]
        fs::File::open(store)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.context("could not save delivery retry state; queue retained")
}

fn bounds(attempts_since_success: u32) -> (u64, u64) {
    let ceiling = (10u64 << attempts_since_success.saturating_sub(1).min(5)).min(300);
    (ceiling / 2, ceiling)
}

fn retry_after(value: &str, at: u64) -> Option<u64> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value
            .parse::<u64>()
            .ok()
            .map(|s| at.saturating_add(s).min(MAX_UNIX));
    }
    httpdate::parse_http_date(value)
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs().min(MAX_UNIX))
}

#[derive(Debug)]
pub(crate) struct HttpFailure {
    status: u16,
    not_before: Option<u64>,
}
impl std::fmt::Display for HttpFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "delivery HTTP {}; queued records retained", self.status)
    }
}
impl std::error::Error for HttpFailure {}

pub(crate) fn require_ok(response: &ureq::http::Response<ureq::Body>) -> Result<()> {
    if response.status().as_u16() == 200 {
        return Ok(());
    }
    Err(HttpFailure {
        status: response.status().as_u16(),
        not_before: response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| retry_after(v, now())),
    }
    .into())
}

/// Persist an attempt reservation first, so killing/restarting a worker cannot reset pacing.
pub(crate) fn attempt<T>(store: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    attempt_with_clock(store, now, operation)
}

fn attempt_with_clock<T>(
    store: &Path,
    clock: impl Fn() -> u64,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let mut state = read(store)?;
    let at = clock();
    ensure!(
        at >= state.next_attempt_unix,
        "delivery deferred until Unix time {}; local capture remains available",
        state.next_attempt_unix
    );
    state.attempts_since_success = state.attempts_since_success.saturating_add(1).min(32);
    let (low, high) = bounds(state.attempts_since_success);
    let delay = rand::rng().random_range(low..=high);
    state.next_attempt_unix = at.saturating_add(delay).min(MAX_UNIX);
    state.http_status = None;
    write(store, &state)?;
    match operation() {
        Ok(value) => {
            write(store, &State::default())?;
            Ok(value)
        }
        Err(error) => {
            state.next_attempt_unix = clock()
                .saturating_add(delay)
                .min(MAX_UNIX)
                .max(state.next_attempt_unix);
            if let Some(http) = error.downcast_ref::<HttpFailure>() {
                state.http_status = Some(http.status);
                if let Some(until) = http.not_before {
                    state.next_attempt_unix = state.next_attempt_unix.max(until);
                }
            }
            write(store, &state)?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_delays_and_jitter_bounds_never_shorten_server_wait() {
        assert_eq!(retry_after("120", 100), Some(220));
        assert_eq!(
            retry_after("Sun, 06 Nov 1994 08:49:37 GMT", 0),
            Some(784111777)
        );
        assert_eq!(
            retry_after("Sunday, 06-Nov-94 08:49:37 GMT", 0),
            Some(784111777)
        );
        assert_eq!(retry_after("Sun Nov  6 08:49:37 1994", 0), Some(784111777));
        for bad in ["", "-1", "1.5", "soon", "120, 200"] {
            assert_eq!(retry_after(bad, 100), None);
        }
        assert_eq!(retry_after(&u64::MAX.to_string(), 100), Some(MAX_UNIX));
        assert_eq!(bounds(1), (5, 10));
        assert_eq!(bounds(32), (150, 300));
    }
    #[test]
    fn restart_cannot_retry_early_and_success_clears_failure_state() {
        let dir = tempfile::tempdir().unwrap();
        let error = HttpFailure {
            status: 429,
            not_before: Some(500),
        };
        assert!(attempt_with_clock::<()>(dir.path(), || 100, || Err(error.into())).is_err());
        let state = read(dir.path()).unwrap();
        assert_eq!(state.next_attempt_unix, 500);
        assert_eq!(state.http_status, Some(429));
        assert!(
            attempt_with_clock(
                dir.path(),
                || 499,
                || -> Result<()> { panic!("deferred must not send") }
            )
            .is_err()
        );
        assert_eq!(read(dir.path()).unwrap().attempts_since_success, 1);
        assert_eq!(attempt_with_clock(dir.path(), || 500, || Ok(3)).unwrap(), 3);
        assert_eq!(read(dir.path()).unwrap().next_attempt_unix, 0);
    }
    #[test]
    fn reservation_survives_interruption_and_bad_state_fails_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let _ = std::panic::catch_unwind(|| {
            attempt_with_clock::<()>(dir.path(), || 100, || panic!("interrupted request"))
        });
        assert!(read(dir.path()).unwrap().next_attempt_unix > 100);
        assert!(
            attempt_with_clock(
                dir.path(),
                || 100,
                || -> Result<()> { panic!("must not send") }
            )
            .is_err()
        );
        fs::write(dir.path().join(FILE), b"broken").unwrap();
        assert!(
            attempt_with_clock(
                dir.path(),
                || 1000,
                || -> Result<()> { panic!("must not send") }
            )
            .is_err()
        );
    }
}
