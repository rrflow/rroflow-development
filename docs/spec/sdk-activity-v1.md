# SDK activity and evidence v1

Status: core projector and structural capture facts implemented on `codex/sdk-monitoring`.
Hosted projection jobs/indexes, shared UI and SDK review persistence are not implemented
by this contract alone. TASK-037 stays open until that complete path is verified.

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

## Storage and UI work still required

The hosted implementation needs account-scoped derived records and durable work/backfill
tracking. New source events must invalidate the relevant projection without charging a
second turn allowance. Worker publication must reject stale revisions, recover crashes
and expose pending/error coverage. Raw events cannot be discarded after projection.
All list/count/filter paths must use the same published cohort and report its coverage.

Activity and Findings must expose SDK operations alongside session work with clear kind
labels, hierarchy, outcome explanation and exact source-event drill-down. Reviews must
pin the projection digest and rule version, retaining the reviewed snapshot when later
events change the current projection. They must not trigger training or promotion.
These server and user-facing requirements are acceptance criteria, not shipped claims.

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
