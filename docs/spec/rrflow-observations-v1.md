# RRFlow observation correlation

Status: implementation candidate, not deployed or end-to-end qualified.

## Boundary and implementation brief

Starting from `076eb08e815f2d0da27b02e16169e6cf0417c795`, extend the existing
immutable capture-event contract and ingest. Do not create another sink, task
lifecycle, consent mechanism or mutable turn record for engine effects.

Add a bounded, versioned `engine` metadata object for `agent: rrflow` and
`kind: engine.observed`. Bind the reported engine instance, scope and public
operation to the SHA-256 of the observed response bytes. Preserve known task,
attempt, job, plan, source and configuration references; missing identities remain
absent. An attempt requires its task; at least one task or job must be identified.
No response content, provider usage, model identity or success verdict is inferred.

These are collector-reported references, not authenticated engine receipts.
Ingest authenticates the receiving devspace, not the source engine. The eventual
engine exporter must bind actual public reads, authorization and local receipt
bytes before transmission. Ingestion success establishes storage only; it cannot
finish a task, approve training, assert source freshness or qualify a model.

Reuse `/v1/events` identical-replay/conflicting-reuse semantics, tenant isolation
and restart readback. The SDK activity projection must retain an unknown outcome
for an engine observation. Existing SDK dataset source policy does not admit
`rrflow` capture events; ingestion creates no legacy turn. Reviewed engine-outcome
export remains a separate boundary.

## Verification and compatibility

Local verification on 31 October 2026, Windows x64 and Rust 1.97.0:

- `cargo test --workspace --locked`: 136 tests passed, none ignored. The
  `RROFLOW_TEST_DATABASE_URL` environment variable pointed to disposable
  PostgreSQL 17, so both JSONL and PostgreSQL store paths ran. This includes
  authenticated ingest, exact replay, conflicting reuse, tenant isolation,
  restart readback and the absence of fabricated legacy turns.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --all --check`: passed.
- `node --test scripts/test-release-gate.mjs scripts/test-dashboard.mjs`:
  46 checks passed. `node scripts/index-research.mjs --check` passed.
- The temporary databases were removed after testing; no existing service,
  model endpoint, dataset or provider account was changed.

The initial full run exposed a wrong assertion in the new consent test: legacy
turn policies accept configurable agent names, while capture datasets have a
separate SDK allowlist. The test now checks the SDK boundary; the ingest test
also proves no legacy turn is created. No dataset policy was loosened or changed.
The corrected workspace log SHA-256 is
`f015e73fbfdfa7d7e3f21874629b5b846f213b9d3a2f304ffa69fabecb4c4f89`.
Other-platform CI, release artifact qualification and a real engine round trip
remain outstanding. These protocol fixtures are not learning data.

Verify strict identifiers/digests, missing and contradictory correlation, denied
payload/provider fields, old-event round trips, unknown activity outcome, actual
ingest replay/conflict/isolation and reopen on the supported stores. Controlled
fixtures prove protocol behavior only; a real RRFlow observation and configured
sink round trip are required separately before integration is claimed.

The optional field is omitted for older events. Older strict readers reject new
events. A migration marks this serialized-data boundary for the existing release
guard; it rewrites no observations. Source and target readers must be upgraded
together. An older binary is not automatically a safe rollback after new records
are stored. No version bump, release publication or deployment is part of this PR.
