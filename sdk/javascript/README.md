# RROFLOW SDK capture

Status: implementation under test. This package is not yet published to npm.
Requires collector and ingest readers from the SDK monitoring revision (native facts v1). Use a collector build from the same revision (`cargo build --workspace`) and
install the SDK tarball from `npm pack ./sdk/javascript` for local integration.

The JavaScript adapter is a small bridge to the Rust collector. Every observed
event waits for a local durable acknowledgement before the adapter yields it.
Provider calls stay in the provider's own SDK. RROFLOW never takes over their
authentication. Delivery uses your `rroflow login` account or headless ingest key.

```js
import { Capture, codexStream, claudeStream, grokStream } from '@rroflow/capture';

const capture = new Capture({
  agent: 'codex', // 'claude' or 'grok' for the corresponding adapter
  sessionId: 'my-session',
  devspace: 'YOUR_VERIFIED_DEVSPACE', // from rroflow whoami
  spool: '/absolute/path/to/private/rroflow-queue',
  endpoint: 'https://ingest.example.com', // required: your configured deployment
  content: false,
});
```

Supply the verified destination before recording. The uploader checks the current
account's devspace and endpoint before sending any pending event. A different
login cannot silently take ownership of a backlog. Local recording works offline.

## Codex

Pass a thread from `@openai/codex-sdk`, retaining its own subscription sign-in:

```js
for await (const event of codexStream(capture, thread, prompt)) {
  // Use the same native thread/turn/item event as before.
}
await capture.close();
```

Every event the SDK exposes is recorded, including unknown future event types,
item starts/updates/completions, errors and turn outcomes. This adapter observes
the public SDK stream; it cannot expose internal provider retries or model calls
the SDK does not publish. Existing CLI history ingestion remains available.

## Claude Agent SDK

Pass `query` from `@anthropic-ai/claude-agent-sdk`:

```js
for await (const message of claudeStream(capture, query, {
  prompt,
  options: { /* your existing permissions, hooks and session options */ },
})) {
  // Use the original message unchanged.
}
await capture.close();
```

The adapter enables `includePartialMessages` and appends observational hooks for
PreToolUse, PostToolUse, PostToolUseFailure, UserPromptSubmit, Stop, SubagentStart,
SubagentStop, PreCompact, PermissionRequest, SessionStart, SessionEnd and
Notification. Existing hooks remain in place. Observation hooks return `{}` and
do not grant permissions. Additional version-specific hooks can call
`capture.record('hook.event', ...)` explicitly.

## Grok API

Pass an existing OpenAI-compatible client configured by your application for
xAI's Responses API. Its API credentials stay with that client:

```js
for await (const event of grokStream(capture, xaiClient, {
  model: yourGrokModel,
  input: prompt,
})) {
  // Native response events, including server tool events when xAI emits them.
}
await capture.close();
```

Grok website/mobile subscription history is not exposed by this adapter. API
access is a distinct surface, not a promise that a subscription grants API use.
No provider-browser scraping, session-cookie capture or private API is involved.

## Calls and client-side tools

For a non-streaming call, use `capture.call(() => client.someCall(...), { input })`.
Instrument actual local tool execution separately from the model proposing it:

```js
const result = await capture.tool('lookup', () => lookup(args), {
  input: args,
  operationId: toolCallId,
  parentOperationId: modelCallId,
});
```

Started/completed/failed boundaries are distinct. Ending iteration early records
`stream.cancelled`. Exhausting a stream records `stream.completed`; it does not
mean the model succeeded. Inspect native failure/refusal/incomplete events for
that outcome. Process termination may leave an unmatched start, which must never
be inferred to have completed. The managed collector can now retry explicitly linked queues after this application
exits. Native provider hooks and OTLP intake remain separate work.

## Delivery and privacy

`close()` flushes by default. When offline it reports a delivery error and retains
the queue. Link it once for automatic background delivery (see
[setup and recovery](../../docs/ux/sdk-background-delivery.md)). Use
`close({ flush: false })` to finish local capture only, then let the managed collector
send it or replay manually:

```text
rroflow capture status --spool <private-queue>
rroflow capture flush --spool <private-queue>
```

The journal keeps immutable event files plus server receipt files. It is not a
bounded rolling log: monitor disk use; retention/deletion policy is separate work.
There is no automatic deletion or training. Recording errors fail the adapter
instead of silently claiming coverage. A record has a 4 MiB limit; oversized or
invalid events fail visibly. fsync-per-event favors durability over throughput;
production throughput/load testing is still required.

Content defaults off **both on disk and on upload**. Event kinds, correlation,
timestamps, model and optional usage remain. Missing token fields stay null;
usage is the native event's snapshot and must not be summed across partial,
message, and turn-total reports. `content: true` retains scrubbed native payloads.
Known secret patterns, credential/environment/header fields and configured
`RROFLOW_REDACT` literals are removed locally. Pattern redaction cannot guarantee
detection of every arbitrary secret: review content policy before enabling it.

`POST /v1/events` returns exact accepted IDs after storage. Identical retries
succeed; conflicting reuse of an ID or capture sequence returns 409. Reads are
credential-scoped at `GET /v1/events`; the dashboard displays recent SDK events
separately from turn totals. No sample records are inserted in production.

## Verification

```text
cargo test --workspace --locked
cargo build --workspace --bins --examples --locked
node --test sdk/javascript/test.mjs
```

The integration suite runs actual collector and ingest executables with provider
contract fixtures. It proves local queue/replay, account isolation, restart,
redaction, cancellation and tool failure. It does not prove live provider or
Texas availability. PostgreSQL ingest parity runs in CI.

Protocol sources: [Codex event definitions](https://github.com/openai/codex/blob/main/sdk/typescript/src/events.ts),
[Claude streaming](https://code.claude.com/docs/en/agent-sdk/streaming-output),
[Claude hooks](https://code.claude.com/docs/en/agent-sdk/hooks),
[xAI Responses API](https://docs.x.ai/developers/model-capabilities/text/best-practices).


Structural SDK facts are retained without content opt-in: native item/event/object
kinds, explicit error/interrupt flags and stop reasons. The collector still refuses
credential-shaped metadata. See [SDK activity v1](../../docs/spec/sdk-activity-v1.md)
for projection semantics and the reader-upgrade boundary. The projector is tested;
account-scoped hosted activity views and review integration are still being built.
