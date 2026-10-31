# Source reporting and queue health

The Sources page separates received agent history from reports sent by the
computer collecting sessions. A recent turn may be backfill; a quiet agent may
have no new turns. Neither is proof of connection health.

An updated `rroflow sources run` sends a small report after a scan, at most once
every 30 seconds. The page shows three states:

- **Reporting recently:** a report reached this service within two minutes.
- **Reporting with a problem:** a recent report includes capture errors or a
  failed delivery attempt. Check the collector output on that computer.
- **No recent report:** the collector may be stopped, disconnected or busy
  scanning. Displayed counts are its last report, not current queue measurements.

The report shows waiting turn revisions and bytes, last-scan error count, and the
last delivery check result. Queue details include the oldest waiting batch timestamp,
retained journal size, collector version and saved signal-settings revision.
The oldest timestamp comes from the local batch file's modification time; restored
or copied journals may change it. Retained bytes include acknowledged records.
Session counts describe this collector, never browser chats. Updated collectors
also report separately counted [explicitly linked SDK queues](sdk-background-delivery.md),
including SDK-only mode, pending events, retries and incomplete-count warnings.

Refresh fetches new reports. The visible report ages while the page remains open;
it cannot stay marked recent indefinitely. An unavailable health endpoint is shown
separately and does not hide received activity. Older collectors show no reports.
This is diagnostic visibility, not a completeness or uptime guarantee.

## Contract and recovery

Authenticated `POST /v1/sources/health` accepts `rroflow.source-health/v1`.
`GET /v1/sources/health` returns the account's latest source observations, server
time and the two-minute freshness window. Account identity comes from the
credential. The request cannot select a devspace. No prompt, response, local path,
hostname, credential or raw error message is included.

The source binding supplies a stable source ID. Under the existing process lock,
the collector atomically persists a strictly increasing report sequence in
`health.json` before sending. Restart continues that sequence. Unavailable delivery
does not delete queued turns; a failed health request cannot undo acknowledgements.
Offline runs do not access credentials or send reports. A single `--once` run
returns failure if capture, delivery or health reporting fails, with separate local
diagnostics. Continuous runs keep collecting and retrying.

Ingest keeps only the latest report per source. An identical replay preserves the
original receipt time; lower sequences and conflicting reuse receive 409. A new
sequence receives server time after the source write is serialized. Postgres locks
the devspace row; the JSONL store uses its single-process lock and an atomic file
replacement. This version supports at most 128 reporting sources per devspace as a
storage bound, not a billing tier. Source deletion/retirement UI remains future work.

Keep the binding, journal and health state together during recovery. Restoring an
older local health sequence causes refusal instead of falsely refreshing status.
Do not clone a bound source into two running collectors; link a separate source
store for a second machine. Damaged health state fails visibly and never resets
silently. This increment does not install automatic startup, enforce journal
retention/disk quotas, guarantee scan latency, or activate Texas.

Migration `0005_source_health.sql` adds the latest-observation table. Its migration
fingerprint changes the managed-release compatibility boundary. Back up and verify
the migration/restore procedure before activation; do not bypass the release guard
to run an older binary over a different migration fingerprint.
