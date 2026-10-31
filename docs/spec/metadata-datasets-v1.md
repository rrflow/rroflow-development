# Consented metadata datasets v1 / SDK extension v2

Status: implemented collection workflow, pending release and hosted activation.
This is the first executable part of [automatic datasets v0](automatic-datasets-v0.md).
The full v0 acceptance gates remain: this format is **not qualified training or
evaluation data**, and no training, clustering, holdout selection or promotion runs.

## Member experience

Datasets offers two independently consented purposes per account:

- Signal review examples: immutable reviewed turn metrics plus explicit
  MATCH/NOT MATCH verdicts. Matching duplicate reviews share one row with all
  review references; contradictory verdicts on the same evidence/rule are excluded.
- Unlabelled pattern examples: metrics from turns without a recorded review.
  Rows have no correctness labels. They are preparation for future pattern analysis.

A member chooses agent sources, selected projects or explicitly all current and
future projects, and a retention duration. The permission covers received history
and subsequent eligible records. Saving requires a checked consent control. Ingest
keys cannot authorize collection or download artifacts; these operations require
an accepted Nomen member token. The server derives the account, never a body field.

Changing a policy creates the next consent revision and invalidates artifacts and
pending jobs under the previous revision. **Stop & remove these datasets** withdraws
permission, cancels jobs and removes retained input/artifact bodies. Received turns
and reviews remain intact. Files previously downloaded outside the service cannot
be recalled; stopping collection does not claim to reverse model training.

The member sees the last build check, exclusions, queued/building/ready/failed
states, expiry and a manifest inspector. Downloads require current consent and
unexpired retention on every request. A stopped worker cannot extend download
permission. Controls load only when Datasets opens; the initial dashboard retains
its existing 150 KiB raw / 40 KiB gzip benchmark, with a separate 6 KiB gzip bound
for dataset controls.

## Format and provenance

The database projection selects optional usage counts, termination, compaction,
finished/error/unfinished tool counts and agent identifiers. Unknown usage stays
null. Prompt/response text, tool names/arguments/results, paths, model names and
signal excerpts do not enter job snapshots or exported rows. This does not turn
the source records into anonymous data: the result is private, pseudonymous data.

Source event, review, reviewer and signal references are SHA-256 of the serialized
JSON tuple `[devspace, reference_kind, original_id]`. An authorized owner can join
them to their source inventory; this version does not provide a source-link UI
for every exported reference. Review rows retain the exact source-turn digest.
Pattern rows retain the digest of the selected metadata projection, not a claim
about uncollected prompt content.

Artifacts contain a manifest, rows and an outer SHA-256. The manifest pins purpose,
policy/consent revision, projection version, input digest, row digest, counts,
exclusions, snapshot time, expiry, builder revision and private owner reference.
The artifact and row hashes cover compact UTF-8 JSON with object keys ordered
lexically, no insignificant whitespace and unchanged array order. The input hash
is an internal typed-snapshot identity; use the artifact/rows hashes for export
verification. No artifact includes its own hash recursively.

All examples are marked `qualification: not_evaluated`, with an `unassigned`
partition and `training_authorized: false`. Project/session/fork/near-duplicate
qualification and valid evaluation splits still need implementation. This makes
collection available without misrepresenting its readiness for training.

## Durable execution

The separate worker scans enabled policies. It materializes a bounded metadata
snapshot and queues a job in one transaction. Job identity combines account,
purpose, consent revision and input digest. An unchanged input does not create
another version; mutable turn updates cannot alter an already published artifact.
The selected records themselves form the immutable read stamp. SDK operations use the
separately consented, typed metadata extension below; they are never coerced into turns.

Workers claim jobs using database leases and unique lease tokens. An expired lease
can be recovered up to the configured attempt limit. Publication checks the current
consent revision, expiry, input integrity and lease token. Artifact visibility is
atomic; a stale worker cannot publish after another worker recovers the job or a
member withdraws consent. The validating phase is inside the worker operation,
not a separate durable status. Builds are kept off the HTTP request path.

Projection bounds fail visibly rather than silently sampling a partial dataset.
The scheduler handles one eligible policy per pass and the worker one job per pass;
more than one account may need multiple passes. It records transient source errors
without source contents and checks again. An unchanged expired snapshot is not
silently recreated; a new source snapshot or newly granted consent is needed.

