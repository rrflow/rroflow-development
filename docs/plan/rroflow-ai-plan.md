# rroflow.ai: plan v0

Status: **proposal**. Decisions marked `D-n` are open until saved as ADRs.
Date: 2026-10-25.
Grounded in:
- the Raindrop absorption (live workspace capture, signal authoring, onboarding, settings)
- the DevPulse research ledger (6 repos, 8 Drive docs, 2026-03 → 2026-10)
- RROFLOW mechanism spec v0
- this repo's working ingest (Claude Code + Codex → Raindrop, `doctor`)

## 1. What rroflow.ai is

**Sentry for your AI.** It captures what your coding agents actually did and finds where they failed, stalled or wasted your money. Then it learns from your corrections what works, and proposes fixes you approve.

| Sentry does | rroflow.ai does |
|---|---|
| Captures errors from your app | Captures every agent turn (Claude Code, Codex): prompt, tools, results, tokens, cost |
| Groups them into issues | Groups failures into **behaviour signals**: blocked handoff, false "done", correction spiral, tool-error streak, usage-limit hit, wasted spend |
| Shows the stack trace | Shows the evidence: the exact turn, tool span and output that matched, and why |
| Alerts you | Tells you in plain words, with the repair to paste into your agent |
| — | **Learns**: your labels teach it (supervised), recurring failure patterns surface by themselves (unsupervised), and every proposed improvement is tested before it is promoted and can be undone |

**For:** vibe coders first, people who build with AI agents and pay for subscriptions (DevPulse Tier 0: "highest economic exposure"). There is a drill-down for developers who want the raw spans.

**What it protects:** "The user's wallet and time are the scarce resources" (DevPulse PHILOSOPHY, locked).

### Why it beats Raindrop for this user

| Raindrop (observed live) | rroflow.ai |
|---|---|
| SDK integration; built for teams shipping AI products | Zero-code: reads the sessions your agents already write. Our ingest does this today |
| A label rewrites the classifier with no check. On our real data that gave precision 5/8 to 7/8 | A label becomes evidence for a *candidate*. It is promoted only after held-out evaluation, and every promotion is reversible |
| Cost is a lower bound on what the SDK reports | Cost is derived from raw tokens and a versioned price table. Unknown stays unknown, never $0 |
| No view of your subscriptions | Plan usage and limits per agent (Codex records rate-limit windows in every rollout) |
| PII guard masks product names, and the setting is plan-gated | Redaction rules are visible and editable from day one |
| Onboarding trace review uses a model | `doctor` runs the same checks deterministically, free and instant (shipped) |

## 2. What exists today (verified)

- `crates/rroflow-raindrop`. Its pieces:
  - `ingest-claude` and `ingest-codex`, validated on real transcripts. That includes forked Codex subagents, compaction, and only finished tool calls being sent.
  - `doctor`, with 7 deterministic checks, validated on three producers.
  - Redaction that is always on.
  - Raindrop as the first **sink**.
  - PRs #1 and #2, CI green.
- Live Raindrop findings that change the design:
  - Events upsert by `event_id`.
  - A span re-sent with a new status is listed twice.
  - Generated classifiers skip turns without tool payloads.
  - The Query API ignores `?project=`.
  - Unclaimed project slugs are hidden.
- The DevPulse designs to adopt (section 4).
- Local checkout: `D:\projects\rroflow.ai` (dabo), on `feat/codex`.

## 3. Product surfaces

1. **rroflow.ai, public web app.** A story site that explains Sentry for your AI with a live demo built from real captured data (source chip on every panel; no sample numbers), docs, and a waitlist. Learning is shown honestly as "being built", with the loop visible.
2. **`rroflow` CLI, the collector.** `connect`, `doctor`, `ingest-*`, and the Claude Code `Stop` hook / Codex hook. It runs on the user's machine. Sinks: a local store, Raindrop, and later rroflow cloud.
3. **App, the dashboard.** Signals, evidence cards, spend and limits, and labels. Local first. Hosted later, opt-in (see D-2).

