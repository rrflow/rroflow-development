# SDK queues that recover after the app exits

Status: implemented on the managed SDK recovery branch; release and hosted activation pending.

An SDK application writes durable events into its private queue. Explicitly linking
that queue lets the existing per-user collector keep delivering it after the app
exits. The provider's own authentication remains with its SDK. This captures calls
made through RROFLOW adapters; it does not read subscription browser/mobile history.

## Connect once

Sign in using the intended deployment configuration first. On a computer that
already has a linked session collector, add the queue and keep that collector running:

```sh
rroflow sources sdk link --spool /absolute/private/sdk-queue
rroflow sources service install
```

For an SDK-only computer, first run `rroflow sources link --sdk-only`. That mode
never scans Claude or Codex session directories. It uses the same verified account,
source health reports and startup manager. Linking a queue does not install startup.
Use the new collector build; an already pinned older service must be upgraded by
removing its registration and installing the new binary. Removal retains captures.

Configure the SDK `Capture` with this queue, the same explicit endpoint and verified
devspace. `close({flush:false})` finishes local recording; the linked collector sends
the backlog. `close()` can still request an immediate flush. Both upload paths share
the queue lock, exact server receipts and durable Retry-After/backoff state.

```sh
rroflow sources sdk status
rroflow capture status --spool /absolute/private/sdk-queue
rroflow sources sdk unlink --spool /absolute/private/sdk-queue
```

Unlink stops future managed passes. A pass already in progress can finish. It does
not stop the SDK application's local recording, delete records/receipts, change
the queue's owner, or revoke the account. A missing queue can still be unlinked
using its recorded absolute path. Damaged registrations fail visibly.

## Ownership and recovery

The queue has an immutable destination binding. New recording rejects a different
endpoint or account even when all earlier records were acknowledged. A legacy queue
is adopted only after every retained event, including acknowledged events, matches
the requested destination. Mixed legacy queues require explicit operator recovery;
the collector does not reassign them or send a partial subset to another account.

The managed process uses its saved account reference, ignoring ambient temporary
key/URL overrides. Each delivery rechecks the authenticated owner. No token is saved
in queue registrations or manager definitions. An unavailable/mismatched account,
invalid receipt, damaged queue or lost connection leaves the backlog on disk.
A missing queue/destination file is reported, never recreated as an empty success.

SDK queues have their own durable retry state. Restart and manual flush respect it.
Recording in another process remains available during delivery waits. The collector
selects one linked queue per pass in round-robin order; an offline queue cannot
prevent other queues from getting a turn. Registration supports up to 16 queues.

Per-queue work is configurable at link time:

```sh
rroflow sources sdk link --spool /absolute/private/sdk-queue --batch-size 16 --time-budget-secs 2
```

The count limit is 1?256 events; the request-start budget is 1?60 seconds. A request
already in flight can run until its existing 15-second timeout. A budget limits
request starts, not full queue scanning or a hard CPU deadline. Changing a saved
link's limits requires unlink/link. The source scan interval remains configurable
through `sources run --interval-secs`; managed startup uses its documented default.

Pending event indexing retains paths/order keys rather than all event payloads.
Each event is bounded to 4 MiB before reading. The pending backlog is validated
before any upload, preserving the no-partial-cross-owner rule. Large directory
scans still need performance qualification; disk quotas, journal pruning and
lossless disk-full handling remain unfinished. No automatic deletion is introduced.

## What people see

Sources separates queued SDK events/bytes from session turn revisions. It shows
linked queue count, retrying queues and incomplete-count warnings when a queue
cannot be read. Reports contain no local queue path or raw error. CLI status exposes
private paths locally for diagnosis. Counts are last-reported observations, not
continuous liveness or proof that every provider action was captured.

The optional SDK/session-mode fields extend `rroflow.source-health/v1`; old reports
round-trip unchanged. Session-only collectors without SDK links omit the new fields.
Deploy an updated server before enabling SDK links or SDK-only mode: older servers
that reject unknown fields cannot accept the added health fields. Existing event
upload and acknowledgement formats are unchanged. SDK events remain a separate
journal; normalization into turn signals and datasets is separate work.

Migration `0008_sdk_health_contract.sql` annotates the data-format boundary without
rewriting observations. Its fingerprint prevents automatic activation/rollback
between readers with different supported fields. A reviewed backup/restore and
reader-compatibility procedure is required before a production upgrade; do not
bypass that guard or discard source health to make an old binary appear compatible.

## Verification scope

The integration test runs actual SDK app processes that exit, a throttled service,
and a restarted collector. It checks durable retry, independent recording, automatic
readback, account refusal, exact receipts, SDK-only scan exclusion, missing queues
and unlink retention. Native startup fixtures run only on disposable GitHub runners.
Desktop/phone checks inspect SDK recovery state through the real health API.
No test establishes Texas activation, live provider coverage, or personal startup.
