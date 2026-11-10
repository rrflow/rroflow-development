# Raw capture v1

Status: stage 1 (local capture) is implemented in development source and not released.
Stages 2 to 5 are agreed direction under Shippin ADR 0015 and are not built.

Owner direction, 2026-11-09: capture is raw regardless. Details matter because datasets are
built from them, and a parser that discards a line today can never recover it. Every source
line is kept first. Turns, signals and datasets are derived and can be rebuilt.

## Stages

| Stage | What happens | Status |
|---|---|---|
| 1. Local | Every complete line is kept on the machine, with no account needed | Built |
| 2. Scrub and consent | One clear yes while joining; a stronger scrubber runs before anything leaves | Not built |
| 3. Upload | Compressed segments go straight to object storage | Not built |
| 4. Parse | A worker writes events to ClickHouse and derives turns and signals | Not built |
| 5. Projects | Sessions map to a project by git remote, then folder, otherwise General | Not built |

## Local store

`<store>/raw/` is private to the user, inside the collector's source store.

| Path | Purpose |
|---|---|
| `segments/<sequence>.jsonl.zst` | Sealed, write-once records, up to about 8 MiB before compression |
| `cursors.json` | How far each source file has been kept |
| `raw.lock` | One capture at a time |

The first line of a segment is a header with `schema: rroflow.raw-segment/v1`, its sequence and
the collector version. Each later line is one record:

| Field | Meaning |
|---|---|
| `id` | SHA-256 of `file`, `offset` and `sha256` |
| `agent` | `claude` or `codex` |
| `file` | Hash of the agent and the path relative to its root; reveals no local path |
| `offset`, `len` | Byte position in the decoded file, and length without the newline |
| `sha256` | Digest of the exact source bytes |
| `observed_at` | When the collector read it, not when the agent wrote it |
| `line` | The exact source text |
| `line_hex` | Replaces `line` when the bytes are not valid UTF-8 |
| `omitted` | `line_too_long` when the text exceeds 64 MiB; position, length and hash remain |

## Guarantees

- Every terminated line is kept, including blank lines and types no parser understands.
- A line without its newline waits until it is finished.
- A segment is durable before its cursor moves. A crash can write a line twice, always with the
  same `id`, so anything downstream deduplicates on it.
- A rollout compressed to `.jsonl.zst` keeps its identity and adds nothing.
- If a file shrinks or the 256 bytes before the cursor change, it is read again from the start.
  Earlier records stay, unchanged lines keep their identity, and changed lines get a new one.
- A pass reads at most 256 MiB, so a first backfill cannot starve the collector.
- Capture does not depend on signing in, the content setting or upload state.

## Commands

```sh
rroflow raw capture   # keep every line now; needs no account and sends nothing
rroflow raw status    # what is stored, without reading session files
rroflow raw verify    # check every kept line against its source; prints counts only
```

`rroflow sources run` also captures on every pass, offline included.

## Known limits

- Records hold session text exactly as written, including any secrets, as the source files do.
  The store is user-private on Unix and inherits the profile's permissions on Windows.
- An edit earlier in a file than the last 256 bytes is not noticed until `raw verify`.
- A link inside a session folder is counted as `links_skipped` and not followed, so capture cannot
  leave its root.
- Segments use the fastest compression the pure-Rust encoder offers. Better ratios need a different
  encoder, which matters most at the upload stage.
- There is no disk limit yet. A failed write is reported as a capture error, and nothing is deleted.
- Files the agent deleted before capture started cannot be recovered.
- Grok is not read yet, and token-by-token streams exist only through SDK wrappers.

## Next: scrubbing and consent

Upload starts only after one explicit yes while joining. It applies to the account and does not
grant dataset or training use, which keep their own permissions.

Raw lines carry arbitrary tool output, so the current redactor is not enough. Today it misses
RROFLOW's own `rrok_` keys unless they are in the environment, `sk_live_` style keys, JWTs outside
a `Bearer` header, Google API keys, database URLs with passwords, and `.env` style assignments.
Before upload is enabled, a new versioned scrubber must pass a leakage test over real sessions and
seeded secrets. Each uploaded segment records the scrubber version.

## Next: upload

The server issues a short-lived upload grant after checking the account, and records a manifest
when the size and digest match. Segment bytes go directly to S3-compatible storage. The server
does not carry them. The local segment remains until its upload is confirmed.

## Next: parse

A worker reads confirmed segments and writes events to ClickHouse under ADR 0014, keyed by `id`,
sorted by tenant and time, and partitioned monthly. Unknown line types are stored unparsed and
never dropped. Parsers are versioned, and a new version can rebuild from raw.

## Next: projects

A machine connects once to an account. A mapping assigns each session to a project by git remote,
then repository folder, otherwise General. A local path is never a label. Moving work changes only
the mapping and never rewrites a record. The repository and commit must be observed while the work
happens, because they cannot be recovered later for sessions whose logs do not contain them.