## 4. Core model, adopted from DevPulse notes

- **Event.** Aligned to devpulselabs `AiEvent` and OTel GenAI:
  - `CallStarted{provider, model, origin}`
  - `CallFinished{tokens_in, cache_read, cache_write, tokens_out, cost_micros, duration_ms, outcome}`
  - `Disposition{kept}`
  - Per turn: `termination_reason ∈ {completed, tool_error, timeout, budget_exhausted, user_cancel, parent_cancel, model_refusal}`.
  - Every token count is optional. `None` means "provider did not say" and is never priced as zero.
- **Cost is derived, tokens are truth.** A versioned `pricing.v1` table with effective dates. Unknown models are flagged, and a census test fails when a model is seen but missing from the table.
  - Cache tokens stay separate. Today's ingest folds Claude's cache reads out of input, which undercounts (fixed in increment 2).
  - Usage counting per provider: Codex reports cumulative totals, and Claude reports per message.
- **Signals.** Per-domain enums with one stamped envelope `{domain, ms, type, …}` and a global `seq`. Signals fire after the write ("report truth, never intent"). A spec declares delivery, sinks and version: "a kind without a spec does not compile".
- **Content.** Prompt and response text live in an opt-in, scrubbed store joined by hash. Metrics never need content.

## 5. Learning loop: supervised, unsupervised, governed

The pipeline: Capture → Signals (rules) → Evidence → Label → Candidate → Held-out eval → Promote (reversible) → Monitor.

- **Tier 1, rules** (regex/structural, µs). Example: "a tool was denied, then the assistant asked the user to act".
- **Tier 2, similarity.** Embeddings plus per-signal classifiers trained on labels. New categories are new exemplars, not a retrain.
- **Tier 3, model judge** for fuzzy cases only. It answers and *mints* a candidate tier-1 rule ("the slow path mints fast paths").
- **Supervised.** Labels are MATCH/NOT MATCH on signal hits plus outcome labels ACCEPTED / EDITED / REJECTED / ABANDONED. EDITED is the best data, because the edit is the correction.
- **Unsupervised.** Cluster unexplained failures (tool errors, aborts, long correction spirals) into candidate signals for a person to name. Cluster members are candidates, never labels.
- **Governed promotion.**
  - A candidate lands `is_active=false`. It is promoted only when the held-out interval supports the gain, and the held-out set is rotated once used (the vibeguard lesson).
  - A human revert is a verdict on that revision, not a permanent block on the category.
  - Promotion is an authorized, reversible engine change, per the RRFlow learning contract.
- **No fine-tuning** until thousands of journaled examples exist ("training first freezes guesses").

## 6. Signal set v0 (deterministic first)

| Signal | Evidence source | Why it matters to a vibe coder |
|---|---|---|
| Blocked handoff | denied tool or permission, then the agent asks the user to act | the agent stopped doing the work |
| False "done" | completion claim while tools failed or TODOs remain (CC-001 ~30%) | "it said done; it wasn't" |
| Correction spiral | 3+ user corrections or reverts in a session window | wasted time and tokens |
| Tool-error streak | N failing tool calls in a row (exit codes, error spans) | stuck agent |
| Usage-limit hit | Codex `rate_limits`, `UsageLimitExceeded`; Claude `rate_limit_event` | subscription friction |
| Wasted spend | cost of turns ending aborted, abandoned or reverted | wallet protection |
| Mood (both directions) | frustration and positive signals on one scale (owner correction: "not just negatives") | ebb and flow, not punishment |
| Context loss | compaction followed by repeated or forgotten work (CC-002/006) | silent quality drops |

## 7. Architecture

