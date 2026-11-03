# Capture from other AI runtimes

Status: implementation in this source revision. Qualification is recorded in
`sdk/javascript/VERIFICATION.md`; this is not a hosted rollout receipt.

## What members get

A developer can wrap an AI runtime such as Clyffy with the same JavaScript
`Capture` used by the Claude, Codex and Grok adapters. Recorded calls, executed
tools, failures and interrupted streams reach the connected project's activity
and evidence views after delivery and processing. The member overview links
each reported runtime to its received operations. Runtime names are displayed
as text, including names that match JavaScript object properties.

Recording waits for a durable local acknowledgement. An unavailable server leaves
events queued for retry; changing accounts cannot silently redirect a backlog.
This provides delivery recovery for recorded actions. It cannot recover actions
the application never sent to the wrapper or actions performed on an unconnected
computer or phone.

## Contract

`rroflow.capture/v1` accepts a runtime ID matching `[a-z][a-z0-9._-]{0,63}`.
The ID describes the source; it does not attest to the runtime's identity or
capabilities. Server-owned account/project provenance still comes from the
authenticated ingestion path. `rrflow` remains reserved for the existing
reference-only engine-observation contract. The JavaScript SDK rejects that ID.

The three existing provider adapters require their corresponding runtime ID
before dispatch. Generic applications use `call`, `tool` or `stream` directly.
The runtime ID is immutable for the lifetime of a capture instance. Callers
provide operation and parent IDs when they want to connect nested operations.
An explicitly wrapped tool is distinct from a model proposing a tool call.

Generic payloads do not inherit known-provider interpretation. Model, tokens,
native IDs/status and structural provider facts stay unknown rather than being
guessed from similarly named fields. The native event type is retained for
inspection. Claude hook names are interpreted only for Claude captures; a generic
`PostToolUseFailure` hook remains unsupported evidence, not an inferred failure. Content is absent by default; explicit content consent retains the
scrubbed body. A future normalization adapter needs its own reviewed schema and
qualification before it can report provider-specific usage or outcomes.

Generic `call.completed` establishes that the callback returned. It does not
establish task quality. Ending an iterator without a recognized provider terminal
leaves the operation incomplete. Actual wrapper failures and interruptions emit
the existing deterministic SDK findings, joined to the original evidence IDs.

## Compatibility and remaining work

Generic IDs require the SDK, collector and ingest built from this revision or a
later compatible release. The published v0.4.0 collector rejects these IDs. Do
not pair this SDK capability with that older binary or promise a released npm
package; the package is currently installed with `npm pack` from source.

This change does not connect an installed Clyffy runtime by itself. Its caller
must adopt the wrapper and qualify a real journey. Browser/subscription apps,
hidden model requests and remote actions are covered only when the executing
source exposes and reports them. Claude/Codex session collection remains a
separate, complementary path.

Custom session signal rules do not yet evaluate SDK operations. Dataset consent
currently enumerates the three known SDK sources; generic runtime capture does
not expand dataset consent automatically. Generic dataset selection and normalized
provider metadata require subsequent implementation. The full screenshot-based
member navigation, early-access administration and live Texas acceptance remain
tracked in the platform plan and owner reference document.
