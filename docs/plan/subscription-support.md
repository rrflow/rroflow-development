# Subscription support and capture matrix

Status: implementation matrix, checked 2026-10-26. Provider features change;
pin the client version and repeat a real end-to-end capture before marking support.

| Provider / surface | Supported path established by source | RROFLOW status / next proof |
|---|---|---|
| Anthropic / Claude Code | Official signed-in Claude Code; local sessions and documented hooks | Parser exists. Prove idempotent setup, per-turn capture and upload from a real subscription session. |
| OpenAI / Codex | Official client sign-in; local rollout capture | Parser exists. Prove complete capture including forks, compaction and plan windows. |
| OpenAI / ChatGPT plan usage in RROFLOW | Official OAuth integration for eligible local open-source apps; hosted/paid availability requires access approval | Design option, not shipped. Register the correct client, keep credentials in the owning runtime, and verify eligibility. OAuth sign-in does not grant chat history. |
| Google / Gemini CLI | Sign in with the Google account associated with Pro/Ultra; documented hooks | Adapter needed. Validate events, tools, partial turns and missing usage against pinned fixtures. |
| xAI / Grok and Grok Bot | Official xAI sign-in; eligible subscription access is documented | Capture adapter/history-export route not yet verified. Do not equate product subscription access with a generic inference API or telemetry feed. |
| Cursor | Official client; xAI documentation describes eligible shared subscription usage | Capture adapter needed. Distinguish first-party included usage from separately billed third-party models. |
| Other major subscriptions, including Copilot | Per-provider official client/integration | Inventory and verify before claiming support; do not infer from a brand name. |
| Claude/ChatGPT/Grok browser conversations | Explicit user export or documented connector, when available | Separate adapters. No assumption that CLI parsers cover chat apps or that sign-in grants historical conversations. |

The initial product scope includes coding tools and chat-app integration paths.
Until the owner answers the surface question, this is a planning assumption,
not permission to scrape private browser state or read provider token stores.

## Connection record

Persist provider and surface separately, adapter/version, capability version,
connection status, source locations selected by the user, capture consent,
dataset/training consent references, last successful capture, and known plan
metadata with observation time. States distinguish unavailable, discovered,
awaiting sign-in, consented, capturing, stale and failed. Unknown plan/usage
remains unknown. Discovery alone sends nothing.

A connection to Nomen identifies the RROFLOW devspace. A provider account
authenticates with that provider's supported client. If an approved direct OAuth
integration is later added, it uses its own requested scopes and credential store;
the collector never repurposes another application's cached credentials.

## Meaning of works immediately

Install a supported binary, sign in, see locally discovered sources, consent per
surface, and observe a real turn with tools/evidence/usage where reported. Replay
the same turn without duplication; disconnect without data loss; revoke capture.
Provide a clear unavailable state for unsupported paths. No synthetic activity or
assumed subscription entitlements may satisfy onboarding.

## Primary sources

- [Claude Code overview](https://code.claude.com/docs/en/overview):
  supported product surfaces and account requirements.
- [OpenAI sign-in overview](https://developers.openai.com/siwc/quickstart) and
  [local open-source plan usage](https://developers.openai.com/siwc/token-sharing-open-source):
  optional scoped plan usage; local versus hosted availability; no conversation access.
- [Gemini CLI authentication](https://geminicli.com/docs/get-started/authentication/) and
  [hooks](https://geminicli.com/docs/hooks/): Google sign-in and hook integration.
- [xAI subscription overview](https://docs.x.ai/x-subscription/overview),
  [Grok Bot inclusion](https://docs.x.ai/x-subscription/grok-bot) and
  [sign-in](https://docs.x.ai/x-subscription/grok-bot-sign-in):
  provider-managed access, not proof of a third-party capture interface.

These sources establish available integration paths, not RROFLOW compatibility.
No subscription changes, paid API requests or provider sign-in were performed.
