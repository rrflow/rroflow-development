# Raw capture v1

Status: stage 1 (local capture) and the scrubber from stage 2 are implemented in development source
and not released. Consent and stages 3 to 5 are agreed direction under Shippin ADR 0015 and are not
built.

Owner direction, 2026-11-09: capture is raw regardless. Details matter because datasets are
built from them, and a parser that discards a line today can never recover it. Every source
line is kept first. Turns, signals and datasets are derived and can be rebuilt.

## Stages

| Stage | What happens | Status |
|---|---|---|
| 1. Local | Every complete line is kept on the machine, with no account needed | Built |
| 2. Scrub and consent | A stronger scrubber runs before anything leaves; one clear yes while joining | Scrubber built; consent not built |
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
rroflow raw scrub-check  # test the scrubber on kept lines and planted fake secrets; counts only
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

## Scrubber

`crates/rroflow-cli/src/scrub.rs` holds one rule set, named `rroflow.scrub/1`. Change the name
whenever the rules change: each uploaded segment will record the version that scrubbed it, so older
data can be found and scrubbed again. Metadata sent today uses the same rules.

It removes provider token formats (Anthropic, OpenAI, xAI, GitHub, GitLab, Slack, AWS, Google,
Stripe, npm, PyPI, Hugging Face, SendGrid), RROFLOW's own credentials, private keys including ones
cut off by a tool's output limit, JWTs, passwords in URLs, `Authorization` and `Cookie` headers,
fields and `.env` assignments with secret names, password flags on commands, and values of the
secret variables set in the collector's environment or listed in `RROFLOW_REDACT`. Replacements
name the kind, such as `[REDACTED:jwt]`, and never contain a quote or backslash, so a JSON line stays
valid JSON.

`rroflow raw scrub-check` is the leak test. It plants a fresh fake secret into each stored JSON line,
at the start, middle or end of one of its strings, across 41 kinds and contexts. It fails if any
part of a secret survives, a line stops being valid JSON, or a kind was not exercised. It also
counts broad secret-like shapes left in scrubbed text, for review; those do not fail it.

Result on real sessions, 2026-11-10: 1,836,289 secrets planted, 0 survived, 0 lines broken, in 157
seconds. The first run failed with 922 survivors and 10 broken lines, which is how three rule
defects were found and fixed.

Limits:

- It finds known shapes and names. A password in plain prose, or a random string with no prefix and
  no secret-like name beside it, passes through.
- A value containing a quote or backslash is removed only up to that character.
- Runs of 48 or more hex characters are removed, which also removes SHA-256 digests. Git commit IDs
  are shorter and stay.
- Lines that are not valid UTF-8 are not scrubbed, so they must not be uploaded.

## Next: consent

Upload starts only after one explicit yes while joining. It applies to the account and does not
grant dataset or training use, which keep their own permissions.

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
