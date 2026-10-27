# Continuous session collection

Status: implemented on the SDK capture branch; not installed as a startup service
and not deployed to Texas. This is a step toward the setup flow in
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
every sixty passes. Failed delivery backs off to five minutes while new passes
continue capturing locally. A pass uploads at most sixteen batches. Requests have
connection/global timeouts and redirects are disabled. The first backfill and a
large changed file can delay a pass; this is not yet a latency guarantee.

Parsers stream JSON records, including compressed Codex rollouts, instead of
loading the entire transcript into one string. Source collection bounds retained
strings before constructing turns. Existing per-turn text limits still apply.
A partial final JSON line is revisited on a later pass; a malformed complete line
fails visibly. A JSON record over 64 MiB or a turn larger than the upload bound
also fails visibly. Symlinks are reported and not followed. No error is a claim
that the source was completely captured.

Project labels default to working-directory grouping when unspecified. The server
also groups older unassigned records by their recorded directory. Project and
session filters apply before the result limit in both JSONL and Postgres stores.
The dashboard groups sessions and plots UTC daily activity from the returned view;
these are explicitly not complete-history totals or source heartbeat indicators.

Still required before turnkey acceptance: per-agent setup choices; startup/service
installation and upgrades; durable source heartbeats and dashboard queue health;
compaction-safe hooks; incremental cursor performance under sustained load; disk
quotas and retention; live signal-settings refresh; ordered revisions across
multiple collectors; large individual turn handling; hosted Texas deployment.
The signal settings are currently pinned at link time. Grok SDK event capture is
a separate path; subscription ownership alone does not make every Grok surface
observable. Dataset generation and training remain unimplemented.

Tests cover more than twelve files across multiple projects, offline recording,
restart without duplicate revisions, changed turns, lock exclusion, malformed and
partial writes, unavailable delivery, uncertain receipts, account refusal, API
tenant isolation, server-side filters, escaped UI content and logout cleanup.
