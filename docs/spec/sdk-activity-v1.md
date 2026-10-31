# SDK activity and evidence v1

Status: implemented on `codex/sdk-monitoring`: core projector, structural facts, hosted
PostgreSQL work/indexes, member dashboard and immutable SDK reviews. Verification and
release receipts are recorded in the owning execution plan; this is not a deployment claim.

## What an activity means

An SDK activity is a recorded operation, identified by capture ID plus operation ID.
It is not a guessed human turn, model request, subscription charge or provider-internal
span. Calls and tools can have parent operation links. Missing ancestors are reported;
cycles and inconsistent operation identities are refused. Identity is a SHA-256 digest
of the serialized identity tuple, scoped by the authenticated account in storage.

The `rroflow.sdk-activity/v1` projection includes its version, source event references,
parent link, known models, observation times, outcome basis, tool observations, findings,
unsupported event IDs and coverage notes. Observation times are not provider wall-clock
execution durations. Native event records remain immutable and independently queryable.
Projection revisions hash their complete serialized metadata, including provenance.
No prompt, command, tool argument/result or error message is copied into the projection.

## Facts without content

`rroflow.capture/v1` gains optional `native` facts with an explicit version of 1:
item type, nested event type, response object type, stop reason, error flag and interrupt
flag. Strings use the existing bounded identifier validation. Credential-shaped metadata
is refused by the collector. These fields are structural observations, not a text-upload
permission. Missing facts stay unknown; the projector does not inspect private payloads
to infer missing structural fields. Codex MCP names use the native `item.tool` field.

Older events omit `native` when reserialized. New readers accept old events, but older
strict readers cannot accept new facts. Migration `0009_sdk_native_facts.sql` records this
reader boundary for the existing release guard without deleting or rewriting observations.
Upgrade collector and ingest readers before using the new SDK. A changed migration
fingerprint requires reviewed activation/restore; an old binary is not a safe rollback
once new-format records exist. Deployment and recovery proof remain separate gates.

## Outcomes and usage

The projection distinguishes completed, failed, cancelled, partial and unknown. Its
basis says whether the observation was a provider terminal, wrapper return, interruption,
or absence/ambiguity of an end. An exhausted iterator alone never establishes provider
success. A later wrapper interruption can coexist with an observed provider completion;
both are retained without attributing cancellation to the user.

One unambiguous provider-terminal usage snapshot is retained with its event ID. A generic
call return can provide a wrapper-scoped snapshot when no provider terminal exists.
Multiple distinct provider terminals leave aggregate usage unknown, even if counts happen
to match. Stream deltas and repeated snapshots are never added together. Zero remains zero;
missing remains missing. Nothing here computes price or subscription consumption.

Completed native items can contain failed tools. Tool lifecycle conflicts remain unknown,
while explicit failure observations remain findings with their original event IDs.
Unsupported events remain listed and count toward coverage warnings. Received events do
not prove complete provider coverage; absence of findings is not a health guarantee.

Initial fixed, versioned observations are `sdk_call_failed`, `sdk_tool_failed`,
`sdk_interrupted` and `sdk_reported_error`. They report observable outcomes, not a judged
root cause. A non-terminal error item does not establish failure of the whole call.
Future configurable signal policies must preserve the definition and evidence revisions.

## Durable hosted storage and publication

Migration `0010_sdk_activities.sql` queues existing captures without changing raw events.
Newly inserted events update a capture's source revision in the same transaction; exact
replay does not dirty it or consume another allowance. Derived activities do not consume
turn allowance. All work, activities and reviews are scoped by authenticated devspace.

Workers claim durable leases with `SKIP LOCKED`, read one complete received capture in a
repeatable-read snapshot, strip private payloads in SQL, and atomically publish a complete
set of operation projections. An expired or replaced lease cannot publish. New arrivals
while processing preserve the active lease: the verified older snapshot can publish, with
`new_events_pending`, and the next source revision stays queued. This avoids starving a
busy source while making staleness explicit. Crash recovery and capped retry exhaustion
are visible. A failed/capacity-limited job keeps raw events and its last published result.

`serve` starts the PostgreSQL worker by default. `[sdk_activity]` can set:

