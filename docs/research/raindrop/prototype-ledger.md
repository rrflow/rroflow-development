# Prototype ledger: the Raindrop wiring already in `EonsofStupid/rrflow-hydradb`

Date: 2026-10-24. Clone: `rroflow-absorb/sources/prototype/rrflow-hydradb` (depth 1, default branch, cloned today). Everything below is read from that tree; nothing was run. Secret values were never read or printed.

## What already exists (and is verified in the repo's own evidence)

- A working Raindrop integration in Jesse's own prototype: real events and feedback signals delivered to Raindrop project `rrflow-hydradb`, org `ea2ca283`, with UI read-back verified on 2026-10-12 [sourced: TEAM.md "Current application and evidence"; research/IMPLEMENTATION_EVIDENCE.md "Raindrop read-back verified"].
- Verified event IDs visible in the live workspace: `0d426672-9f38-482b-a765-cf6551c84482` (retrieval, five hits, `answer_helpful` signal), `2a0539cd-b479-423a-bf64-ec0d5752b364` (`Answer helpful`), `6576c8be-0b0f-510b-a137-aec6aec33451` (A2UI, `Brain UI helpful`, `outcome=ready`, `verification_run=true`), `9ec69549-b8af-4618-b460-5db313e16e67` [sourced: research/IMPLEMENTATION_EVIDENCE.md:9,23,25,49-52].
- Event names in use: `rrflow_setup`, `workspace_sync`, retrieval events with `kind=retrieval`, `recall_mode=fast`, `outcome=evidence_returned`, `hit_count`, the native HydraDB request id and measured latency as properties [sourced: IMPLEMENTATION_EVIDENCE.md:48-50].
- Credential: Infisical Cloud, org `DevPULSE`, project `rrflow-hydradb`, env `dev`, secret name `RAINDROP_WRITE_KEY` (same value as the older `raindrop_apikey`); injected into server process memory by the PowerShell launcher; no key in any app file [sourced: TOOLING.md:7-11,43].
- Raindrop CLI exists: `raindrop cloud setup --local --project rrflow-hydradb`, WSL CLI 0.1.21, OAuth sign-in [sourced: TOOLING.md:11].
- MCP: HTTP server `https://mcp.raindrop.ai/mcp`, configured for Claude Code (`.mcp.json`), Codex (`.codex/config.toml`) and Cursor (`.cursor/mcp.json`); OAuth; Codex sign-in succeeded on 2026-10-12 [sourced: TOOLING.md:55-75; .cursor/mcp.json].
- Official skills pinned at `raindrop-ai/skills@d0e21c3fac39744130252380d20ef0affdb1943e` under `.claude/skills/` and `.agents/skills/` [sourced: TOOLING.md:57-60].

## The HTTP write surface the prototype uses

File: `rrflow-app/apps/rrflow/src/lib/raindrop.ts` (69 lines). Base `https://api.raindrop.ai/v1`; `Authorization: Bearer <RAINDROP_WRITE_KEY>`; `X-Raindrop-Project-Id: <slug>`; response header `x-request-id` captured into a `ServiceReceipt`; 5 s abort timeout; failures reported without discarding the user's result ("Acceptance is not evaluation") [sourced: raindrop.ts:4,41-65].

| Call | Path | Body |
|---|---|---|
| `trackEvent` | `POST /v1/events/track` | `[RaindropEvent]` |
| `trackSignal` | `POST /v1/signals/track` | `[RaindropSignal]` |

```ts
RaindropEvent  = { event_id, user_id, event, properties?, ai_data?: { model?, input?, output?, convo_id?, tool_calls?: RaindropToolCall[] } }
RaindropToolCall = { name, arguments?, result?, status: 'ok'|'error', error? }
RaindropSignal = { event_id, signal_name, sentiment?: 'POSITIVE'|'NEGATIVE', signal_type?: 'default'|'feedback'|'edit'|'standard', properties? }
```
[sourced: raindrop.ts:7-29]

From the pinned skill's HTTP reference (`.agents/skills/raindrop-setup/references/http-api.md`): required event fields `event_id` (UUID), `user_id`, `event`, and `ai_data` with at least one of `input`/`output`; optional `timestamp` (ISO 8601 UTC, no microseconds), `attachments[]` (`code|text|image|iframe`, `role: input|output|context`); 1 MB per event; `POST events/track_partial` for streaming (single object, merged by `event_id`, `is_pending`); `POST users/identify` with `traits`; signal types `feedback` (requires `properties.comment`), `edit` (requires `properties.after`), `default`; SDKs buffer and flush about every second in batches of 10; retry up to 3 times, idempotent on `event_id`; project header omitted/`default` → the org's default Production project; unknown new slug auto-creates up to a per-org limit, archived slug → 403 [sourced: http-api.md:39-47,53-158,218-229].

## The read/analysis surface (pinned MCP tool reference, `.agents/skills/raindrop-investigate/references/mcp-tools.md`)

