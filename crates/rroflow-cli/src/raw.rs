//! Raw-first capture: keep every complete source line before any parsing.
//!
//! Records are immutable and stay on this machine, so a newer parser can rebuild turns and
//! signals from them. This module reads no credentials and uses no network. Records hold
//! session text as the agent wrote it, secrets included, so they live in the private store
//! and must be scrubbed before anything leaves it.

use crate::{account, capture, sources};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const SEGMENT_SCHEMA: &str = "rroflow.raw-segment/v1";
const CURSOR_SCHEMA: &str = "rroflow.raw-cursors/v1";
const SEGMENT_BYTES: usize = 8 * 1024 * 1024;
/// Source bytes read in one pass, so a first backfill cannot starve the collector loop.
pub(crate) const PASS_BYTES: u64 = 256 * 1024 * 1024;
const LINE_LIMIT: usize = 64 * 1024 * 1024;
const READ_BUFFER: usize = 1024 * 1024;
/// Bytes before the cursor that must still match, to notice a rewritten file.
const TAIL_BYTES: usize = 256;

pub(crate) type Roots<'a> = [(&'static str, &'a Path)];

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Keep every line of local agent sessions on this machine. Needs no account; sends nothing.
    Capture {
        #[arg(long)]
        store: Option<PathBuf>,
        /// Claude session root; defaults to CLAUDE_CONFIG_DIR/projects or ~/.claude/projects.
        #[arg(long)]
        claude_root: Option<PathBuf>,
        /// Codex session root; defaults to CODEX_HOME/sessions or ~/.codex/sessions.
        #[arg(long)]
        codex_root: Option<PathBuf>,
    },
    /// Report what is stored, without reading session files.
    Status {
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Re-read session files and check every captured line is stored intact. Prints counts only.
    Verify {
        #[arg(long)]
        store: Option<PathBuf>,
        #[arg(long)]
        claude_root: Option<PathBuf>,
        #[arg(long)]
        codex_root: Option<PathBuf>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
struct Cursor {
    agent: String,
    offset: u64,
    tail_len: u64,
    tail_sha256: String,
    /// Size and change time of each form of the file (plain or zst) when last read to its end.
    stamps: BTreeMap<String, (u64, u64)>,
    rewrites: u64,
}

impl Cursor {
    fn new(agent: &str) -> Self {
        Self {
            agent: agent.into(),
            offset: 0,
            tail_len: 0,
            tail_sha256: sha_hex(b""),
            stamps: BTreeMap::new(),
            rewrites: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Cursors {
    schema: String,
    files: BTreeMap<String, Cursor>,
}

/// One source line. `id` is stable across retries, restarts and machines holding the same file.
#[derive(Serialize, Deserialize)]
struct Record {
    id: String,
    agent: String,
    file: String,
    offset: u64,
    len: u64,
    sha256: String,
    observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line: Option<String>,
    /// Used instead of `line` when the source bytes are not valid UTF-8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_hex: Option<String>,
    /// Why the text is absent. The position, length and hash are still kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    omitted: Option<String>,
}

#[derive(Default)]
struct Pass {
    files_seen: u64,
    files_read: u64,
    lines: u64,
    bytes: u64,
    rewritten: u64,
    partial_files: u64,
    backlog: bool,
    links_skipped: u64,
    errors: Vec<String>,
}

enum Next {
    End,
    Partial,
    Line {
        consumed: u64,
        len: u64,
        sha256: String,
        omitted: bool,
    },
}

struct Store {
    dir: PathBuf,
    cursors: Cursors,
    buffer: Vec<u8>,
    next: u64,
    _lock: fs::File,
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Stable across compression of the file and moves of the root; reveals no local path.
fn file_id(agent: &str, root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).context("file outside its root")?;
    let name = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let name = name.strip_suffix(".zst").unwrap_or(&name);
    Ok(sha_hex(format!("{agent}\0{name}").as_bytes())[..32].to_string())
}

fn record_id(file: &str, offset: u64, sha256: &str) -> String {
    sha_hex(format!("{file}\0{offset}\0{sha256}").as_bytes())
}

fn is_zst(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "zst")
}

/// Opens the decoded stream at `skip`. The flag is false when the stream is shorter than that.
fn open(path: &Path, skip: u64) -> Result<(Box<dyn BufRead>, bool)> {
    let mut file = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
    if is_zst(path) {
        let decoder = ruzstd::decoding::StreamingDecoder::new(BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("decompress {}: {e}", path.display()))?;
        let mut reader: Box<dyn BufRead> = Box::new(BufReader::with_capacity(READ_BUFFER, decoder));
        let skipped = std::io::copy(&mut (&mut reader).take(skip), &mut std::io::sink())?;
        return Ok((reader, skipped == skip));
    }
    file.seek(SeekFrom::Start(skip))?;
    Ok((Box::new(BufReader::with_capacity(READ_BUFFER, file)), true))
}

fn push_tail(tail: &mut Vec<u8>, bytes: &[u8]) {
    if bytes.len() >= TAIL_BYTES {
        tail.clear();
        tail.extend_from_slice(&bytes[bytes.len() - TAIL_BYTES..]);
    } else {
        tail.extend_from_slice(bytes);
        if tail.len() > TAIL_BYTES {
            tail.drain(..tail.len() - TAIL_BYTES);
        }
    }
}

/// Reads one line into `buf`. An unterminated final line is left for a later pass.
fn next_line(reader: &mut dyn BufRead, buf: &mut Vec<u8>, tail: &mut Vec<u8>) -> Result<Next> {
    buf.clear();
    let read = (&mut *reader)
        .take(LINE_LIMIT as u64 + 1)
        .read_until(b'\n', buf)?;
    if read == 0 {
        return Ok(Next::End);
    }
    if buf.last() == Some(&b'\n') {
        push_tail(tail, buf);
        buf.pop();
        return Ok(Next::Line {
            consumed: read as u64,
            len: buf.len() as u64,
            sha256: sha_hex(buf),
            omitted: false,
        });
    }
    if read <= LINE_LIMIT {
        return Ok(Next::Partial);
    }
    // Too long to hold: keep reading only to hash it and find where it ends.
    let mut hasher = Sha256::new();
    hasher.update(&*buf);
    let mut pending = tail.clone();
    push_tail(&mut pending, buf);
    let mut total = read as u64;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(Next::Partial);
        }
        let newline = chunk.iter().position(|b| *b == b'\n');
        let taken = newline.map_or(chunk.len(), |i| i + 1);
        push_tail(&mut pending, &chunk[..taken]);
        hasher.update(&chunk[..newline.unwrap_or(taken)]);
        total += taken as u64;
        reader.consume(taken);
        if newline.is_some() {
            *tail = pending;
            return Ok(Next::Line {
                consumed: total,
                len: total - 1,
                sha256: format!("{:x}", hasher.finalize()),
                omitted: true,
            });
        }
    }
}

/// Finds session files. A link is counted and not followed, so capture cannot leave its root.
fn walk(path: &Path, paths: &mut Vec<PathBuf>, links: &mut u64, errors: &mut Vec<String>) {
    let result = (|| -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            *links += 1;
        } else if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                walk(&entry?.path(), paths, links, errors);
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
        errors.push(format!("{}: {error:#}", path.display()));
    }
}

fn load_cursors(dir: &Path) -> Result<Cursors> {
    let path = dir.join("cursors.json");
    match fs::read(&path) {
        Ok(bytes) => {
            let cursors: Cursors =
                serde_json::from_slice(&bytes).context("damaged raw cursors; nothing discarded")?;
            ensure!(
                cursors.schema == CURSOR_SCHEMA,
                "unknown raw cursor version"
            );
            Ok(cursors)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Cursors {
            schema: CURSOR_SCHEMA.into(),
            files: BTreeMap::new(),
        }),
        Err(e) => Err(e.into()),
    }
}

fn segment_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![];
    match fs::read_dir(dir.join("segments")) {
        Ok(entries) => {
            for entry in entries {
                let path = entry?.path();
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.ends_with(".jsonl.zst"))
                {
                    paths.push(path);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    paths.sort();
    Ok(paths)
}

impl Store {
    fn open(store: &Path) -> Result<Self> {
        let dir = store.join("raw");
        capture::private_dir(&dir)?;
        capture::private_dir(&dir.join("segments"))?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("raw.lock"))?;
        lock.try_lock().context("another raw capture is running")?;
        let mut next = 0;
        if let Some(last) = segment_paths(&dir)?.last() {
            let name = last.file_name().and_then(|s| s.to_str()).unwrap_or("");
            next = name
                .split('.')
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .context("unrecognized raw segment name; nothing discarded")?
                + 1;
        }
        Ok(Self {
            cursors: load_cursors(&dir)?,
            dir,
            buffer: Vec::new(),
            next,
            _lock: lock,
        })
    }

    /// Writes buffered records. A segment is durable before any cursor can move past it.
    fn seal(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let mut plain = serde_json::to_vec(&json!({
            "schema": SEGMENT_SCHEMA,
            "sequence": self.next,
            "collector_version": env!("CARGO_PKG_VERSION"),
        }))?;
        plain.push(b'\n');
        plain.extend_from_slice(&self.buffer);
        let packed = ruzstd::encoding::compress_to_vec(
            plain.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        capture::write_once(
            &self
                .dir
                .join("segments")
                .join(format!("{:020}.jsonl.zst", self.next)),
            &packed,
        )?;
        self.next += 1;
        self.buffer.clear();
        Ok(())
    }

    fn save_cursors(&self) -> Result<()> {
        let path = self.dir.join("cursors.json");
        let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        file.write_all(&serde_json::to_vec(&self.cursors)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(tmp, path)?;
        #[cfg(unix)]
        fs::File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    fn checkpoint(&mut self, id: &str, tail: &[u8]) -> Result<()> {
        let cursor = self.cursors.files.get_mut(id).context("cursor")?;
        cursor.tail_len = tail.len() as u64;
        cursor.tail_sha256 = sha_hex(tail);
        self.seal()?;
        self.save_cursors()
    }

    /// Positions a reader after what is already kept, or at the start if the file changed.
    fn resume(
        &mut self,
        id: &str,
        path: &Path,
        pass: &mut Pass,
    ) -> Result<(Box<dyn BufRead>, Vec<u8>)> {
        let cursor = self.cursors.files.get_mut(id).context("cursor")?;
        if cursor.offset > 0 {
            let (mut reader, complete) = open(path, cursor.offset - cursor.tail_len)?;
            let mut held = vec![0; usize::try_from(cursor.tail_len)?];
            let intact = complete
                && match reader.read_exact(&mut held) {
                    Ok(()) => sha_hex(&held) == cursor.tail_sha256,
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => false,
                    Err(e) => return Err(e.into()),
                };
            if intact {
                return Ok((reader, held));
            }
            // Earlier records stay. Unchanged lines keep their identity, so nothing doubles.
            cursor.offset = 0;
            cursor.tail_len = 0;
            cursor.tail_sha256 = sha_hex(b"");
            cursor.rewrites += 1;
            pass.rewritten += 1;
        }
        Ok((open(path, 0)?.0, Vec::new()))
    }

    fn capture_file(
        &mut self,
        agent: &str,
        root: &Path,
        path: &Path,
        budget: u64,
        pass: &mut Pass,
    ) -> Result<()> {
        let id = file_id(agent, root, path)?;
        let metadata = fs::metadata(path)?;
        let changed = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_millis();
        let stamp = (metadata.len(), u64::try_from(changed)?);
        let form = if is_zst(path) { "zst" } else { "plain" };
        let cursor = self
            .cursors
            .files
            .entry(id.clone())
            .or_insert_with(|| Cursor::new(agent));
        if cursor.stamps.get(form) == Some(&stamp) {
            return Ok(());
        }
        let (mut reader, mut tail) = self.resume(&id, path, pass)?;
        pass.files_read += 1;
        let observed_at = chrono::Utc::now().to_rfc3339();
        let mut buf = Vec::new();
        let mut finished = false;
        while pass.bytes < budget {
            let (consumed, len, sha256, omitted) =
                match next_line(reader.as_mut(), &mut buf, &mut tail)? {
                    Next::End => {
                        finished = true;
                        break;
                    }
                    Next::Partial => {
                        pass.partial_files += 1;
                        finished = true;
                        break;
                    }
                    Next::Line {
                        consumed,
                        len,
                        sha256,
                        omitted,
                    } => (consumed, len, sha256, omitted),
                };
            let cursor = self.cursors.files.get_mut(&id).context("cursor")?;
            let offset = cursor.offset;
            let (line, line_hex) = if omitted {
                (None, None)
            } else {
                match std::str::from_utf8(&buf) {
                    Ok(text) => (Some(text.to_string()), None),
                    Err(_) => (None, Some(to_hex(&buf))),
                }
            };
            let record = Record {
                id: record_id(&id, offset, &sha256),
                agent: agent.into(),
                file: id.clone(),
                offset,
                len,
                sha256,
                observed_at: observed_at.clone(),
                line,
                line_hex,
                omitted: omitted.then(|| "line_too_long".to_string()),
            };
            serde_json::to_writer(&mut self.buffer, &record)?;
            self.buffer.push(b'\n');
            cursor.offset += consumed;
            pass.lines += 1;
            pass.bytes += consumed;
            if self.buffer.len() >= SEGMENT_BYTES {
                self.checkpoint(&id, &tail)?;
            }
        }
        if finished {
            let cursor = self.cursors.files.get_mut(&id).context("cursor")?;
            cursor.stamps.insert(form.into(), stamp);
        } else {
            pass.backlog = true;
        }
        self.checkpoint(&id, &tail)
    }
}

/// Keeps every new complete line under the given roots. Independent of account and consent.
pub(crate) fn capture_pass(store: &Path, roots: &Roots, budget: u64) -> Result<Value> {
    let mut raw = Store::open(store)?;
    let mut pass = Pass::default();
    for (agent, root) in roots {
        if !root.exists() {
            continue;
        }
        let mut paths = vec![];
        walk(root, &mut paths, &mut pass.links_skipped, &mut pass.errors);
        paths.sort();
        for path in paths {
            pass.files_seen += 1;
            if pass.bytes >= budget {
                pass.backlog = true;
                continue;
            }
            if let Err(error) = raw.capture_file(agent, root, &path, budget, &mut pass) {
                pass.errors.push(format!("{}: {error:#}", path.display()));
            }
        }
    }
    raw.seal()?;
    raw.save_cursors()?;
    Ok(json!({
        "files_seen": pass.files_seen,
        "files_read": pass.files_read,
        "lines": pass.lines,
        "source_bytes": pass.bytes,
        "rewritten_files": pass.rewritten,
        "files_with_unfinished_line": pass.partial_files,
        "backlog": pass.backlog,
        "links_skipped": pass.links_skipped,
        "errors": pass.errors,
    }))
}

/// Disk state only. Holds no local path or session text.
pub(crate) fn status(store: &Path) -> Result<Value> {
    let dir = store.join("raw");
    let segments = segment_paths(&dir)?;
    let mut stored_bytes = 0;
    for path in &segments {
        stored_bytes += fs::metadata(path)?.len();
    }
    let cursors = load_cursors(&dir)?;
    Ok(json!({
        "segments": segments.len(),
        "stored_bytes": stored_bytes,
        "files": cursors.files.len(),
        "captured_source_bytes": cursors.files.values().map(|c| c.offset).sum::<u64>(),
        "rewrites": cursors.files.values().map(|c| c.rewrites).sum::<u64>(),
    }))
}

fn short(hex: &str) -> Result<u128> {
    ensure!(hex.len() >= 32, "malformed raw digest");
    Ok(u128::from_str_radix(&hex[..32], 16)?)
}

/// Checks stored records against themselves and against the session files. Counts only.
pub(crate) fn verify(store: &Path, roots: &Roots) -> Result<Value> {
    let dir = store.join("raw");
    let cursors = load_cursors(&dir)?;
    let mut stored: HashSet<(u128, u64, u128)> = HashSet::new();
    let (mut stored_lines, mut damaged, mut duplicates, mut omitted) = (0u64, 0u64, 0u64, 0u64);
    let segments = segment_paths(&dir)?;
    for path in &segments {
        let file = fs::File::open(path)?;
        let decoder = ruzstd::decoding::StreamingDecoder::new(BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("decompress {}: {e}", path.display()))?;
        let mut reader = BufReader::with_capacity(READ_BUFFER, decoder);
        let mut bytes = Vec::new();
        reader.read_until(b'\n', &mut bytes)?;
        let header: Value = serde_json::from_slice(&bytes).context("segment header")?;
        ensure!(
            header["schema"] == SEGMENT_SCHEMA,
            "unknown raw segment version"
        );
        loop {
            bytes.clear();
            if reader.read_until(b'\n', &mut bytes)? == 0 {
                break;
            }
            stored_lines += 1;
            let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
                damaged += 1;
                continue;
            };
            let content = match (&record.line, &record.line_hex) {
                (Some(text), None) => Some(text.as_bytes().to_vec()),
                (None, Some(hex)) => (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
                    .collect(),
                _ => None,
            };
            match content {
                Some(content)
                    if sha_hex(&content) == record.sha256 && content.len() as u64 == record.len => {
                }
                None if record.omitted.is_some() => omitted += 1,
                _ => {
                    damaged += 1;
                    continue;
                }
            }
            if record.id != record_id(&record.file, record.offset, &record.sha256) {
                damaged += 1;
                continue;
            }
            if !stored.insert((short(&record.file)?, record.offset, short(&record.sha256)?)) {
                duplicates += 1;
            }
        }
    }
    let (mut source_files, mut source_lines, mut missing, mut later, mut unfinished) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut errors, mut links) = (0u64, 0u64);
    for (agent, root) in roots {
        if !root.exists() {
            continue;
        }
        let mut paths = vec![];
        let mut failures = vec![];
        walk(root, &mut paths, &mut links, &mut failures);
        errors += failures.len() as u64;
        paths.sort();
        for path in paths {
            source_files += 1;
            let id = file_id(agent, root, &path)?;
            let kept = cursors.files.get(&id).map_or(0, |c| c.offset);
            let file = short(&id)?;
            let (mut reader, _) = open(&path, 0)?;
            let (mut buf, mut tail) = (Vec::new(), Vec::new());
            let mut offset = 0;
            loop {
                match next_line(reader.as_mut(), &mut buf, &mut tail)? {
                    Next::End => break,
                    Next::Partial => {
                        unfinished += 1;
                        break;
                    }
                    Next::Line {
                        consumed, sha256, ..
                    } => {
                        if offset >= kept {
                            later += 1;
                        } else {
                            source_lines += 1;
                            if !stored.contains(&(file, offset, short(&sha256)?)) {
                                missing += 1;
                            }
                        }
                        offset += consumed;
                    }
                }
            }
        }
    }
    Ok(json!({
        "ok": missing == 0 && damaged == 0 && errors == 0,
        "segments": segments.len(),
        "stored_lines": stored_lines,
        "distinct_lines": stored.len(),
        "duplicates": duplicates,
        "omitted_text_lines": omitted,
        "damaged": damaged,
        "source_files": source_files,
        "source_lines_checked": source_lines,
        "missing": missing,
        "lines_not_captured_yet": later,
        "files_with_unfinished_line": unfinished,
        "links_skipped": links,
        "scan_errors": errors,
    }))
}

fn store_dir(store: Option<PathBuf>) -> Result<PathBuf> {
    store
        .or_else(|| account::config_dir().map(|p| p.join("sources")))
        .context("raw store unavailable")
}

fn roots(claude: Option<PathBuf>, codex: Option<PathBuf>) -> Result<(PathBuf, PathBuf)> {
    Ok((
        claude
            .map(Ok)
            .unwrap_or_else(|| sources::default_root("CLAUDE_CONFIG_DIR", ".claude", "projects"))?,
        codex
            .map(Ok)
            .unwrap_or_else(|| sources::default_root("CODEX_HOME", ".codex", "sessions"))?,
    ))
}

pub(crate) fn run(command: Command) -> Result<()> {
    match command {
        Command::Capture {
            store,
            claude_root,
            codex_root,
        } => {
            let store = store_dir(store)?;
            let (claude, codex) = roots(claude_root, codex_root)?;
            let paths = [("claude", claude.as_path()), ("codex", codex.as_path())];
            let (mut lines, mut bytes, mut errors) = (0, 0, vec![]);
            loop {
                let pass = capture_pass(&store, &paths, PASS_BYTES)?;
                lines += pass["lines"].as_u64().context("lines")?;
                bytes += pass["source_bytes"].as_u64().context("bytes")?;
                errors.extend(pass["errors"].as_array().context("errors")?.iter().cloned());
                if pass["backlog"] != true {
                    break;
                }
            }
            println!(
                "{}",
                json!({"lines":lines,"source_bytes":bytes,"errors":errors,"stored":status(&store)?})
            );
            ensure!(errors.is_empty(), "some session files could not be kept");
        }
        Command::Status { store } => println!("{}", status(&store_dir(store)?)?),
        Command::Verify {
            store,
            claude_root,
            codex_root,
        } => {
            let store = store_dir(store)?;
            let (claude, codex) = roots(claude_root, codex_root)?;
            let report = verify(
                &store,
                &[("claude", claude.as_path()), ("codex", codex.as_path())],
            )?;
            println!("{report}");
            ensure!(report["ok"] == true, "raw capture is incomplete or damaged");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        store: tempfile::TempDir,
        root: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                store: tempfile::tempdir().unwrap(),
                root: tempfile::tempdir().unwrap(),
            }
        }
        fn path(&self, name: &str) -> PathBuf {
            let path = self.root.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            path
        }
        fn roots(&self) -> [(&'static str, &Path); 1] {
            [("claude", self.root.path())]
        }
        fn capture(&self) -> Value {
            let pass = capture_pass(self.store.path(), &self.roots(), PASS_BYTES).unwrap();
            assert_eq!(pass["errors"], json!([]), "{pass}");
            pass
        }
        fn verify(&self) -> Value {
            verify(self.store.path(), &self.roots()).unwrap()
        }
        fn records(&self) -> Vec<Record> {
            let mut out = vec![];
            for path in segment_paths(&self.store.path().join("raw")).unwrap() {
                let decoder = ruzstd::decoding::StreamingDecoder::new(BufReader::new(
                    fs::File::open(path).unwrap(),
                ))
                .unwrap();
                for line in BufReader::new(decoder).lines().skip(1) {
                    out.push(serde_json::from_str(&line.unwrap()).unwrap());
                }
            }
            out
        }
    }

    fn append(path: &Path, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn keeps_every_complete_line_including_unknown_types_blank_and_non_utf8() {
        let f = Fixture::new();
        let path = f.path("project/session.jsonl");
        let mut body = b"{\"type\":\"never-seen-before\",\"x\":1}\n\n".to_vec();
        body.extend_from_slice(&[0xff, 0xfe, b'\n']);
        body.extend_from_slice(b"{\"crlf\":true}\r\n");
        fs::write(&path, &body).unwrap();

        assert_eq!(f.capture()["lines"], 4);
        let records = f.records();
        assert_eq!(
            records[0].line.as_deref(),
            Some("{\"type\":\"never-seen-before\",\"x\":1}")
        );
        assert_eq!(records[1].line.as_deref(), Some(""));
        assert_eq!(records[2].line_hex.as_deref(), Some("fffe"));
        assert_eq!(
            records[3].line.as_deref(),
            Some("{\"crlf\":true}\r"),
            "the source bytes are kept exactly"
        );
        assert_eq!(records[1].offset, 35);
        let report = f.verify();
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["source_lines_checked"], 4);
    }

    #[test]
    fn an_unfinished_line_waits_and_appends_add_only_new_lines() {
        let f = Fixture::new();
        let path = f.path("a/s.jsonl");
        fs::write(&path, b"{\"n\":1}\n{\"n\":2").unwrap();
        let first = f.capture();
        assert_eq!(first["lines"], 1);
        assert_eq!(first["files_with_unfinished_line"], 1);

        assert_eq!(f.capture()["lines"], 0, "an unchanged file is not reread");
        assert_eq!(f.capture()["files_read"], 0);

        append(&path, b"}\n{\"n\":3}\n");
        assert_eq!(f.capture()["lines"], 2);
        let records = f.records();
        assert_eq!(
            records.iter().map(|r| r.offset).collect::<Vec<_>>(),
            vec![0, 8, 16]
        );
        assert_eq!(records[1].line.as_deref(), Some("{\"n\":2}"));
        assert_eq!(f.verify()["ok"], true);
    }

    #[test]
    fn identity_is_stable_and_a_lost_cursor_cannot_change_what_a_line_is() {
        let f = Fixture::new();
        let path = f.path("a/s.jsonl");
        fs::write(&path, b"{\"n\":1}\n{\"n\":2}\n").unwrap();
        f.capture();
        let before: Vec<String> = f.records().into_iter().map(|r| r.id).collect();

        // A crash after the segment is durable but before the cursor is written.
        fs::remove_file(f.store.path().join("raw/cursors.json")).unwrap();
        f.capture();
        let after: Vec<String> = f.records().into_iter().map(|r| r.id).collect();
        assert_eq!(after.len(), 4, "the replay wrote the lines again");
        assert_eq!(&after[2..], &before[..], "with the same identities");

        let report = f.verify();
        assert_eq!(report["ok"], true);
        assert_eq!(report["distinct_lines"], 2);
        assert_eq!(report["duplicates"], 2);
    }

    #[test]
    fn compressing_a_finished_session_adds_nothing() {
        let f = Fixture::new();
        let plain = f.path("2026/rollout.jsonl");
        let body = b"{\"n\":1}\n{\"n\":2}\n";
        fs::write(&plain, body).unwrap();
        f.capture();

        let packed = ruzstd::encoding::compress_to_vec(
            body.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        fs::write(plain.with_extension("jsonl.zst"), packed).unwrap();
        fs::remove_file(&plain).unwrap();

        assert_eq!(f.capture()["lines"], 0);
        assert_eq!(f.capture()["files_read"], 0, "the zst stamp is remembered");
        let report = f.verify();
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["source_lines_checked"], 2);
    }

    #[test]
    fn a_rewritten_file_is_read_again_and_unchanged_lines_keep_their_identity() {
        let f = Fixture::new();
        let path = f.path("a/s.jsonl");
        fs::write(&path, b"{\"n\":1}\n{\"n\":2}\n").unwrap();
        f.capture();

        // A different size, so the change is seen on filesystems with coarse timestamps.
        fs::write(&path, b"{\"n\":1}\n{\"n\":99}\n").unwrap();
        let pass = f.capture();
        assert_eq!(pass["rewritten_files"], 1);
        assert_eq!(pass["lines"], 2);

        let records = f.records();
        assert_eq!(records[0].id, records[2].id, "the unchanged line");
        assert_ne!(records[1].id, records[3].id, "the changed line");
        assert_eq!(
            records[1].line.as_deref(),
            Some("{\"n\":2}"),
            "history stays"
        );
        let report = f.verify();
        assert_eq!(report["ok"], true);
        assert_eq!(report["distinct_lines"], 3);
    }

    #[test]
    fn a_shrunken_file_is_noticed() {
        let f = Fixture::new();
        let path = f.path("a/s.jsonl");
        fs::write(&path, b"{\"n\":1}\n{\"n\":2}\n").unwrap();
        f.capture();
        fs::write(&path, b"{\"n\":1}\n").unwrap();
        let pass = f.capture();
        assert_eq!(pass["rewritten_files"], 1);
        assert_eq!(f.verify()["ok"], true);
    }

    #[test]
    fn a_budget_leaves_a_backlog_that_later_passes_finish() {
        let f = Fixture::new();
        fs::write(f.path("a/one.jsonl"), b"{\"n\":1}\n{\"n\":2}\n").unwrap();
        fs::write(f.path("b/two.jsonl"), b"{\"n\":3}\n").unwrap();

        let first = capture_pass(f.store.path(), &f.roots(), 1).unwrap();
        assert_eq!(first["lines"], 1);
        assert_eq!(first["backlog"], true);
        let report = f.verify();
        assert_eq!(report["ok"], true, "what was kept is intact");
        assert_eq!(report["lines_not_captured_yet"], 2);

        let mut passes = 0;
        while capture_pass(f.store.path(), &f.roots(), 1).unwrap()["backlog"] == true {
            passes += 1;
            assert!(passes < 10, "backlog never finished");
        }
        let report = f.verify();
        assert_eq!(report["source_lines_checked"], 3, "{report}");
        assert_eq!(report["lines_not_captured_yet"], 0);
    }

    #[test]
    fn a_line_over_the_limit_keeps_its_position_and_hash_without_its_text() {
        let f = Fixture::new();
        let path = f.path("a/s.jsonl");
        let mut body = vec![b'x'; LINE_LIMIT + 5];
        body.push(b'\n');
        body.extend_from_slice(b"{\"after\":true}\n");
        fs::write(&path, &body).unwrap();

        assert_eq!(f.capture()["lines"], 2);
        let records = f.records();
        assert_eq!(records[0].omitted.as_deref(), Some("line_too_long"));
        assert!(records[0].line.is_none());
        assert_eq!(records[0].len, LINE_LIMIT as u64 + 5);
        assert_eq!(records[0].sha256, sha_hex(&vec![b'x'; LINE_LIMIT + 5]));
        assert_eq!(records[1].offset, LINE_LIMIT as u64 + 6);
        let report = f.verify();
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["omitted_text_lines"], 1);
    }

    #[test]
    fn verify_reports_a_lost_segment() {
        let f = Fixture::new();
        fs::write(f.path("a/s.jsonl"), b"{\"n\":1}\n").unwrap();
        f.capture();
        for path in segment_paths(&f.store.path().join("raw")).unwrap() {
            fs::remove_file(path).unwrap();
        }
        let report = f.verify();
        assert_eq!(report["ok"], false);
        assert_eq!(report["missing"], 1);
    }

    #[test]
    fn verify_reports_a_damaged_record() {
        let f = Fixture::new();
        fs::write(f.path("a/s.jsonl"), b"{\"n\":1}\n").unwrap();
        f.capture();
        let dir = f.store.path().join("raw");
        let path = segment_paths(&dir).unwrap().remove(0);
        let mut text = String::new();
        ruzstd::decoding::StreamingDecoder::new(BufReader::new(fs::File::open(&path).unwrap()))
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let altered = text.replace("{\\\"n\\\":1}", "{\\\"n\\\":7}");
        assert_ne!(altered, text);
        fs::write(
            &path,
            ruzstd::encoding::compress_to_vec(
                altered.as_bytes(),
                ruzstd::encoding::CompressionLevel::Fastest,
            ),
        )
        .unwrap();
        let report = f.verify();
        assert_eq!(report["ok"], false);
        assert_eq!(report["damaged"], 1);
    }

    #[test]
    fn large_captures_seal_segments_and_status_reveals_no_path_or_text() {
        let f = Fixture::new();
        let line = format!(
            "{{\"secret-looking-text\":\"{}\"}}\n",
            "y".repeat(1024 * 1024)
        );
        fs::write(f.path("private-project/s.jsonl"), line.repeat(20)).unwrap();
        f.capture();
        let state = status(f.store.path()).unwrap();
        assert!(state["segments"].as_u64().unwrap() >= 2, "{state}");
        assert!(
            state["stored_bytes"].as_u64().unwrap()
                < state["captured_source_bytes"].as_u64().unwrap()
        );
        let text = state.to_string();
        assert!(!text.contains("private-project") && !text.contains("secret-looking-text"));
        assert!(!f.records()[0].file.contains("private-project"));
        assert_eq!(f.verify()["source_lines_checked"], 20);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_counted_and_not_followed_or_treated_as_a_failure() {
        let f = Fixture::new();
        fs::write(f.path("a/s.jsonl"), b"{\"n\":1}\n").unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("other.jsonl"), b"{\"n\":2}\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), f.root.path().join("a/linked")).unwrap();

        let pass = f.capture();
        assert_eq!(pass["lines"], 1);
        assert_eq!(pass["links_skipped"], 1);
        let report = f.verify();
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["links_skipped"], 1);
    }

    #[test]
    fn a_missing_root_is_not_an_error() {
        let f = Fixture::new();
        let gone = f.root.path().join("not-installed");
        let pass = capture_pass(f.store.path(), &[("codex", gone.as_path())], PASS_BYTES).unwrap();
        assert_eq!(pass["errors"], json!([]));
        assert_eq!(pass["lines"], 0);
    }
}