```toml
embedded_worker = true
max_events_per_capture = 20000
max_snapshot_bytes = 16777216
poll_seconds = 2
lease_seconds = 60
max_attempts = 3
retry_seconds = 30
```

For dedicated workers set `embedded_worker = false` and run
`rroflow-ingest activity-worker --config <service.toml>`. `--once` processes at most one
claim. `--retry-errors` resets failed work after the operator fixes its source/configuration.
The CLI reads the same database configuration; it does not take an account key. Each
snapshot is bounded before allocation, projection output is byte-bounded, and database
statements have a ten-second timeout. These limits are explicit capacity controls, not
production load qualification. Failed jobs and pending captures are workspace-wide counts.

## Monitoring and immutable reviews

`GET /v1/sdk/activity` uses one published cohort for list/count filters: agent, session,
outcome, signal, literal metadata search, observation interval, exact ID, and has-findings.
Keyset pages use observation time plus activity ID and return at most 100 compact rows.
The same repeatable-read transaction supplies counts and pending/error coverage. Detailed
provenance is fetched only for an exact ID. `/v1/events?event_id=...` resolves the original
source record within the same account, without relying on the latest journal page.

Activity and Findings have an SDK operations view alongside Session turns. It explains
outcomes, usage unknowns, parent links, unsupported events and waiting updates. Lists are
compact; the same-origin UI module loads on demand. Event lists expand in batches of 50.
Recorded content is escaped text, never HTML. Sign-out and route changes invalidate late
responses. The JSONL/local service explicitly reports SDK indexing as unavailable; its raw
events remain in Sources. Standalone local indexing is separate future work.

`POST /v1/sdk/reviews` pins activity revision, signal/rule version, reviewer and immutable
metadata snapshot. Stale new reviews are refused; identical accepted retries stay valid
after later source events. A review ID cannot be reused with different evidence/verdict.
`GET /v1/sdk/reviews` returns bounded metadata, optionally scoped by activity; exact review
ID returns the saved snapshot. The dashboard can inspect earlier reviews after the current
projection changes. Review snapshots are private and do not grant training consent.
SDK reviews can enter private datasets only through the explicit `sdk_agents` consent
extension in [metadata datasets](metadata-datasets-v1.md); existing session-only permissions
do not include them.

## Sources and verification

- [Codex event definitions](https://github.com/openai/codex/blob/67f0994b532b8740f14b5b738af7b7b2a915b1d6/sdk/typescript/src/events.ts)
  distinguish turn terminals from item lifecycle events; turn usage is scoped to the turn.
- [Codex item definitions](https://github.com/openai/codex/blob/67f0994b532b8740f14b5b738af7b7b2a915b1d6/sdk/typescript/src/items.ts)
  identify tool kinds, statuses, exit codes and MCP names.
- [Claude SDK reference](https://code.claude.com/docs/en/agent-sdk/typescript), read
  2026-10-31: result subtype and error flag are separate; partial messages wrap native
  events; tool-failure hooks can identify interruption. Missing flags are not defaulted.
- [xAI Responses reference](https://docs.x.ai/developers/rest-api-reference/inference/responses),
  read 2026-10-31: typed response objects expose status and optional errors. The mapping
  uses those explicit facts rather than assuming success from a stream event name.

Core tests exercise replay/reorder convergence, identity refusal, missing/ambiguous
terminals, zero/missing usage, tool errors, interrupted consumers and parent integrity.
The SDK fixture suite sends actual recorded events through Rust ingest/readback and the
same core projector using `project-capture`, a bounded verification example. It makes no
paid model calls and is not proof of live Grok or browser/mobile subscription capture.

PostgreSQL integration tests exercise pre-migration backfill, raw-event preservation,
replay, late arrivals, stale/expired leases, retry exhaustion, capacity recovery, identical
filter/count pagination, exact event ownership and pinned review readback. The isolated
member browser fixture uses all three actual JavaScript adapters and the Rust durable
recorder against the hosted store/worker, then inspects activity, findings, original events,
late revisions and immutable reviews on desktop and phone. Its native responses are fixtures;
there is no paid provider call or claim of subscription web/mobile history access.