Retention is the shorter of the member's 1–90 day window and the configured count
of recent versions per purpose. Worker passes remove expired artifact/input bodies.
HTTP download expiry is immediate even if cleanup is delayed. Terminal job metadata
has a separate configurable retention; consent revision history remains an audit
journal. Full source deletion, organization RBAC, downstream export deletion and
enterprise audit controls remain separate acceptance work.

## Operator setup

This feature is off unless the hosted config has `[datasets]`; it requires Postgres.
It is currently an operator-enabled member preview, **not a paid-plan entitlement**.
Shippin entitlement synchronization must be connected before selling it as a paid
capability. No prices or billing grants are inferred from this configuration.

```toml
[datasets]
max_rows = 1000
max_snapshot_bytes = 2097152
max_retained_per_purpose = 10
refresh_seconds = 300
poll_seconds = 5
lease_seconds = 60
max_attempts = 3
job_history_days = 90
```

Run the same immutable service artifact with external configuration:

```sh
rroflow-ingest dataset-worker --config /etc/rroflow/ingest.toml
# One scheduling/build/cleanup pass for verification:
rroflow-ingest dataset-worker --config /etc/rroflow/ingest.toml --once
```

The worker needs the existing database credential, no model/provider credential.
`deploy/tx-infra-prod/rroflow-dataset-worker.service` is an installation template;
adapt service paths/identity through normal deployment configuration. It is not
automatically installed or started. Backup/restore and Texas activation remain
required before production. Migration 0007 adds dataset tables without modifying
source records. Rolling back the UI must not erase recorded consent or permit old
workers to bypass a withdrawal.

## Evidence

Rust tests use real PostgreSQL and two accepted fixture member identities to check
consent, scope, deduplication/conflicts, immutable review snapshots, quota bounds,
expired-lease recovery, stale-worker refusal, revocation and expiry. Browser tests
run the actual server and worker binaries against PostgreSQL, inspect/download
the resulting artifact and verify withdrawal. Their bearer identities come from
a disposable introspection issuer; they do not establish live Wisconsin sign-in
or Texas deployment. OAuth/PKCE/session tests remain separate.


## SDK sources: metrics-only/v2

Policies may explicitly select `sdk_agents` (`claude`, `codex`, `grok`). Missing or empty
means no SDK collection: an old permission does not expand after upgrading. Existing
`agents`, `projects` and `all_projects` still govern session turns. SDK sources are a
separate member choice covering their current and future operations; the SDK event model
has no project field, so session project filters cannot truthfully constrain those sources.
SDK-only policies do not need session sources or project selection. Saving or withdrawing
SDK consent uses the same revision, revocation, retention and member authorization gates.

SDK review rows read the immutable snapshot saved with each review, not the latest activity.
They retain the source activity digest and pseudonymous activity, parent, signal, review
and reviewer references. Different evidence revisions remain separate; matching duplicate
reviews combine references, and conflicting verdicts for one activity revision/rule are
excluded. Pattern rows select only fully published current operations without any saved
review for that activity; queued/error/stale projections are excluded. Pattern labels stay
null even if the operation failed or a deterministic finding fired. Nothing treats a
cluster member or observed failure as a supervised label.

Features include the SDK projection version, operation kind/outcome/basis, provider outcome,
optional terminal usage, event and unsupported-event counts, and tool/error counts. They
exclude prompts, responses, tool names, model names, session identifiers, arguments/results,
paths, original event IDs and error messages. References use the same account-scoped tuple
hash with distinct `sdk_activity`, `sdk_signal` and `sdk_review` namespaces. Outcome and
usage semantics remain those of [SDK activity v1](sdk-activity-v1.md).

Explicit SDK consent selects `rroflow.dataset-input/v2` and `metrics-only/v2`. Its rows
carry `source_kind` (`session_turn` or `sdk_operation`) so consumers can handle different
feature types explicitly. Legacy session-only policies retain v1 serialization and inputs.
The scheduler uses a repeatable-read transaction across both sources and applies a combined
row limit and total snapshot byte limit. It refuses oversized inputs without sampling.
Artifacts remain private, unassigned, not evaluated, and not authorized for training.

Migration `0011_sdk_dataset_consent.sql` marks the strict-reader compatibility boundary.
Upgrade workers with the API before enabling SDK policies; old workers cannot deserialize
new permissions or build v2 inputs. Reviewed deployment and recovery remain required.
