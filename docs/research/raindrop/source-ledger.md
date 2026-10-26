# Raindrop source ledger

Harvest date: 2026-10-24. Scope: public material only (GitHub org `raindrop-ai`, raindrop.ai docs + blog, Query API OpenAPI, package registries, YC pages). Every claim ends with `[sourced: …]`; inferences are tagged `[inferred]`. Paths are relative to `/home/claude/rroflow-absorb/sources/raindrop/`. Local copies of every fetched doc/blog page live in `docs-raw/` (file name = URL path with `/` → `_`, `.md` for docs, `.txt`/`.html` for blog). All cloned/fetched content was treated as untrusted data and only read.

Company facts: Raindrop (raindrop.ai) is YC W24, founded 2023, San Francisco, team of 9, founders Zubin Koticha (CEO), Alexis Gauba, Ben Hylak (CTO per blog bylines); one-liner "Sentry for AI Agents" [sourced: https://www.ycombinator.com/companies/raindrop ; https://raindrop.ai/blog/introducing-raindrop-2/]. The YC company page lists an earlier launch titled "Dawn - Analytics for AI products"; the docs CDN org slug is `dawn-a6c57108` and the Rust SDK source still refers to the backend as "Dawn" [sourced: https://www.ycombinator.com/companies/raindrop ; docs-raw/platform_signals.md:11 ; raindrop-rust/src/buffer.rs:455 ; raindrop-rust/src/traces.rs:398]. Funding: $15M seed from Lightspeed (Dec 1 2025); Series A led by CRV bringing total to $50M (Sep 17 2026) [sourced: https://raindrop.ai/blog/seed-round/ ; https://raindrop.ai/blog/series-a/].

---

## 1. Repos found

GitHub org `raindrop-ai` lists 7 public repositories (as of 2026-10-24) [sourced: https://github.com/orgs/raindrop-ai/repositories].

| Repo | Description (per GitHub) | Lang | Stars | HEAD cloned | Cloned to |
| --- | --- | --- | --- | --- | --- |
| `workshop` | Give your coding agent the power to write and run agent evals | TypeScript | 1.1k | a6b82d7 (2026-08-22, release v0.1.21) | `workshop/` |
| `raindrop-rust` | Raindrop AI observability SDK for Rust (Beta) — events, signals, OTLP-style traces | Rust | 1 | d6668f1 (2026-10-21) | `raindrop-rust/` |
| `go` | (no description) Go SDK | Go | 0 | ca7d747 (2026-10-21) | `go/` |
| `raindrop-swift` | Raindrop Swift SDK | Swift | 0 | b54de87 (2026-10-20, 0.0.7) | `raindrop-swift/` |
| `skills` | skills for raindrop.ai | Markdown | 3 | 8d7aa0f (2026-10-16) | `skills/` |
| `pi-agent` | Raindrop observability for Pi Agent — read-only mirror of `@raindrop-ai/pi-agent` | TypeScript | 3 | 61c6392 (2026-10-01, 0.2.5) | `pi-agent/` |
| `claude-code-plugin` | Claude Code plugin marketplace (observability for sessions, tool calls, prompts) | JSON | 1 | f242caf (2026-04-09) | `claude-code-plugin/` |

[sourced: https://github.com/orgs/raindrop-ai/repositories ; `git log -1` in each clone]

- A Rust SDK EXISTS: crate `raindrop-ai` v0.0.9, lib name `raindrop`, MSRV 1.88, MIT, `publish = false` (not on crates.io; install via git tag `v0.0.9`) [sourced: raindrop-rust/Cargo.toml:1-19 ; raindrop-rust/README.md:7-20].
- The Rust crate declares its own status as Beta: wire contract stable and verified against the live backend on every push; crate API may change before 0.1.0 [sourced: raindrop-rust/src/lib.rs:5-7 ; raindrop-rust/README.md:7].
- No OpenAPI/HTTP-spec repo exists on GitHub; the Query API publishes an OpenAPI 3 document at `https://query.raindrop.ai/v1/openapi.json` (title "Raindrop Query API (Beta)", version 1.0.0), saved to `query-api-openapi/openapi.json` with a generated summary at `query-api-openapi/openapi-summary.txt` [sourced: https://query.raindrop.ai/v1/openapi.json ; query-api-openapi/openapi-summary.txt:1-3].
- The TypeScript and Python SDKs are NOT on public GitHub: `@raindrop-ai/pi-agent` is an automated export from a private SDK monorepo and depends on an internal unpublished `@raindrop-ai/core` package [sourced: pi-agent/README.md:1-9]. npm `raindrop-ai`, `@raindrop-ai/ai-sdk`, `@raindrop-ai/claude-code`, `@raindrop-ai/mcp`, `@raindrop-ai/browser-sdk` all have no repository URL [sourced: https://registry.npmjs.org/raindrop-ai and the other package endpoints, fetched 2026-10-24].
- Registry versions (2026-10-24): npm `raindrop-ai` 1.0.2 (2026-10-19); `@raindrop-ai/ai-sdk` 0.3.6; `@raindrop-ai/query` 0.1.14 (repo link `github.com/raindrop-ai/query-sdk`, which is private/404 to anonymous clone); `@raindrop-ai/claude-code` 0.0.21; `@raindrop-ai/mcp` 0.1.1 (2026-10-23); `@raindrop-ai/browser-sdk` 0.1.9 [sourced: npm registry JSON per package].
- PyPI `raindrop-ai` 0.0.75 (2026-10-09), requires Python ≥3.10, depends on `traceloop-sdk>=0.46.0`, `opentelemetry-sdk>=1.36.0`, `httpx`, `requests`, `pydantic`, `rfc8785`, optional `quickjs` extra named `portable`; no project URLs listed [sourced: https://pypi.org/pypi/raindrop-ai/json]. PyPI `raindrop-query` 0.1.14 [sourced: https://pypi.org/pypi/raindrop-query/json].
- The `traceloop-sdk` dependency is OpenLLMetry, which explains the `traceloop.*` attribute namespace used across every Raindrop SDK [sourced: https://pypi.org/pypi/raindrop-ai/json ; raindrop-rust/src/traces.rs:686-873] [inferred: that the namespace was inherited rather than chosen independently].
- Go SDK version 0.1.7 [sourced: go/version.go:1-3]. Swift SDK 0.0.7, beta, iOS 16+/macOS 13+/Linux [sourced: raindrop-swift/README.md:1-9].
- The canonical ingest schema is referenced as `@raindrop-ai/schemas/ingest` (`BaseAttachmentSchema`, `SignalEventSchema`) but is not public [sourced: raindrop-rust/src/events.rs:14-16 ; raindrop-rust/src/signals.rs:13].
- Documentation index: `https://raindrop.ai/docs/llms.txt` lists 68 pages; each is fetchable as Markdown by appending `.md` [sourced: https://raindrop.ai/docs/llms.txt].

---

## 2. Event & trace model

### 2.1 Ingest endpoints (write side)

| Endpoint | Method/shape | Notes |
| --- | --- | --- |
| `https://api.raindrop.ai/v1/` | base URL; Rust `DEFAULT_ENDPOINT` | [sourced: raindrop-rust/src/lib.rs:73-74 ; docs-raw/sdk_http-api.md:9-13] |
| `events/track` | POST, JSON **array** of events | primary HTTP endpoint; 204 on success [sourced: docs-raw/sdk_http-api.md:43-64,120 ; skills/raindrop-setup/references/http-api.md:53-91] |
| `events/track_partial` | POST, **single object**; same `event_id` merged server-side; `is_pending` flag | used by all Rust SDK event paths (track_event/track_ai/begin/patch/finish) [sourced: raindrop-rust/src/buffer.rs:44-56,243-247 ; skills/raindrop-setup/references/http-api.md:95-111] |
| `signals/track` | POST, JSON array | [sourced: raindrop-rust/src/signals.rs:99-109 ; docs-raw/sdk_http-api.md:126-170] |
| `users/identify` | POST; Rust sends a single object, HTTP doc shows single object, skill ref shows array | [sourced: raindrop-rust/src/users.rs:18-36 ; docs-raw/sdk_http-api.md:172-206 ; skills/raindrop-setup/references/http-api.md:115-129] |
| `traces` | POST OTLP/HTTP `ExportTraceServiceRequest`; JSON or protobuf; gzip/deflate for protobuf only; no gRPC; no metrics/logs | [sourced: raindrop-rust/src/trace_buffer.rs:70-88 ; docs-raw/sdk_opentelemetry.md:9-24] |

- Auth: `Authorization: Bearer <write key>` + `Content-Type: application/json` on every POST [sourced: raindrop-rust/src/http.rs:144-153 ; docs-raw/sdk_http-api.md:15-21].
- Project routing header `X-Raindrop-Project-Id: <slug>`; slug grammar `^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$`, max 63 chars; omitted/empty/`default` → org's Production project; unknown new slug accepted up to a per-org limit; archived or over-limit → 403; malformed → 400 [sourced: raindrop-rust/src/project_id.rs:13-22 ; raindrop-rust/src/http.rs:155-157 ; docs-raw/platform_projects.md:139-150].
- Debug-only header `X-Raindrop-Sdk: raindrop-rust` [sourced: raindrop-rust/src/http.rs:159-161].
- Write key empty → client is a no-op unless a local Workshop URL resolved [sourced: raindrop-rust/src/client.rs:342-343,428-433].

### 2.2 Event wire payload (`events/track_partial`, Rust SDK)

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `event_id` | string | yes (SDK generates UUID v4 if empty) | [sourced: raindrop-rust/src/buffer.rs:47 ; raindrop-rust/src/client.rs:450-454 ; raindrop-rust/src/helpers.rs:219] |
| `user_id` | string | yes — payload is NOT shipped without it (restored to buffer, debug log) | [sourced: raindrop-rust/src/buffer.rs:373-383] |
| `event` | string | yes; defaults to `ai_generation` | [sourced: raindrop-rust/src/buffer.rs:385-391 ; raindrop-rust/src/lib.rs:81] |
| `timestamp` | ISO-8601 string | yes (SDK fills now()) | [sourced: raindrop-rust/src/buffer.rs:50,417] |
| `ai_data` | object, omitted when none of input/output/model/convo_id set | optional | [sourced: raindrop-rust/src/buffer.rs:51-52,424-435] |
| `ai_data.input` | string | optional (omitted if empty) | [sourced: raindrop-rust/src/buffer.rs:60-61] |
| `ai_data.output` | string | optional | [sourced: raindrop-rust/src/buffer.rs:62-63] |
| `ai_data.model` | string | optional | [sourced: raindrop-rust/src/buffer.rs:64-65] |
| `ai_data.convo_id` | string | optional; conversation/thread grouping | [sourced: raindrop-rust/src/buffer.rs:66-67 ; raindrop-rust/src/events.rs:83-84] |
| `ai_data.tool_calls[]` | array of `{name (required), arguments?, result?, status?, error?, call_id?, timestamps?}` | optional; HTTP API only, NOT emitted by Rust SDK | [sourced: docs-raw/sdk_http-api.md:60,64-104] |
| `properties` | map<string, JSON> | yes (always present); SDK injects `$context` | [sourced: raindrop-rust/src/buffer.rs:53,399-407] |
| `properties.$context` | `{library:{name,version}, metadata:{language:"rust", rustcChannel}}` | SDK-injected | [sourced: raindrop-rust/src/client.rs:379-385 ; raindrop-rust/tests/wire_format.rs:78-85] |
| `properties.raindrop.app.commit_sha` / `.commit_dirty` / `.branch` | string / bool / string | optional; app Git provenance, auto-discovered (750 ms bounded subprocess) or from `RAINDROP_COMMIT_SHA` / `RAINDROP_COMMIT_DIRTY` / `RAINDROP_BRANCH` | [sourced: raindrop-rust/src/app_git.rs:12-16 ; raindrop-rust/README.md:376-415] |
| `attachments[]` | array | yes (may be empty) | [sourced: raindrop-rust/src/buffer.rs:54] |
| `is_pending` | boolean (strict bool, not string) | yes; `true` from `begin`, `false` from `track_event`/`track_ai`/`finish` | [sourced: raindrop-rust/src/buffer.rs:55 ; raindrop-rust/src/client.rs:465,496,535,642 ; raindrop-rust/tests/wire_format.rs:88-89] |
| `feature_flags` | map<string,string> | NOT in Rust or Go wire struct; TypeScript SDK sends it (`featureFlags` array/object → `{"flag":"true"}` normalisation) | [sourced: grep of raindrop-rust/src and go/*.go ; docs-raw/sdk_typescript.md:407-480] |

- Attachment object: `type` ∈ `text|code|image|iframe` (serde-renamed from `kind`), `role` ∈ `input|output` (anything not `input` is treated as `output`; the skill HTTP ref also lists `context`), `attachment_id` (UUID; backend auto-assigns if empty; can be pre-set to pair with a later signal), `name`, `value` (body or URL), `language` (only for `code`) [sourced: raindrop-rust/src/events.rs:22-47 ; skills/raindrop-setup/references/http-api.md:161-179]. Dashboard renders only `text|image|iframe`; `code` survives ingestion but is filtered from the attachments view [sourced: raindrop-rust/src/events.rs:25-27].
- Event = one turn: user input, the tools the agent ran, and the assistant output [sourced: docs-raw/sdk_http-api.md:64]. Pricing defines one event as a user message + agent response including tool calls and sub-agents, or one background agent run [sourced: https://www.raindrop.ai/ pricing section, docs-raw/site_home.txt:1083].
- Size limit: 1 MB per event; properties truncated for larger events [sourced: docs-raw/sdk_http-api.md:124]. Rust SDK drops any serialized payload > 1 MiB client-side (rate-limited warn) to avoid a 413 [sourced: raindrop-rust/src/http.rs:20-24,103-114].
- Text field cap: input/output/tool I/O/LLM content truncated to 1,000,000 chars with marker `...[truncated by raindrop]`; stricter `OTEL_SPAN_ATTRIBUTE_VALUE_LENGTH_LIMIT` honoured [sourced: raindrop-rust/src/client.rs:33,273-288,1069-1077].
- Phantom-event guard: a finalized (`is_pending=false`) payload with empty input AND output and no attachments is dropped (also when event name resolved to `ai_generation` with no `ai_data`) [sourced: raindrop-rust/src/buffer.rs:440-479].
- Event views show finalized events only; a `begin()` interaction appears after `finish()` [sourced: docs-raw/platform_projects.md:154].
- Interaction lifecycle: `begin` (pending patch) → `patch`/`set_property`/`add_attachments`/`set_input` → `finish` (final patch, `is_pending=false`); patches to one `event_id` are merged client-side with last-write-wins on scalars, map merge on properties, append on attachments [sourced: raindrop-rust/src/events.rs:91-152,231-305 ; raindrop-rust/src/buffer.rs:307-345]. `resume_interaction(event_id)` recovers only an in-memory interaction in the same process [sourced: docs-raw/sdk_rust.md:158-160].
- Idempotency: the HTTP API is described as idempotent on `event_id`; retry up to 3 times on network failure [sourced: skills/raindrop-setup/references/http-api.md:227-229].

### 2.3 Signal wire payload (`signals/track`)

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `event_id` | string | yes | event being graded [sourced: raindrop-rust/src/signals.rs:68] |
| `signal_name` | string | yes | e.g. `thumbs_up` [sourced: raindrop-rust/src/signals.rs:69] |
| `signal_type` | string | yes (defaults `default`) | accepted: `default`, `standard` (synonym), `feedback` (pair with `comment`), `edit` (pair with `after`), `agent` (agent self-report), `agent_internal` (not surfaced to end users) [sourced: raindrop-rust/src/signals.rs:11-21,51-64 ; docs-raw/sdk_typescript.md:165-171] |
| `timestamp` | ISO string | optional | [sourced: raindrop-rust/src/signals.rs:71-72] |
| `sentiment` | `POSITIVE` / `NEGATIVE` / `OTHER` (`NEUTRAL` accepted as `OTHER`) | optional; sets sentiment when Raindrop first sees the signal name | TS SDK default `NEGATIVE` [sourced: docs-raw/sdk_http-api.md:145-146 ; docs-raw/sdk_typescript.md:171] |
| `attachment_id` | string | optional | ties signal to one attachment [sourced: raindrop-rust/src/signals.rs:75-76] |
| `properties` | map | yes | SDK merges `comment` and `after` into it [sourced: raindrop-rust/src/signals.rs:85-91] |

- `sentiment: OTHER` is for label signals that describe rather than grade an event, e.g. a user intent like `intent_reorder`; label signals are excluded from positive/negative rates and can filter Experiment cohorts [sourced: docs-raw/sdk_http-api.md:130].
- Self-diagnostics signal shape: `signal_name: "self diagnostics - <category>"`, `signal_type: "agent"`, `properties: {source:"agent_reporting_tool", category, signal_description, detail}` [sourced: docs-raw/sdk_typescript.md:275-288].

### 2.4 Identify payload (`users/identify`)

- `{user_id: string, traits: map}`; skipped when `user_id` empty; recommended traits `email`, `name`, `plan` ∈ `free|paid|trial` [sourced: raindrop-rust/src/users.rs:9-36 ; docs-raw/platform_users.md:31-46].

### 2.5 Trace / span model (OTLP/JSON as emitted by Rust SDK)

- Export shape: `{resourceSpans:[{resource:{attributes:[service.name, service.version]}, scopeSpans:[{scope:{name,version}, spans:[…]}]}]}`; `service.name` default `raindrop.rust-sdk` [sourced: raindrop-rust/src/otlp.rs:234-268 ; raindrop-rust/src/lib.rs:80].
- Span: `traceId` (16 random bytes, base64), `spanId` (8 bytes, base64), `parentSpanId` (omitted when empty), `name`, `startTimeUnixNano`/`endTimeUnixNano` (decimal strings), `attributes[]` of `{key, value:{stringValue|intValue(string)|doubleValue|boolValue|arrayValue}}`, `status:{code 0/1/2, message}` [sourced: raindrop-rust/src/otlp.rs:5-15,33-57,139-232]. End time is clamped to ≥ start because the backend (Tinybird) stores `duration_ns` as UInt64 [sourced: raindrop-rust/src/traces.rs:328-332].
- Span-to-event linkage attributes: `ai.telemetry.metadata.raindrop.eventId` plus canonical `traceloop.association.properties.event_id` (backend `getCustomEventId` prefers the latter) [sourced: raindrop-rust/src/traces.rs:347-372].
- Backend ingestion filter (`hasAIOperation`) silently DROPS spans lacking one of: `ai.operationId`, `traceloop.span.kind`, `traceloop.workflow.name`, `traceloop.association.properties.{user_id,convo_id,event_id}`, any `gen_ai.*` [sourced: raindrop-rust/src/traces.rs:341-345 ; raindrop-rust/README.md:281-286].
- Association properties: every entry of `properties` becomes `traceloop.association.properties.<key>` (strings capped, bool/int/float typed, string arrays as arrays, other JSON stringified); interaction-started spans auto-inherit `user_id`, `convo_id`, `event` [sourced: raindrop-rust/src/traces.rs:832-873 ; raindrop-rust/src/events.rs:359-379].
- Operation ids: LLM spans default `ai.operationId = ai.generateText`; tool spans `ai.toolCall`; README suggests `ai.workflow` for standalone spans [sourced: raindrop-rust/src/client.rs:748-752,808,851-854 ; raindrop-rust/README.md:281-283].
- LLM span attributes: `traceloop.span.kind=llm`; model → `ai.model.id`, `gen_ai.request.model`, `gen_ai.response.model`; provider → `ai.model.provider`, `gen_ai.system`; single input → `ai.prompt`, `gen_ai.prompt.0.role=user`, `gen_ai.prompt.0.content`; messages → `ai.prompt.messages` (JSON array) + `gen_ai.prompt.{i}.role/content`; output → `ai.response.text`, `gen_ai.completion.0.role=assistant`, `gen_ai.completion.0.content`; tokens → `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` (omitted when 0) [sourced: raindrop-rust/src/traces.rs:686-791]. Backend derives per-event `input_tokens`/`output_tokens`/`model` from these, and `gen_ai.response.model` is a required gate — token usage is dropped without it [sourced: raindrop-rust/src/traces.rs:283-295].
- Tool span attributes: `traceloop.span.kind=tool`, `traceloop.entity.name`, `traceloop.entity.input`, `traceloop.entity.output` (bounded JSON strings), `traceloop.entity.duration_ms` [sourced: raindrop-rust/src/traces.rs:796-828,557-614].
- Retroactive `track_tool`: derives start/end from any of start/end/duration, emits a complete span with status OK or ERROR (`error` string → status message) [sourced: raindrop-rust/src/client.rs:820-883,1079-1095 ; raindrop-rust/src/traces.rs:49-72].
- Raw OTLP ingestion maps GenAI semconv: `gen_ai.system` marks an LLM span (required for generation treatment), `gen_ai.response.model`, `gen_ai.usage.input_tokens`/`output_tokens`, `gen_ai.input.messages`/`gen_ai.output.messages` (JSON arrays of `{role, content}`), `gen_ai.system_instructions`, `ai.prompt.tools` (+ `ai.prompt.tools.source`); AI Events are created from eligible model spans without any `begin()`/`finish()` [sourced: docs-raw/sdk_opentelemetry.md:186-208].
- Tool catalog attribute `ai.prompt.tools`: string array, one JSON doc per tool (`{"type":"function",name,description,inputSchema}` or `{"type":"provider-defined",id,name,args}`); absent = unknown, `[]` = model had no tools; recorded uncapped on every model span; `ai.prompt.tools.source` marks a catalog vs exact per-request list [sourced: docs-raw/sdk_tool-catalog.md:7-52].
- Server-side span types: `INTERNAL`, `LLM_GENERATION`, `LLM_GENERATION_STREAM`, `TOOL_CALL`; plus synthetic `SYSTEM_PROMPT` via MCP `get_trace`; statuses `UNSET|OK|ERROR` [sourced: query-api-openapi/openapi.json traces_list schema ; skills/raindrop-investigate/references/mcp-tools.md:576-577]. Workshop's local span types additionally include `TRACE` and `AGENT_ROOT` [sourced: workshop/src/mcp/tools.ts:116].
- Server-side event columns (RQL `events` table): `project_id`, `event_id`, `timestamp`, `event_name`, `user_id`, `convo_id`, `custom_event_id`, `ai_input`, `ai_output`, `ai_model`, `signals[]`, `properties` map, `user_traits` map, `feature_flags` map, `user_plan`, `user_created_at`, `is_internal`, `is_excluded_user`, `input_words`, `output_words`, `tool_calls[json]`, `tool_names[]`, `tool_call_count`, `error_spans[json]`, `error_names[]`, `error_count`, `has_errors`, `token_usage[json]`, `input_tokens`, `output_tokens`, `total_tokens` [sourced: docs-raw/mcp_rql-reference.md:13-47].
- Server-side `traces` table: `project_id`, `event_id`, `trace_id`, `span_id`, `parent_span_id`, `span_type`, `span_name`, `status`, `timestamp`, `end_timestamp`, `duration_ms`, `input_payload`, `output_payload`, `input_tokens`, `output_tokens`, `provider`, `model`, `user_id`, `convo_id`, `attributes` map(string), `attributes_num` map(number) incl. `raindrop.pricing.cost_usd` [sourced: docs-raw/mcp_rql-reference.md:49-73].
- `users` rollup: `event_count`, `first_seen`, `last_seen`, `active_days`, `plan`, `traits`, `signals[]`; `conversations` rollup: `convo_id`, anchor `event_id`, `user_id`, `start_ts`, `end_ts`, `message_count`, `duration_seconds`, `signals[]` [sourced: docs-raw/mcp_rql-reference.md:75-101].
- Query API event object: `id`, `project_id` (only in `*` scope), `event_name`, `user_id`, `convo_id`, `timestamp`, `received_at`, `user_input`, `assistant_output`, `model`, `topics[]`, `input_attachments[]`, `output_attachments[]`, `signals[{id uuid, name, score}]`, `error_spans[]`, `tool_calls[{tool_name,status,duration_ms,started_at,span_id}]`, `feature_flags`, `properties` [sourced: query-api-openapi/openapi.json events_get response schema].
- Query API conversation object adds `participant_user_ids[]` and `participant_count` (multi-actor conversations) [sourced: query-api-openapi/openapi.json conversations_get].

### 2.6 Client batching / flush / retry (Rust SDK defaults)

| Knob | Default | Source |
| --- | --- | --- |
| `partial_flush_interval` (event ticker) | 1 s; non-pending patches flush immediately | [sourced: raindrop-rust/src/client.rs:125 ; raindrop-rust/src/buffer.rs:157-176] |
| `trace_flush_interval` | 1 s | [sourced: raindrop-rust/src/client.rs:126] |
| `trace_max_batch_size` | 50 spans per `traces` POST; flush triggered when reached | [sourced: raindrop-rust/src/client.rs:127 ; raindrop-rust/src/trace_buffer.rs:58-63] |
| `trace_max_queue_size` | 5000 spans; oldest dropped on overflow | [sourced: raindrop-rust/src/client.rs:128 ; raindrop-rust/src/trace_buffer.rs:52-57] |
| `event_max_queue_size` | 5000 distinct event ids; new ids dropped with rate-limited warn | [sourced: raindrop-rust/src/client.rs:47,129 ; raindrop-rust/src/buffer.rs:139-150] |
| `max_attempts` | 3 | [sourced: raindrop-rust/src/client.rs:130] |
| `base_delay` / `jitter_fraction` | 1 s exponential (×2^(n-1)) with ±20 % jitter; `Retry-After` (seconds or HTTP-date) honoured | [sourced: raindrop-rust/src/client.rs:131-132 ; raindrop-rust/src/http.rs:242-290] |
| Non-retryable | any 4xx except 429 | [sourced: raindrop-rust/src/http.rs:189-192] |
| `request_timeout` / connect | 30 s per attempt / 5 s connect | [sourced: raindrop-rust/src/client.rs:36-39,133] |
| `close_timeout` | 10 s hard deadline; remaining telemetry dropped | [sourced: raindrop-rust/src/client.rs:43,958-994] |
| Failure logs | rate-limited to once per 30 s per family | [sourced: raindrop-rust/src/buffer.rs:140-147 ; raindrop-rust/src/http.rs:104-112] |

- HTTP-API reference states the SDKs flush about every 1 second in batches of 10 [sourced: skills/raindrop-setup/references/http-api.md:220]. (Rust trace batch is 50; event patches are per-event-id, not batched [sourced: raindrop-rust/src/client.rs:127 ; raindrop-rust/src/buffer.rs:180-198].)
- Go SDK: `WithTraceBatchSize` default 50 [sourced: docs-raw/sdk_go.md:254].
- Local Workshop mirroring: every POST is also fire-and-forget mirrored (2 s timeout) to `RAINDROP_LOCAL_DEBUGGER` / `RAINDROP_WORKSHOP` URL or, by default, to `http://localhost:5899/v1/` when a 100 ms TCP probe of 127.0.0.1:5899 succeeds [sourced: raindrop-rust/src/http.rs:16-18,116-118,201-240 ; raindrop-rust/src/local_debugger.rs:4-10,83-119].
- Known Rust SDK limitations: no automatic framework instrumentation (manual spans only); no PII redaction (Python `set_redact_pii`, TS `redactPii` exist) [sourced: raindrop-rust/README.md:342-346].

---

## 3. Signals model

- Definition: a signal is a named behavior tracked across every event; each signal labels the events that match it, giving a rate over time, alerts on spikes, and drill-down to matching conversations [sourced: https://raindrop.ai/docs/platform/signals.md (docs-raw/platform_signals.md:7-9)].
- Signal `type` enum (Query API): `topic`, `regex`, `instrumented`, `metric`, `code`; `sentiment` enum `NEGATIVE|POSITIVE|NEUTRAL`; fields `id` (uuid), `name`, `description`, `created_at`, `group_id`, `group_name` [sourced: query-api-openapi/openapi.json signals_get schema].
- Source taxonomy (docs): Created signals (described → classifier drafted; app or MCP; count against slots), Presets (no slot), Regex (literal keyword/pattern; count), Instrumented (SDK `signals/track`; no slot), Tool errors (auto-derived from tool-call errors in traces; no slot), Self Diagnostics (agent-reported; no slot) [sourced: docs-raw/platform_signals.md:103-112].
- Presets: User Frustration, Task Failure, Forgetting, Refusals, Jailbreaking, NSFW, User Praise [sourced: docs-raw/platform_signals.md:83-91].
- Creation flow (UI): describe behavior → Raindrop's agent analyzes traces, writes numbered policy rules, turns them into a classifier, scans events → review panel shows matches one at a time with a reason; label Match / Not Match (arrow keys); label a sample not a checklist; "Run Again" folds labels into the policy and rescans → read/edit the numbered rules in chat → set name, description, sentiment (Positive/Negative/Other) → optional "Preview classifier code" → create → runs on every new event and backfills last 90 days (sampled) [sourced: docs-raw/platform_signals.md:27-63].
- Refinement after live: select wrongly-matched events in the events table → Refine Signal → recorded as confirmed non-matches → agent conversation seeded with current definition and all prior verdicts → in-place update [sourced: docs-raw/platform_signals.md:77-79].
- Triage-created signals are "code signals": a small sandboxed function that labels matching events, seeded with examples from the investigation; refinements like "only count negative mentions" regenerate and re-sample [sourced: docs-raw/platform_triage-agent.md:104-106].
- MCP `create_signal`: inputs `intent` (required), `project` (required), `positive_event_ids[]` / `negative_event_ids[]` (optional, ≤25 each, events from the project within last 30 days, no overlap), `org`; returns `status:"creating"` + `session_id`; background authoring/testing/installing; no polling [sourced: skills/raindrop-investigate/references/mcp-tools.md:450-508].
- `get_signal_creation_status`: phases `preparing`, `scanning`, `reviewing`, `installing`, terminal; `scan.scanned_total` (events in completed batches), `scan.matched_total` (confirmed examples), `progress` (current batch, provisional matches); only `created` includes `signal_id`; scan totals reset per classifier revision [sourced: skills/raindrop-investigate/references/mcp-tools.md:495-508].
- MCP `refine_signal`: `signal_id`, `project` required; `positive_event_ids`/`negative_event_ids` ≤500 each; `comment` ≤4,000 chars; at least one id or a comment; applies only to an accepted user-created JavaScript signal; same headless flow as the Signals page; returns `status:"refining"` [sourced: skills/raindrop-investigate/references/mcp-tools.md:510-534 ; docs-raw/mcp_overview.md:290-315].
- Agent guidance baked into the skill: establish what counts, what is excluded, and whether to judge the current turn or the whole conversation before creating; ask at most one or two focused questions for broad labels; keep `intent` close to the user's goal; never infer labels from aggregates [sourced: skills/raindrop-investigate/SKILL.md:128-158].
- Match-rate sensitivity: four one-sentence variations of the same behavior over the same 2,000 production traces matched 0.9 %–4.6 % and 67 % of traces matched by any variation were rejected by another [sourced: https://raindrop.ai/blog/signals-2-frontier-classification/ (docs-raw/blog_signals-2-frontier-classification.txt:201-213)].

### 3.1 rd-signal-2 architecture (Signals 2.0, launched 2026-08-11)

- Signals v1 (June 2025) trained small classifiers that judged a single input/output pair; v2 exists because agent failures span turns, tool calls and subagents, up to hundreds of thousands of tokens [sourced: blog_signals-2-frontier-classification.txt:100-104].
- For each behavior, rd-signal-2 studies production traces, writes code to collect the relevant context, and trains a task-specific model on that context [sourced: blog_signals-2-frontier-classification.txt:108].
- Generated classifier shape (example): `export function run(event)` → `findToolCalls(event.trace, name)` → deterministic predicate (≥3 identical-input failed calls) → early `return {matched:false}` → `classify(formatForReview(attempts, event.finalResponse), "<claim text>", {spanIds})`; `classify()` handles model training and ongoing tuning [sourced: blog_signals-2-frontier-classification.txt:130-168].
- Ambiguous candidates go to a task-specific classification head plus an in-house reasoning model; deterministic non-matches never call a model [sourced: blog_signals-2-frontier-classification.txt:164-168].
- Cost model: build once + deterministic execution + (ambiguous candidates × compact context) vs. traffic × full-trace LLM judging [sourced: blog_signals-2-frontier-classification.txt:172-183].
- Reported accuracy on their benchmark: precision 71 % (vs Claude Sonnet 5 adaptive 78 %, GPT-5.6 Sol xhigh 71 %, GPT-5.6 Luna xhigh 67 %); recall 87 % (vs 91 / 83 / 75 %); relative per-trace cost 1× vs 260× / 1,600× / 1,650× [sourced: blog_signals-2-frontier-classification.txt:59-88].
- Drift control: daily frontier-model evaluation of a random sample of production signals; regressions trigger prompt re-optimization and classifier retune (deploy → sample → evaluate → find regressions → retune → re-evaluate); customers can inspect and edit the code and prompt behind each signal [sourced: blog_signals-2-frontier-classification.txt:215-248].
- Runtime: generated signal code treated as untrusted; per-customer isolated environment with no credentials or outbound internet, scoped to that org's data; trace context fetched once and cached near evaluation workers so multiple signals reuse it; median trace classified in 100 ms; >20 billion traces per month evaluated [sourced: blog_signals-2-frontier-classification.txt:250-258].
- Issue Detection uses the same architecture; any detected issue can be turned into a Signal, refined, and used in Experiments [sourced: blog_signals-2-frontier-classification.txt:262].
- Signals 2.0 API for calling classifiers from customer systems, and ZDR Signals / Signal Builder for training+hosting classifiers without retaining production data (healthcare-style constraints) — mentioned, no public API doc found (`/docs/query-api/zdr-signals.md` returns 404) [sourced: blog_signals-2-frontier-classification.txt:57,268-272 ; curl of https://raindrop.ai/docs/query-api/zdr-signals.md → 404].
- Signals are per-project: topic classification, signals, "Tiny Models", and Triage operate per project so a model trained in one project does not leak into another [sourced: docs-raw/platform_projects.md:31].
- Slots: 1 custom signal on Hobby, 10 on Pro; past 10 is Enterprise; presets/instrumented/tool-error/self-diagnostic signals do not count [sourced: docs-raw/platform_signals.md:21-23,116].
- Backfill: new created signals backfill the last 90 days sampled; instrumented/preset/tool-error signals reflect data from enablement onward [sourced: docs-raw/platform_signals.md:114-116].
- Signal alerts: threshold on users or events affected, or alert on every match; states active/snoozed/disabled; Slack delivery [sourced: docs-raw/platform_signal-alerts.md:9-17].
- `get_signal` (MCP) returns definition, `occurrence_count`, hourly (≤24 h) or daily timeseries [sourced: skills/raindrop-investigate/references/mcp-tools.md:422-431].
- Query API returns a numeric `score` per matched signal on an event [sourced: query-api-openapi/openapi.json events_get `signals[].score`]. [inferred: `score` is classifier confidence; nothing public states its range or threshold.]

---

## 4. Issues & intents

### 4.1 Issue Detection (Pro)

- Three-layer model (Issue Detection v2, 2026-06-03): Stumbles = a single failure in one run (smallest unit, including agent self-flagged); Issues = the same stumble recurring across many runs/users, ranked by severity with affected-user counts and examples; Signals = classifiers scoring every trace to track a behavior over time [sourced: https://raindrop.ai/blog/introducing-raindrop-2/ (docs-raw/blog_introducing-raindrop-2.txt:18-28)].
- Issue fields: severity `critical|high|medium|low`; status `unresolved|resolved|ignored`; affected event and user counts; report; trend over time; matching events; "affected dimensions" = tools/models/signals overrepresented in matched events; timeline; tags [sourced: docs-raw/platform_issues.md:15 ; skills/raindrop-investigate/SKILL.md:36,56 ; skills/raindrop-investigate/references/mcp-tools.md:597-598].
- Actions: Resolve, Ignore (reasons: not accurate, too broad, too specific, "show less like this"), Unresolve/Reactivate, Duplicate (merge into another issue); "Fix with Devin" opens a coding session grounded in the issue's evidence; writes require a concrete project [sourced: docs-raw/platform_issues.md:50-61].
- MCP: `list_issues` (`status: active|all`, requires org feature flag `new_issues_nav`), `get_issue`, `update_issue` (resolve/ignore/reopen/reprioritize), `get_issue_events`, `get_issue_timeseries` [sourced: skills/raindrop-investigate/references/mcp-tools.md:585-603 ; docs-raw/mcp_overview.md:318-325].
- Slack issue alerts carry a copyable prompt block containing affected tools/models, event and user counts, last-seen, root-cause hypothesis, and issue ID [sourced: docs-raw/cookbooks_self-healing-loop.md:48].
- Empty Issues page on Pro is a "monitoring" state, not proof of no failures [sourced: docs-raw/platform_issues.md:17].

### 4.2 Stumbles (Pro)

- A Stumble = one flagged conversation: what happened, why flagged, timeline of key moments; tags summarize the failure (examples: Logic Error, Hallucination); a Similar panel groups Stumbles with the same pattern [sourced: docs-raw/platform_issues.md:21].
- Detected on a scan cadence (`cadence_minutes`, `last_run_at` returned by `search_stumbles`); default window `max(24h, 2 × cadence)`; up to 50 per page; separate ID space from issues [sourced: skills/raindrop-investigate/references/mcp-tools.md:607-622 ; docs-raw/platform_issues.md:69-77].

### 4.3 Intents / topics (grouping)

- Events carry a `topics[]` array attributed server-side (may be absent for very recent events) [sourced: query-api-openapi/openapi.json events_get `topics`].
- Signal type `topic` exists alongside `code` (legacy topic classifiers vs Signals 2.0 code classifiers) [sourced: query-api-openapi/openapi.json signals_get `type` enum] [inferred: `topic` = the pre-2.0 "small classification model" lineage].
- Topic classification runs per project [sourced: docs-raw/platform_projects.md:31].
- Explicit intent labelling is done with label signals (`sentiment: OTHER`, e.g. `intent_reorder`) [sourced: docs-raw/sdk_http-api.md:130].
- For MCP servers, `@raindrop-ai/mcp` injects a hidden `_intent` tool parameter the model fills on the first call of a prompt; stored as `mcp.intent` property and prepended to the event input under a "Session intent:" header; a hidden `_turn_id` groups one prompt's tool calls into one `mcp_activity` event [sourced: docs-raw/integrations_mcp.md:58-85,124].
- Conversation summaries are a Pro feature [sourced: docs-raw/platform_plans.md:9,20].
- No public page documents an intent taxonomy, how topics are generated, or how many topics exist [sourced: full-text grep of docs-raw for "intent|topic"; only the hits above].

---

## 5. Deep Search / few-shot classifiers

- YC launch "Raindrop Deep Search" (2025-06-19): describe an issue → Deep Search finds examples across millions of events → refine with feedback → start tracking; positioned as production-wide issue-frequency measurement vs offline/online evals [sourced: https://www.ycombinator.com/launches/Nn7-raindrop-deep-search].
- Method per the launch: "bespoke few-shot classifiers" needing only a few examples (no count given), bootstrapping weaker systems from stronger ones to train custom small models analyzing millions of events a day; analogized to materialized views for natural language [sourced: same URL].
- This is the Signals v1 lineage: June 2025 launch of a pipeline for training small classification models over single input/output pairs [sourced: blog_signals-2-frontier-classification.txt:100-102].
- Current "deep search" surface = `search_events` with `mode: "semantic"` (MCP only; Query API supports only `text|regex`), then `get_event` on 3–5 matches, then `text`/`regex` search, then count + timeseries [sourced: skills/raindrop-investigate/SKILL.md:86-91 ; query-api-openapi/openapi.json events_search `mode` enum].
- Semantic search is a Pro feature [sourced: docs-raw/site_home.txt:1098 (pricing: "Experiments & semantic search")].
- Text/regex searches via MCP should stay within 24 h or carry a user/convo filter; wider scans can return `search_timeout` [sourced: skills/raindrop-investigate/SKILL.md:71-72].
- Datasets hold labeled examples (events, traces, custom uploaded rows) with labels Good/Bad/Needs review/Interesting or custom, exportable as CSV; MCP `annotate_items` labels are `{label (1–100 chars), color ∈ green|red|blue|orange|purple|gray}` [sourced: docs-raw/platform_datasets.md:13-19 ; skills/raindrop-investigate/references/mcp-tools.md:392-399].
- Memory: `list_memories`/`create_memory`/`update_memory`/`delete_memory` hold customer-specific context used across surfaces (Pro) [sourced: docs-raw/mcp_overview.md:327-329 ; docs-raw/platform_plans.md:20]. Triage personalization learns from refined signals, feedback, fixed issues, and conversations [sourced: docs-raw/blog_introducing-triage.txt (section 'Why we made our agent an MCP tool', item 2 Personalization)].

---

## 6. Feedback (explicit & implicit)

Explicit (end-user / developer supplied):
- `signals/track` with `signal_type` `feedback` (+`comment`), `edit` (+`after`), `default`/`standard`; `sentiment` POSITIVE/NEGATIVE/OTHER; optional `attachment_id` to grade one attachment; named e.g. `thumbs_up`, `thumbs_down`, regenerations, shares, schema failures [sourced: raindrop-rust/src/signals.rs:11-21 ; docs-raw/sdk_http-api.md:126-146 ; docs-raw/platform_signals.md:93-95].
- Signal review labels: Match / Not Match per sampled event during creation; "Refine Signal" selections stored as confirmed non-matches [sourced: docs-raw/platform_signals.md:43-45,79].
- MCP labels: `positive_event_ids` / `negative_event_ids` on `create_signal` (≤25) and `refine_signal` (≤500) plus free-text `comment` [sourced: skills/raindrop-investigate/references/mcp-tools.md:458-463,516-521].
- Issue triage verdicts: resolve / ignore (with reasons) / reopen / duplicate / reprioritize [sourced: docs-raw/platform_issues.md:50-55].
- Dataset annotations and Workshop annotations (`kind ∈ issue|good|note`, `source ∈ user|claude-code|codex`, run- or span-level) [sourced: skills/raindrop-investigate/references/mcp-tools.md:392-399 ; workshop/src/db/schema.ts:126-139].

Implicit / derived:
- Tool-error signals auto-derived from tool-call errors in traces [sourced: docs-raw/platform_signals.md:111].
- Agent self-diagnostics: SDK generates a tool (`__raindrop_report` default) whose description is prompt-engineered from category definitions; defaults `missing_context`, `repeatedly_broken_tool`, `capability_gap`, `complete_task_failure` (Claude Code plugin adds `noteworthy`); recorded as `signal_type: agent`; custom categories replace defaults [sourced: docs-raw/platform_self-diagnostics.md:13,42,101-108 ; docs-raw/integrations_claude-code.md:264,294,324-330].
- Classifier signals (presets/created) label events automatically; Issue Detection and Stumbles are generated without human input; daily frontier-model audits of signal samples [sourced: docs-raw/platform_signals.md:15 ; blog_signals-2-frontier-classification.txt:219].
- Feature flags and properties attach experiment context to events for later cohort comparison [sourced: docs-raw/sdk_typescript.md:409].

---

## 7. Experiments & replay

- Experiment = Baseline cohort vs Experiment cohort of already-logged events; cohorts defined by date ranges + conditions (model, feature flag, property, tool); outcome = All signals or selected signals; unit Events % or Users %; lower negative / higher positive signal rates count as better; Pro only; one project each [sourced: docs-raw/platform_experiments.md:9-34].
- Verdict rule: likely better / likely worse requires a 95 % CI excluding zero and passing a sample check; Mixed = one outcome improves while another worsens; page, Triage and MCP share the rule [sourced: docs-raw/platform_experiments.md:56-58].
- Also compares Event Stats incl. median cost per event (unpriced events excluded), tools, duration, errors; auto-saves; share link; archive/restore [sourced: docs-raw/platform_experiments.md:65-87].
- MCP: `create_experiment` (write, `write:experiments`), `get_experiment` [sourced: docs-raw/mcp_overview.md:335-345].
- Experiments do not run or replay the agent [sourced: docs-raw/platform_experiments.md:9].
- Change Reports (2026-10-22, Pro/Enterprise): connect GitHub; on merge to default branch Raindrop reads the agent harness (prompts, models, tools), determines which agents changed and how, then reports an early check soon after merge (only if something plausibly related appears) and a full report after ~24 h: verdict, events/users handled, whether each changed tool was called and by how many users, negative signals on events using it, related stumbles, highlights/lowlights with quotes; uses `raindrop.app.commit_sha` to scope traffic by commit, else by time [sourced: docs-raw/platform_change-reports.md:9-42 ; docs-raw/blog_introducing-change-reports.txt].
- Simulations (early access, Series A post): replay real production traffic and test cases against a proposed harness change on every PR, then apply anomaly detection; described as simulating the world around the agent rather than replaying cached tool responses [sourced: https://raindrop.ai/blog/series-a/].
- Workshop replay: project needs `.raindrop/agents.yaml`, a replay server exposing `GET /health` and `POST /replay` on a port in 61020–61044, registered via `raindrop replay register`; `/health` returns `{ok, eventName, port, cwd, command, input, prefillFromTrace, models}`; `POST /replay` body `{replayRunId, sourceRunId?, messages[], systemPrompt?, userMessage?, model?, context}` and stays open until the run finishes; the replayed run must reuse the same event name and set the SDK event id = `replayRunId` so Workshop stitches original and replay [sourced: workshop/skills/setup-agent-replay/SKILL.md:12-26,96-165].
- MCP `replay_run` (Workshop): `run_id` + optional `user_message`, `model`, `system_prompt`, `context`; checks `/health`, scans ports, starts the stored command, prefills from trace, posts `/replay`, waits; returns `replay_run_id` or structured error (`setup_required`, `command`, `cwd`, `log_path`) [sourced: workshop/src/mcp/tools.ts:89-103,377-437].
- Workshop eval loop: coding agent reads spans, writes a code-aware eval asserting on what broke (output, tool-call sequence, state changes, files), runs the agent until it passes [sourced: docs-raw/blog_introducing-raindrop-2.txt:36 ; docs-raw/cookbooks_self-healing-loop.md:124-145].

---

## 8. Workshop & MCP tools

### 8.1 Workshop (local, open-source)

- Install `curl -fsSL https://raindrop.sh/install | bash`; daemon on port 5899 (`RAINDROP_WORKSHOP_PORT`), SQLite at `~/.raindrop/raindrop_workshop.db` (`RAINDROP_WORKSHOP_DB_PATH`); SDKs mirror when `RAINDROP_LOCAL_DEBUGGER` is set or port 5899 is detected; accepts OTLP JSON at `/v1/traces`; `--cloud` flag runs `raindrop cloud setup` instead (writes `RAINDROP_WRITE_KEY` to `.env`, installs hosted MCP + cloud skills); local MCP server named `workshop`, cloud one `raindrop` [sourced: workshop/README.md:14-104 ; docs-raw/workshop_overview.md:13-128].
- Local DB tables: `runs(id,event_id,name,event_name,display_name,user_id,convo_id,started_at,last_updated_at,metadata)`, `spans(id,run_id,parent_span_id,name,span_type,status,input_payload,output_payload,start_time_ms,end_time_ms,duration_ms,model,provider,input_tokens,output_tokens,attributes)`, `live_events(trace_id,span_id,type,content,timestamp,metadata)`, `saved_events(... user_input, assistant_output, signals, properties, summary, source, folder)`, `saved_folders`, `sessions`, `messages`, `annotations(run_id,span_id,kind,note,source,created_at)`, view `runs_with_hints(+model,finished,span_count,live_event_count,payload_total_chars)` [sourced: workshop/src/db/schema.ts:4-173].
- Normalization layer: adapters for Vercel AI SDK, Claude Agent SDK, Traceloop, LiveKit map SDK-specific attributes into `NormalizedLLMSpan{messages[{role,content,toolCallId?,raw?}], userMessage, systemPrompt, model, providerOptions}` / `NormalizedToolSpan{name,args,result,resultIsError}` / other [sourced: workshop/src/spans/normalized.ts:1-82 ; workshop/src/spans/adapters/*.ts].

### 8.2 Workshop MCP tools (11) and what each returns

| Tool | Required args | Returns | Backend route |
| --- | --- | --- | --- |
| `get_current_run` | none | the run the UI is focused on (or active run), `selected_span_id`, `selected_span`, size hints | `GET /api/ui/viewing`, `GET /api/runs/active` [sourced: workshop/src/mcp/tools.ts:16-20,233-256] |
| `query_traces` | `sql` (single SELECT, no CTEs; `limit` ≤1000, `max_bytes` ≤1,000,000) | rows over `runs`, `runs_with_hints`, `spans`, `live_events`, `annotations` | `POST /api/traces/query` [sourced: tools.ts:21-34,257-284] |
| `get_span_payload` | `span_id`, `target ∈ input|output`; optional `jsonpath`, `range`, `max_chars` (8000 default, 32000 max), `format` | payload slice + `next_offset` | `GET /api/spans/{id}/payload` [sourced: tools.ts:35-50,285-304] |
| `annotate` | `run_id`, `kind ∈ issue|good|note`; optional `span_id`, `note` | `{ok, annotation_id, run_id, span_id}` | `POST /api/annotations` [sourced: tools.ts:51-64,305-339] |
| `get_run_outline` | `run_id`; `payload_preview_chars` ≤400 | totals, span-type counts, tool-call counts with previews, first/final LLM previews, flat span list, live-event histogram, detected sub-agents, error shortlist, annotations | `GET /api/runs/{id}/outline` [sourced: tools.ts:65-76,340-350] |
| `ask_agent` | `question`; optional `run_id` | continuation of the captured agent conversation; structured states for missing context / missing provider key / provider error / answer | `POST /api/agents/ask` [sourced: tools.ts:77-88,351-376] |
| `replay_run` | `run_id`; optional `user_message`, `model`, `system_prompt`, `context` | SSE-derived events, `replay_run_id`, or structured failure | `POST /api/replay` [sourced: tools.ts:89-103,377-437] |
| `search_run` | `run_id`, `pattern`; `regex`, `case_sensitive`, `scope ⊂ {span_input,span_output,span_attributes,live_event}`, `span_type ∈ TRACE|LLM_GENERATION|TOOL_CALL|AGENT_ROOT|INTERNAL`, `context_chars`, `max_matches` ≤200 | matches with `span_id`, scope, char range, snippet | `GET /api/runs/{id}/search` [sourced: tools.ts:104-121,438-453] |
| `get_span_context` | `span_id`; `before`/`after` (default 2), `include_parent` | sibling/parent skeletons (id, parent_id, name, span_type, status, start, duration, tokens, model) | `GET /api/spans/{id}/context` [sourced: tools.ts:122-135,454-464] |
| `import_cloud_trace` | `event_id` (must be verified via cloud MCP); `open_in_ui` | local `run_id`; downloads via Query API key (`RAINDROP_QUERY_API_KEY` or a token) | `POST /api/cloud/traces/import` [sourced: tools.ts:136-148,206-211,465-492] |
| `show_in_ui` | optional `run_id` / `event_name` / `user_id` / `span_id` / `note` | UI navigation (`navigate_to_run`, `open_filter`) and `compose_annotation` draft; status if no UI connected | `GET /api/ui/connected`, `POST /api/agent-ui/commands` [sourced: tools.ts:149-162,493-548] |

### 8.3 Cloud MCP (`https://mcp.raindrop.ai/mcp`, Streamable HTTP)

- Auth: OAuth 2.1 (PropelAuth introspection) or API key `Authorization: Bearer`; personal keys follow the user's role; org-wide keys for CI; "Limit to owners and admins" switch; write scopes `write:issues`, `write:datasets`, `write:memories`, `write:signals`, `write:experiments`; API keys can perform writes without scopes [sourced: skills/raindrop-investigate/references/mcp-tools.md:22 ; docs-raw/mcp_overview.md:373-379,387-397].
- Tool inventory (grouped): `ask_agent_question`, `get_agent_progress`, `get_agent_conversation`, `list_agent_conversations`; `list_events`, `get_event`, `search_events`, `get_event_count`, `get_event_timeseries`, `get_event_facets`, `get_error_span_facets`, `run_rql`, `query_cost`, `get_trace`; `list_conversations`, `get_conversation`, `search_stumbles`; `list_signals`, `get_signal`, `list_signal_groups`, `get_signal_group`, `get_application_overview`, `get_dashboard`, `preview_dashboard_panel_query`, `create_dashboard`, `edit_dashboard`; `create_signal`, `get_signal_creation_status`, `refine_signal`; `list_issues`, `get_issue`, `update_issue`, `get_issue_events`, `get_issue_timeseries`; `list_memories`, `create_memory`, `update_memory`, `delete_memory`; `list_datasets`, `create_dataset`, `update_dataset`, `update_dataset_members`, `annotate_items`; `create_experiment`, `get_experiment`; `list_projects`, `list_organizations`, `list_users`, `get_user`, `list_feature_flag_keys`, `list_feature_flag_values`, `get_write_key`; `search_docs`, `submit_feedback`; `raindrop_skills` [sourced: docs-raw/mcp_overview.md:196-360].
- `raindrop_skills` topics: `explore`, `rql`, `rql_reference`, `dashboards`, `signals`, `triage`, (`evals` when exposed) [sourced: skills/raindrop-investigate/SKILL.md:12-20 ; references/mcp-tools.md:42-47].
- Three-tier conversation read model: `get_conversation` (metadata + truncated turns + `page_info`) → `list_events` with `convo_id` (full untruncated turns oldest-first, `tools` map `{count,total_duration_ms?,error_count?}`) → `get_event` (full I/O, properties, matched signals, `user_traits`, truncated `system_prompt_snapshot`) / `get_trace` (full spans; `span_type: SYSTEM_PROMPT` for the full prompt; `include_context` adds ≤8 ancestors and nearest preceding/following model generations within ±24 h and 2,000 spans) [sourced: references/mcp-tools.md:78-105,219-243,538-579].
- `get_event_facets` fields: `event_name`, `user_id`, `signal_id`, `ai_model`; filters by `feature_flags`, `properties`, `user_traits` with `op ∈ eq|neq` [sourced: references/mcp-tools.md:155-171].
- `run_rql`: one read-only SELECT over `events|traces|users|conversations`; ≤7-day window for events/traces (`time_range {from,to}`), raw-text filters ≤24 h, no joins/unions/FROM-subqueries/window functions, ≤2 identity `IN (SELECT …)` subqueries, 100 rows default / 1,000 max, 20,000-char query, 30 s runtime, 8 concurrent per org, redacted content [sourced: docs-raw/mcp_rql.md:9-96 ; references/mcp-tools.md:49-61].
- `query_cost`: `summary|breakdown|timeseries|events`, explicit `from`/`to` ≤31 days (7 for hourly), `cost_basis: reported_plus_catalog`, `pricing_coverage_ratio`, `unpriced_model_calls` [sourced: references/mcp-tools.md:177-203].
- Every event/conversation/turn/issue/signal/stumble/dataset carries an app `url` to use as-is [sourced: references/mcp-tools.md:32].
- Triage agent harness is exposed as a tool (`ask_agent_question`) because tuning individual coding agents gave little improvement and Raindrop cannot control their harnesses; Triage carries personalization and context handoff [sourced: docs-raw/blog_introducing-triage.txt (Why we made our agent an MCP tool)].

### 8.4 Skills repo (triage-loop model)

- `raindrop-setup` drives: Phase 1 explore (list structure, find user-facing LLM call sites, detect existing Raindrop code, package manager, `RAINDROP_WRITE_KEY`, target project slug, pick integration/base SDK/HTTP API via import-signal tables for 19 TS and 12 Python frameworks); Phase 1.5 disambiguate entry points; Phase 2 plan + explicit approval (core `begin()/finish()`, `setUserDetails`, `convoId`, real ids never placeholders, `trackSignal`, attachments, `withSpan`/`withTool`, self-diagnostics, `redactPii`, project targeting); Phase 3 execute with debug logging and ≤2 build-fix attempts; Phase 4 verify event arrives with populated I/O, correct user/convo ids, nested tool spans, properties; recommend MCP server + investigate skill + Slack [sourced: skills/raindrop-setup/SKILL.md:22-41,65-137,159-183,213-256].
- Guardrails: never change control flow, runtime, bundler, or API contracts; instrument at existing call sites; events without `userId` are silently skipped [sourced: skills/raindrop-setup/SKILL.md:15,175,185-196].
- `raindrop-investigate` loop: 1 Orient (`get_application_overview`, `search_stumbles`, `list_signals`) → 2 Investigate (`get_issue`, `search_stumbles`, `get_event`, `get_conversation`, `list_events`, `get_trace`) → 3 Understand (`get_event_timeseries`, `get_event_facets`, `get_event_count`, `query_cost`, `get_signal`, `search_events` semantic/text/regex) → 4 Act (diagnosis: root cause, severity, concrete code/prompt/config fix) → 5 Verify (timeseries after deploy; exact before/after windows with `run_rql`) [sourced: skills/raindrop-investigate/SKILL.md:43-80].
- Principles: discovery-first, project-aware (one project per investigation; `*` only for org-wide aggregates), count before concluding, multi-angle, link with `url`, check both Issues and Stumbles [sourced: skills/raindrop-investigate/SKILL.md:24-39].
- Specialized flows: Deep Search, Stumble Triage, User Investigation, Signal Exploration, signal creation/refinement, dashboard authoring [sourced: skills/raindrop-investigate/SKILL.md:84-219].
- `raindrop-ai-sdk-v7`: version-skew troubleshooting for Vercel AI SDK v7 (native telemetry via `registerTelemetry(raindrop())` vs proxy `wrap()`; `generateText` inside a tool's `execute` auto-wrapped as a subagent) [sourced: skills/raindrop-ai-sdk-v7/SKILL.md:1-120].

---

## 9. HTTP API (ingest + query)

- Ingest endpoints, auth, project header, limits: see §2.1–2.4 [sourced: docs-raw/sdk_http-api.md ; skills/raindrop-setup/references/http-api.md].
- Responses: all authorized ingest requests return 204 [sourced: docs-raw/sdk_http-api.md:120,170,206].
- OTLP `/v1/traces` limits: default 3,000 requests per sliding 60 s per write key (configurable per org); 429 with `Retry-After`; `Content-Type` decides JSON vs protobuf (missing → protobuf); `Content-Encoding` only gzip/deflate (zstd → 400); >32 MiB decompressed → 413 [sourced: docs-raw/sdk_opentelemetry.md:210-223].
- Query API: base `https://query.raindrop.ai`, Bearer Query API key (separate from write key), `X-Raindrop-Project-Id` (default `default`; `*` all-projects with `project_ids` narrowing; unknown/archived → 403, malformed → 400); endpoints `GET /v1/events`, `/v1/events/search` (`mode text|regex`, `search_in` default `user_input,assistant_output`, limit ≤100), `/v1/events/count` (`count_by event_id|user_id|convo_id`), `/v1/events/timeseries` (`interval minute|hour|day|week|month`), `/v1/events/facets` (`field event_name|user_id|signal`, ≤100), `/v1/events/error-span-facets` (`span_type` default `TOOL_CALL`), `/v1/events/{id}`, `/v1/traces?event_id=` (limit default 200, max 1000, `trunc_len`), `/v1/signals`, `/v1/signals/{id}`, `/v1/signal-groups[/{id}[/signals]]`, `/v1/users`, `/v1/users/{id}`, `/v1/conversations`, `/v1/conversations/{id}`, `/v1/projects`, `POST/GET /v1/deletions/users`, `GET /v1/deletions/users/{id}`, `DELETE /v1/deletions/users/{id}/{user_id}`; list limits default 50 max 500; cursor pagination `meta.cursor`/`has_more` [sourced: query-api-openapi/openapi-summary.txt ; docs-raw/query-api_api-reference.md].
- Query API rate limits: 400 RPM most endpoints; 50 RPM for `/v1/events/search`; 40 RPS combined; search rejected once combined usage hits 20 RPS [sourced: docs-raw/query-api_api-reference.md:55-57].
- Deletion jobs: `regulation ∈ gdpr|ccpa|other`, `status ∈ pending|in_progress|completed|rejected`, `due_by` 30 days after receipt, users revocable while pending [sourced: query-api-openapi/openapi.json DeletionJob schema].
- Query SDKs: `@raindrop-ai/query` (npm) and `raindrop-query` (PyPI) [sourced: docs-raw/query-api_sdk.md:13-19].

---

## 10. Scale & operational practices

- Infrastructure subprocessors: OpenAI, Anthropic, Google (AI); Supabase, Tinybird, ClickHouse, Dragonfly DB (databases); Vercel (hosting); AWS, Modal, Microsoft Azure (infrastructure); Axiom, Sentry (observability); PostHog; Stripe; event data encrypted at rest in AWS on dedicated infrastructure; SOC 2 Type II [sourced: docs-raw/security_data-security-and-privacy.md:13-17,30-110 (subprocessor table)].
- Tinybird is the hot store the SDK authors design against: `is_pending` must be a strict boolean because Tinybird casts strictly; `duration_ns` stored as UInt64 [sourced: raindrop-rust/tests/wire_format.rs:88 ; raindrop-rust/src/traces.rs:330-331]. RQL's function set (`uniqExact`, `toStartOfHour`, `arrayJoin`, `quantiles(...)(x)`, `JSONExtract*`, `mapKeys`) is ClickHouse SQL dialect [sourced: docs-raw/mcp_rql-reference.md:103-210] [inferred: events/traces are served from ClickHouse-family storage behind Tinybird].
- RQL caveat: recent reprojections can expose multiple physical rows per logical record until background merges settle (hence `uniqExact(event_id)`) [sourced: docs-raw/mcp_rql-reference.md:15,51].
- Signal evaluation: isolated per-customer sandboxes without credentials or egress; trace context fetched once and cached near workers; 100 ms median per trace; >20 B traces/month; millions of model evaluations/day with queueing and retries [sourced: blog_signals-2-frontier-classification.txt:250-258].
- Stumble detection runs on a per-org cadence, not real time [sourced: references/mcp-tools.md:612-614].
- Enrichment is asynchronous: `topics`, attachments, signals, error spans, tool calls may be absent for very recent events [sourced: query-api-openapi/openapi.json events_get field descriptions].
- PII Guard (server-side): pattern matching + AI models at ingestion, only redacted text stored; per-value cap 131,008 bytes UTF-8 (text past the cap is cut before redaction and not stored); $80/mo base with per-event tiers (1–10k free; 10,001–200k $0.0002; …; 15M+ $0.000022) [sourced: docs-raw/security_pii-redaction.md:15-39]. Client-side `redactPii` (TS only) is regex-based and covers only `ai_data.input`/`output` [sourced: docs-raw/security_pii-redaction.md:42-46].
- Projects isolate ingest, dashboards and AI models end to end; archived slugs are never reusable; no hard delete of projects [sourced: docs-raw/platform_projects.md:25-33,119-125].
- Cost tracking: versioned catalog of public model rates covering uncached input, cache reads/writes, output, reasoning, tiered rates; provider/gateway-reported totals win; per-span `raindrop.pricing.cost_usd` numeric attribute [sourced: docs-raw/platform_cost-tracking.md:13-19 ; docs-raw/mcp_rql-reference.md:73].
- Briefs: Daily Digest, custom briefs (cron, ≥3 h apart, Slack/DM/email), Change Reports; Weekly Digest auto-created per project (Mon 9am) [sourced: docs-raw/platform_briefs.md:9-39].
- Triage runs in Slack, web, MCP; `aside:` / `!aside` suppresses replies in a thread [sourced: docs-raw/platform_triage-agent.md:15-43].
- SDK hardening norms (shared across SDKs per CHANGELOG): never stall the host hot path, never wedge shutdown, bounded memory, rate-limited logs [sourced: raindrop-rust/CHANGELOG.md:19-24].

---

## 11. Export / retention / self-host / pricing & trial

- Plans: Hobby $0 — 1,000 events/mo (ingestion stops for the rest of the calendar month past 1,000), 14-day retention, 1 custom signal, 15 Triage messages/mo; tracing, search, Slack notifications, presets, MCP, Query SDK included; Issues/Experiments visible as preview only [sourced: docs-raw/platform_plans.md:15-19,35 ; docs-raw/site_home.txt:1055-1083].
- Pro $299/mo + $0.003/event up to 1M then $0.002/event; 14-day free trial; 10 custom signals; 500 Triage messages/mo (more purchasable); unlimited retention; realtime Issue Detection, Triage, Stumbles, Experiments & semantic search, conversation summaries, Memory, data export; edge-PII redaction as add-on [sourced: docs-raw/platform_plans.md:20-24 ; docs-raw/site_home.txt:1086-1110].
- Enterprise: custom limits, SSO/SAML, edge PII redaction, audit logs, warehouse exports, priority support, SLAs; VPC deployment announced 2026-06-03 with select partners; homepage lists "Self-Hosting (Beta)" in your own cloud [sourced: docs-raw/platform_plans.md:25 ; docs-raw/blog_introducing-raindrop-2.txt (Announcing VPC) ; docs-raw/site_home.txt:1043-1046].
- Legacy Starter/Startup plans keep their terms [sourced: docs-raw/platform_plans.md:29].
- Export paths: Query API/SDK (programmatic); Users CSV export (paid); Dataset CSV export; warehouse exports (Enterprise); Workshop `import_cloud_trace` pulls one event's trace locally [sourced: docs-raw/query-api_sdk.md:9 ; docs-raw/platform_users.md:52-58 ; docs-raw/platform_datasets.md:41-43 ; docs-raw/platform_plans.md:25 ; workshop/src/mcp/tools.ts:136-148].
- Deletion: user-data deletion jobs via Query API (GDPR/CCPA, 30-day due) [sourced: query-api-openapi/openapi.json Deletions].
- Zero-data-retention orgs cannot run `run_rql` [sourced: references/mcp-tools.md:61].
- Signal slot limit refusal names the limit; lapsed subscription falls back to Hobby limits without deleting data [sourced: docs-raw/platform_plans.md:33,38].

---

## 12. Open questions (only visible inside a logged-in workspace or unpublished)

- Exact classifier training UX: how many labels the review panel typically asks for before "matches look right"; whether labels are per-event or per-span; what the "Preview classifier code" shows (full `run(event)` JS, prompt, both) [sourced: docs-raw/platform_signals.md:41-57 gives the steps but no counts/shapes].
- The `event` object API available inside generated signal code (`event.trace`, `findToolCalls`, `haveIdenticalInputs`, `formatForReview`, `classify`) — only one example exists publicly [sourced: blog_signals-2-frontier-classification.txt:130-168].
- Meaning/range/threshold of `signals[].score` on events [sourced: query-api-openapi/openapi.json].
- How `topics[]` are generated, the topic taxonomy size, and whether users can edit it [sourced: no public page].
- Issue grouping algorithm (how stumbles cluster into issues; "Similar" panel similarity method), severity scoring, and the `new_issues_nav` flag semantics [sourced: references/mcp-tools.md:588].
- Signals 2.0 API / ZDR Signals / Signal Builder request/response contract [sourced: blog mention only].
- Full `track_partial` server-side merge semantics for conflicting fields and the exact `events/track` array size limit [sourced: skill ref says merged server-side, no detail].
- Whether Rust/Go SDKs will gain `feature_flags` and `ai_data.tool_calls` on the wire [sourced: absent from both clones].
- Experiment statistical test used behind the 95 % CI / p-value and the sample-size check thresholds [sourced: docs-raw/platform_experiments.md:53-58].
- Daily drift-audit sample size, retune triggers, and whether customers see audit results [sourced: blog_signals-2-frontier-classification.txt:219].
- What the `quickjs` "portable" extra in the Python SDK executes locally [sourced: https://pypi.org/pypi/raindrop-ai/json].
- Settings surfaces (PII Playground fields, User Exclusions, Filters, API Keys, Billing, Triage "Use all projects") are described but their exact fields are not public [sourced: docs-raw/security_pii-redaction.md ; docs-raw/platform_users.md:50 ; docs-raw/platform_triage-agent.md:23,134].
