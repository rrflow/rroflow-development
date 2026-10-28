# Continuous session collection

Status: collection and per-user startup commands implemented on development branches;
not activated on the user's machine or deployed to Texas. This is a step toward the setup flow in
[sdk-install-and-linking.md](sdk-install-and-linking.md), not completed turnkey setup.

The one-off preview selected twelve files per agent and a date cutoff. It could
not establish capture coverage. `rroflow sources` instead binds the current user's
Claude Code and Codex session roots to a verified RROFLOW devspace and visits all
session files under those roots, across working directories, with no date cutoff.
Custom roots support alternate agent homes. It does not capture other OS users,
browser-only chats, or activity that a provider never writes to these files.

After signing in to a configured service:

```sh
rroflow login
rroflow sources link
rroflow sources run
```

`sources link --content` opts into scrubbed text at link time; otherwise only metrics
and signal results are retained. `--claude-root`, `--codex-root` and `--store` select
explicit absolute roots and the journal location. An existing binding cannot be
silently changed. `sources status` reports disk state, not process liveness.
`sources run --once --offline` is a network-free capture pass after initial linking.
`sources flush` retries up to sixteen pending batches without reparsing transcripts.

The default store is `sources/` under the existing RROFLOW account configuration
directory. Its binding contains a source UUID, endpoint, devspace, roots, content
choice and a snapshot of signal settings. It contains no provider credentials or
RROFLOW access token. Delivery obtains credentials through the existing account
mechanism, verifies the endpoint and devspace, and refuses account switching.

Each changed turn revision is scrubbed and written to an immutable, numbered batch
using the same fsync/atomic-create helper as the SDK outbox. Only after that write
can upload begin. The collector checks the server's devspace and accepted count
before recording a durable acknowledgement. An uncertain receipt is replayed;
stable event IDs let the server replace the same turn rather than duplicate it.
Later batches cannot pass a failed batch. One process lock protects each journal.
Restart reconstructs the revision index from retained batches, including acknowledged
ones. No acknowledged revision is automatically deleted.

The process scans changes every five seconds by default and rechecks every file
every sixty passes. Failed delivery uses a saved, jittered backoff of up to five
minutes; a server Retry-After can require longer. New passes continue capturing
locally, and restart/manual flush respects the saved wait. Health requests share
the same gate. See [delivery recovery](../ux/delivery-recovery.md).
A pass uploads at most sixteen batches. Requests have
connection/global timeouts and redirects are disabled. The first backfill and a
large changed file can delay a pass; this is not yet a latency guarantee.

Parsers stream JSON records, including compressed Codex rollouts, instead of
loading the entire transcript into one string. Source collection bounds retained
strings before constructing turns. Existing per-turn text limits still apply.
A partial final JSON line is revisited on a later pass; a malformed complete line
fails visibly. A JSON record over 64 MiB or a turn larger than the 4 MiB upload bound
also fails visibly. Symlinks are reported and not followed. No error is a claim
that the source was completely captured.

Project labels default to working-directory grouping when unspecified. The server
also groups older unassigned records by their recorded directory. Project and
session filters apply before the result limit in both JSONL and Postgres stores.
The dashboard groups sessions and plots UTC daily activity from the returned view;
these are explicitly not complete-history totals or source heartbeat indicators.

Still required before turnkey acceptance: per-agent setup choices; released and
activated startup installation;
compaction-safe hooks; incremental cursor performance under sustained load; disk
quotas and retention; ordered revisions across
multiple collectors; large individual turn handling; hosted Texas deployment.
The follow-ups below implement settings refresh and latest source reports. Grok SDK event capture is
a separate path; subscription ownership alone does not make every Grok surface
observable. Dataset generation and training remain unimplemented.

Tests cover more than twelve files across multiple projects, offline recording,
restart without duplicate revisions, changed turns, lock exclusion, malformed and
partial writes, unavailable delivery, uncertain receipts, account refusal, API
tenant isolation, server-side filters, escaped UI content and logout cleanup.


## Signal settings refresh (2026-10-27 follow-up)

The signal-workspace increment refreshes built-in settings after successful delivery,
at most once per minute. It verifies the bound endpoint and devspace, validates the
configuration and writes an immutable local settings revision before using it. On
restart the newest cached revision is used, including offline runs. Older/conflicting
revisions are refused and reported separately from delivery; acknowledged uploads
remain acknowledged. Changed built-in choices invalidate the scan cache so local
rules are recomputed on the next pass. Custom definitions remain cloud-only rules
on uploaded projections; a custom-setting change alone does not clear the scan cache.
The revision cache currently retains all settings revisions. Older collector builds
retain link-time settings and need an upgrade. See [signal workspace](../ux/signal-workspace.md).


## Source health reports

The source-health increment sends bounded latest-only reports with a durable sequence, pending queue counts, retained bytes, scan-error count and last upload result. The Sources page distinguishes recent reports from stale or unavailable status; old activity is never a heartbeat. See [source reporting and recovery](../ux/source-health.md). See [collector startup](../ux/collector-startup.md) for native per-user installation, lifecycle and logout limitations. Hosted activation remains unfinished.