Facts that shape the learning layer:
- Vocabulary: events = individual AI interactions; signals = patterns detected on events (types: topics, regex, instrumented, metrics); issues = AI-discovered reports of **broad distribution shifts** (dimensions overrepresented: tools, models, signals; timeline); stumbles = **one-off bad experiences**, detected on a scan cadence (`cadence_minutes`, `last_run_at`); traces = OpenTelemetry span trees per event [sourced: SKILL.md:17-26; mcp-tools.md:521-561].
- Signals are **code**: authoring produces "classifier source" (`raindrop_get_signal_session_code`) and refinement acts on "an accepted user JavaScript signal" [sourced: mcp-tools.md:443,452].
- Signal authoring loop: `start_signal_session` from a behavioural description → `authoring` (first round about 4 minutes) → `reviewing` a batch of events → `label_signal_batch` once per batch with `match | no_match | skip`, user judgment never inferred → `refine_signal_session` → `close_signal_session` with `create`+`confirm` or `discard`; one session, one project [sourced: SKILL.md:101-109; mcp-tools.md:433-446].
- Existing-signal refinement: `refine_signal` with `negative_event_ids` (false positives, ≤500), optional `positive_event_ids` (≤500), optional `comment` (≤4,000 chars); headless, applied automatically to the same signal [sourced: mcp-tools.md:448-472].
- Traces: span types `INTERNAL | LLM_GENERATION | LLM_GENERATION_STREAM | TOOL_CALL | SYSTEM_PROMPT`; status `UNSET | OK | ERROR`; `include_context` returns up to eight ancestors plus nearest preceding/following model generations, bounded to ±24 h and 2,000 spans, framed as temporal neighbours "not proof of causation" [sourced: mcp-tools.md:476-517].
- RQL: read-only `SELECT` over `events | traces | users | conversations`; seven-day maximum window for events and traces; `uniqExact(...)` counting (ClickHouse dialect [inferred]); 100 rows default, 1,000 max; content redacted; zero-data-retention orgs cannot run it [sourced: mcp-tools.md:37-58].
- Datasets: shared collections of saved events/traces snapshotted at add time, with labels `{label, color}` — the eval-set primitive [sourced: mcp-tools.md:331-383].
- Events carry `user_traits`, `feature_flags`, `properties` (filterable with ops such as `eq`), a `tools` map `{count, total_duration_ms?, error_count?}`, and a truncated `system_prompt_snapshot` [sourced: mcp-tools.md:75-102].
- Costs: provider-reported plus catalog-priced usage; `pricing_coverage_ratio` and `unpriced_model_calls` qualify every total [sourced: mcp-tools.md:159-185].
- Auth: org API key or OAuth 2.1 token via PropelAuth introspection; write scopes `write:signals`, `write:dashboards`, `write:datasets` [sourced: mcp-tools.md:21,255,333,435].

## The prototype's own design conclusions (Jesse's team, 2026-10-12)

- "A signal classification is evidence about behavior, not automatic authorization to change a rule." [sourced: research/RAINDROP.md:15]
- Experiments compare cohorts of existing logged events; they do not run or replay an agent. Replay is a separate harness that calls the real agent against a dataset and grades outputs (`defineAgent`, `createEvalHandler` in npm `raindrop-ai@1.0.1`) [sourced: research/RAINDROP.md:19].
- Raindrop's OTLP endpoint accepts OTLP/HTTP traces (JSON or protobuf), not metrics or logs [sourced: research/RAINDROP.md:13].
- Proposed correlated identifiers for one agent turn: trace and session ids; RRFlow address and tree revision; rule and snapshot revision; retrieval id and source-version ids; governance verdict and reason; outcome and evaluator version [sourced: research/RAINDROP.md:27-36].
- Proposed flow: trace one turn → Raindrop exposes a recurring failure → correlate with the decision/evidence graph → candidate correction with examples and counterexamples → Governance + held-out evaluation decide promotion → snapshot compiler produces a new revision → compare outcomes against the previous revision [sourced: research/RAINDROP.md:38-46].
- Product decision 10: "Raindrop observes real interactions and supports candidate evaluation. Active prompt/rule revisions remain canonical in HydraDB; no automatic prompt promotion without evidence." [sourced: TEAM.md "Product decisions" 10]. Under the October canon this canonical home is rrflowDB, not HydraDB (build order step 5 points the prototype at RRFlow's API) [sourced: claude/rrflow-build-order.md step 5].

## Open items this ledger raises

- The prototype's `signal_type` union includes `'standard'`, which the HTTP reference does not list [sourced: raindrop.ts:26 vs http-api.md:152-155] — check against current docs.
- Whether `tool_calls` inside `ai_data` is accepted by the current API or only by SDK-shaped payloads [inferred from raindrop.ts:20 vs http-api.md:66-71].
- The pinned skill revision `d0e21c3f` vs the current `raindrop-ai/skills` head — the public-harvest ledger records the current one.
