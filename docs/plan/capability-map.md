# Capability map: Raindrop + Sentry → rroflow.ai

Date: 2026-10-26. Every row cites where the capability was observed. Status for rroflow: **built** (in this repo, run on real data), **planned** (build-order increment), **gap** (neither competitor has it; ours).

Sources:
- **Raindrop:** live workspace capture 2026-10-24/25 (`claude/raindrop-*` docs).
- **Sentry:**
  - AI agent dashboards: docs.sentry.io/ai/monitoring/agents/dashboard/
  - Spend cookbook: sentry.io/cookbook/monitor-ai-agent-spend-with-dashboards-and-alerts/
  - Seer GA changelog (2025-06-17)
  - MCP monitoring: docs.sentry.io/ai/monitoring/mcp

## Capture

| Capability | Raindrop | Sentry | rroflow.ai | Status |
|---|---|---|---|---|
| How data gets in | SDK + OTLP; write API events/track, traces | Sentry SDK gen_ai spans | **Zero-code**: reads Claude Code and Codex session files; same record from every agent | built (`ingest-claude`, `ingest-codex`) |
| Agent / LLM / tool spans | LLM_GENERATION, TOOL_CALL | agent invocations, LLM generations, tool calls with inputs and outputs, handoffs | turn with tool spans; forked subagents; agent-to-agent handoff (`prompt_author`) | built |
| Handoffs between agents | — | "handoffs between agents and to humans" | subagent path and the sending agent are recorded per turn | built |
| Idempotent re-send | upsert by event_id | — | deterministic ids; finished spans only (Raindrop double-lists re-sent spans) | built |
| Prompt and response content | always sent; server PII guard | shown only "if PII is enabled" | **opt-in content**: signals computed on the user's machine, only results and metrics uploaded unless content is turned on (DevPulse law) | built |
| Redaction | server guard, plan-gated settings | PII toggle | client-side redaction before anything leaves; patterns and literals; `doctor` verifies nothing leaked | built |
| Integration check | AI "trace review" + repair prompt | — | `doctor`: 7 deterministic checks + repair prompt; exit 2 on a leaked secret | built |

## Signals and issues

| Capability | Raindrop | Sentry | rroflow.ai | Status |
|---|---|---|---|---|
| Detect behaviour | signals (topic/regex/instrumented/code); 8 presets | errors and tool errors per tool | **behaviour signals v0**, deterministic with evidence: blocked handoff, tool-error streak, usage-limit hit, false done, aborted turn, context loss, correction spiral, mood (both directions); grouped as Failure, Friction, Cost, Context; public catalogue `GET /v1/signals` | built |
| Signal settings | signal settings per project (plan-gated) | alert rules | per-devspace on/off and thresholds (`/v1/signals/settings`), read by collectors before evaluating, edited in the dashboard | built |
| Compare agents | — | per-model views | Claude Code and Codex side by side: turns, hits per signal per agent, agent filter | built |
| Evidence | "Why Triage thinks this is a match" + confidence | trace view | every hit names the tool spans and text that matched, and the rule version | built |
| Author a signal in words | Triage agent writes rules + JS classifier | — | tier 3 judge mints a candidate rule from a description | planned (5) |
| Group into issues | Issues (unlock at 100 events), Stumbles | issue grouping | signals grouped per session and project | planned (4) |
| Root cause and fix | — | **Seer**: root cause from traces, logs, commits; actionability score; auto PR | repair prompt per finding (shipped in `doctor`); per-signal repair for the coding agent | planned (4) |
| Alerts | signal alerts | spend threshold + anomaly detectors | spend and signal thresholds per devspace | planned (6) |

## Spend and subscriptions

| Capability | Raindrop | Sentry | rroflow.ai | Status |
|---|---|---|---|---|
| Cost | lower bound, unknowns counted | estimated from tokens + price list; per user, per conversation, per model | tokens are truth, cost derived from a versioned price table; cache read and write kept separate; unknown is unknown, never $0 | planned (2); token counts with cache read/write split are built |
| Subscription limits | — | — | **gap we fill**: plan windows (Codex `rate_limits`), usage-limit hits as a signal | planned (6); `usage_limit_hit` signal built |
| Wasted spend | — | — | cost of aborted, failed or reverted turns | planned (6) |

## Learning (the rroflow vision)

| Capability | Raindrop | Sentry | rroflow.ai | Status |
|---|---|---|---|---|
| Supervised labels | MATCH / NOT MATCH, rewrites the policy directly | — | label → candidate → held-out eval → promote; reversible | planned (5) |
| Unsupervised | Stumbles | — | failure clustering into candidate signals | planned (7) |
| Recursive | — | — | signal precision measured and improved; promoted rules land in RRFlow | vision / planned |
| Evals | portable evaluator (gated per org) | — | evaluator artifact = rule + judge binding + pinned projection + digest | planned (5) |

## Where data lives

| Capability | Raindrop | Sentry | rroflow.ai | Status |
|---|---|---|---|---|
| Tenancy | org → projects (unclaimed slugs hidden) | org → projects | **devspace** owned by the user's Nomen account, attachable to their Shippin.cloud tenant → projects; the credential decides the devspace, so a client cannot write elsewhere | built (keys); Nomen sign-in planned |
| Store | SaaS | SaaS | Postgres (devspaces, turns, signal hits, settings) with embedded migrations; JSONL for single-user local runs | built |
| Hosting | SaaS | SaaS | rroflow.ai ingest on tx-infra-prod with Postgres; local-first collector works without it | built (verified locally on Postgres); host deploy pending the root run |
| MCP monitoring | — | MCP server tools, resources, errors | MCP tool calls appear as tool spans in the turn (`mcp__*` names) | built (as tool spans) |
