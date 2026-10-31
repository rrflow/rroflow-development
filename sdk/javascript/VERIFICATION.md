# Capture verification — 2026-10-27

## Live local checks

Official SDKs: `@anthropic-ai/claude-agent-sdk` 0.3.311,
`@openai/codex-sdk` 0.164.0. Claude executable: 2.1.310.

Each check requested one harmless echo command through an existing local sign-in.
The real Rust collector journaled metadata, flushed to a temporary local Rust
ingest service, and read the events back under the expected devspace. No event
payload content was retained or uploaded. No Texas deployment was involved.

| Provider | Native SDK events observed | Additional boundaries/hooks | Server records read back | Result |
|---|---:|---:|---:|---|
| Claude | 37 | 4 hooks + 2 call/stream boundaries | 43 | All observed events delivered |
| Codex | 6 | 2 call/stream boundaries | 8 | All observed events delivered |
| Grok | — | — | — | Live check pending; surface/access unconfirmed |

Claude included PreToolUse, PostToolUse, UserPromptSubmit and Stop hooks, plus
stream events, assistant/user/result messages and rate-limit events. Codex
included thread/turn starts, item start/completions and turn completion.

This is evidence for these small checks, not a certification of complete
provider-internal coverage. No hidden model requests, retries or server actions
are inferred from events a provider does not expose.

## Automated contract tests

`test.mjs` passes synthetic native event streams through the same adapter,
collector process, local queue, authenticated HTTP ingest and read-back path.
It covers all three adapters, unknown native events, token zero versus missing,
failed tools, stream cancellation, provider exception, offline queue preservation,
replay without duplicate records, conflicting identity rejection, account
isolation, wrong-account replay refusal, content redaction and server restart.

Rust HTTP tests exercise both JSONL and Postgres when CI supplies its database,
including immutable identities and refusal to discard an account's event-only
devspace during a claim. Browser state tests exercise the SDK event journal
alongside existing account-owned turn views.

Still required: real Grok verification for the selected surface, pinned-release
installation, native CLI setup/hooks, sustained load and recovery tests,
retention policy, and trusted Texas deployment with real-account read-back.


## Hosted monitoring integration

The `scripts/dashboard-ui` member fixture now connects the real adapters and Rust durable
recorder to PostgreSQL ingestion and its projection worker. Codex tool failures, Claude
terminal outcomes and Grok incomplete responses reach Activity and Findings. Desktop/phone
checks inspect original events, save exact-revision reviews, ingest late events and reopen
the earlier immutable evidence. Providers remain explicit contract fixtures. This expands
automated integration coverage; it does not change the live-provider ledger above.

See `docs/spec/sdk-activity-v1.md` for semantics, capacity controls, worker configuration
and the explicit separation of SDK operations from session turns and dataset policies.