- **Rust-first.** Python only for ML training.
- **Collector:** this crate, split into `rroflow-core` (event model, pricing, redaction), `rroflow-collect` (Claude and Codex parsers) and sinks (`rroflow-raindrop`, local store).
- **Public site:** same pattern as the decided rrflow.ai site (Astro static + islands, plus a Rust gateway that is the only data client; source chip on every live panel; planned states labelled, never sample data). Explainers rendered with Remotion. Lives in `site/` in this repo (repo scope already decided: outward lab + site).
- **Store:** open, see D-4.

## 8. Build order (one PR each; each builds on the last)

0. **Repo ready.** #1 and #2 merged 2026-10-25; checkout at `D:\projects\rroflow.ai`; workspace layout from section 7; CI unchanged (fmt, clippy, tests).
1. **Positioning page.** `site/` skeleton on Astro with the hero "Sentry for your AI", how it works, the learning loop marked "being built", and a waitlist. Deployed to rroflow.ai.
2. **Event model + pricing.** `rroflow-core` with the AiEvent-aligned event, cache tokens split, `pricing.v1`, census test. Ingest moves onto it; the Raindrop sink stays byte-compatible.
3. **`connect` + hooks.** Claude Code `Stop` hook and the Codex equivalent; key reference in keychain or Infisical; project create/claim. Per-turn capture, so compaction no longer loses data.
4. **Signals v0** as deterministic rules with evidence spans, evaluated offline on real captured traffic, with precision reported per signal.
5. **Evidence card.** Label → candidate → held-out result with interval → promote or revert. First on Blocked Work Handoff.
6. **Spend and limits view.** Per agent, project and day; Codex plan windows.
7. **Unsupervised:** failure clustering that proposes candidate signals.
8. **Live demo on rroflow.ai** from the public demo dataset through the gateway.
Hosted track, in parallel from increment 2 on the same event model:
- **H1.** rroflow.ai accounts and project keys; `api.rroflow.ai` ingest accepting the event model (OTLP + events), deduplicated by `event_id`, with the finished-span rule enforced server-side.
- **H2.** Hosted dashboard: signals, evidence cards, spend and limits. It reuses the local dashboard's components.
- **H3.** Sync: the local collector uploads opt-in, redacted projections; content stays local unless enabled.

## 9. Decisions (one per pass)

- **D-1 Positioning — answered 2026-10-25:** *RRFlow is the reasoning engine; RROFLOW (rroflow.ai) is Sentry for AI.* Two brands, one stack: rroflow.ai leads with capture, signals, spend and learning, and links to rrflow.ai for the engine. This replaces the May 2026 "RRO sold to enterprise" headline. Not canon until saved as an ADR.
- **D-2 Order of surfaces — answered 2026-10-25: both in parallel.** The local-first collector and dashboard, and hosted rroflow.ai accounts with cloud ingest, are built as two tracks on one event model. Not canon until saved as an ADR. Consequence: the event model (increment 2) is the shared contract, and hosted ingest must honour the DevPulse laws: no provider keys held; content opt-in and scrubbed before upload.
- **D-3 Site — answered 2026-10-25:** `site/` in this repo, using Astro static with islands, a Rust gateway as the only data client, and Remotion explainers. This is the same pattern as the rrflow.ai site. Not canon until saved as an ADR.
- **D-4 Local store.** Parquet + DataFusion (devpulselabs, 2026-08) vs Turso vs RRFlow when ready. The live repos disagree.
- **D-5 Term.** "Behaviour signal" for the learned classifier, because "signal" is already H-05 telemetry in RRFlow.

## 10. Sources

- Raindrop: `claude/raindrop-*` project docs; live captures 2026-10-24/25.
- DevPulse: project doc `claude/devpulse-research-ledger.md`. It covers platform_devpulse, devpulselabs, devpulse-telemetry, devpulse_alpha and devpulse-template, plus the Drive LFG_NOTES_v3, the RRFlow Classification Compiler notes, adr-turn-identity and the CLYFFY specs.
- RROFLOW mechanism spec v0; the RRFlow learning contract (`controller-training.md`).
