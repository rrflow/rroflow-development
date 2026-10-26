# DevPulse research ledger — facts bearing on rroflow.ai

Compiled 2026-10-25. Read-only scan of six GitHub repos plus Google Drive. One line per fact; source as `repo:path (date)` or `Drive: "title" (date)`. Repo dates are the file's last commit (first commit in brackets where it matters), taken from the GitHub REST commits API, because the clones are shallow. Where a doc carries its own date in its header, that date is given. Newer beats older; see the "Conflicts and superseded" section.

Repo abbreviations: **PD** = EonsofStupid/platform_devpulse (HEAD 2026-10-24), **LABS** = EonsofStupid/devpulselabs (HEAD 2026-10-25), **TEL** = EonsofStupid/devpulse-telemetry (HEAD 2026-06-28), **ALPHA** = EonsofStupid/devpulse_alpha (HEAD 2026-05-08), **TPL** = EonsofStupid/devpulse-template (HEAD 2026-08-17). LABS-H = devpulselabs at tag `docs-history-2026-10-12` (fetched read-only via the API).

---

## (a) Signal and telemetry definitions, event schemas, taxonomies

### Agent-run and AI-call events (most directly reusable)
- `AiEvent` is a stamped fabric domain with 4 variants, `CallStarted{call, provider, model, origin, observation}`, `FirstToken{call, ttft_ms}`, `CallFinished{call, tokens_in, cache_read, cache_write, tokens_out, total_tokens, context_used, context_size, cost_micros, duration_ms, outcome}` and `Disposition{call, kept, target}`, all keyed by `CallId`. — LABS:crates/fabric/src/lib.rs (2026-10-25)
- Every token count is `Option`. `None` means "provider did not say", which is not zero, "and must never be priced as zero". — LABS:crates/fabric/src/lib.rs (2026-10-25)
- Message content is deliberately absent from `AiEvent`. Prompts and responses go to a separate opt-in, credential-scrubbed store, joined by hash. — LABS:crates/fabric/src/lib.rs (2026-10-25); LABS-H:docs/plans/phase3-analytics-program.md §3.6 (2026-07-29)
- `AiOutcome = Ok | Refused | Cancelled | Error(AiFailureClass)`. A refusal is a valid answer, not a failure. — LABS:crates/fabric/src/lib.rs (2026-10-25)
- `AiFailureClass = Auth | RateLimit | Timeout | Transport | Protocol | Runtime | Server | Unknown`. — LABS:crates/fabric/src/lib.rs (2026-10-25)
- `Observation = Live | Backfilled`. Backfilled facts never carry `FirstToken` and must be excluded from latency and ordering queries ("a latency percentile over backfilled rows is refused, not approximated"). — LABS:crates/fabric/src/lib.rs (2026-10-25); LABS-H:phase3-analytics-program.md §3.2c (2026-07-29)
- `Origin{route, surface, subject}` carries attribution ("what the AI was doing, was it kept"), and `Disposition.kept` is "the attribution payoff". — LABS:crates/fabric/src/lib.rs; LABS-H:phase3-analytics-program.md §3.2 (2026-07-29)
- Mandatory wide-event attributes on every agent span include `devpulse.termination_reason ∈ {completed, tool_error, timeout, budget_exhausted, user_cancel, parent_cancel, model_refusal}`, plus `model_sha`, `prompt_hash`, `cache_hit`, `vram_start/end_mb`, `gen_ai.usage.input/output_tokens` and `gen_ai.response.finish_reasons`. — PD:scripts/docs/adr/ADR-002-TELEMETRY-PRIMITIVES.md D8 (accepted 2026-04-15)
- Ops/agent-session signal vocabulary, one JSON line each `{ts, signal, session_id, lane, detail}`: `session.boot, open.identified, milestone.set, plan.pass, research.web, model.ground, model.close, model.exec, context.budget, journal.wrote, compact.observed, resume.loaded, dirty.set/clear`. Stated purpose: "count missed journals, plan overruns, and which model stage fails". — LABS:docs/ops/references/signals-schema.md (2026-07-29)
- Turn identity: every journal record carries `(turnId, attemptId)`. `turnId` is a ULID that doubles as the W3C trace-id. `attemptId` is minted fresh on every entry and also serves as the fencing token. Record kinds are `turn.lease, turn.delta, turn.terminal, turn.resolution, turn.join.*, turn.payload_gc`. Timestamps are `tsUs`, integer microseconds. — Drive: "adr-turn-identity.md" (ADR-0001, 2026-08-13)
- Doctrine: "The journal records everything, always. Levels and grouping are views over the raw, never write-time decisions. Timing not recorded is timing that cannot be recovered." — Drive: "adr-turn-identity.md" (2026-08-13)
- Vendor block normalization (anti-corruption layer, aligned to OTel GenAI): `input`, `reasoning`, `tool_call`, `retrieval`, `output`, `termination`. Each maps from Anthropic and OpenAI block types to a handler. Key off type and metadata, never reasoning text (reasoning text is often summarized or encrypted). — Drive: "LFG_NOTES_v3" §9 (2026-09-26, PROPOSED)
- Proposed RRFlow category root: Operation (`gen_ai.operation.name`) → Part → Termination (finish reasons) → Cost/effort (Score over usage, including cache tokens) → Domain. Vendor names live only in adapter mapping tables, and additions go under an `rrflow.*` namespace. — Drive: "RRFlow — Classification Compiler: Session Notes" §6 (2026-09-24, PROPOSED)
- Governance signal payload: `GovernanceEvent = ProviderSelected | RateLimitHit | BudgetThresholdReached | CooldownStarted | CooldownEnded | CouncilArbitration | LaneEscalation`, routed to the Broadcast and Analytics sinks. — PD:scripts/docs/sprints/claude-integration/SPRINT-02-GOVERNANCE-SIGNALS.md (2026-04-06, PLANNED)
- Provider auth-health signals to watch. Codex: `account/read`, `account/rateLimits/read|updated`, turn errors `Unauthorized` and `UsageLimitExceeded`. Claude: `claude auth status` exit code, stream-json `rate_limit_event` and `error: authentication_failed`. ACP: `authenticate`/`auth_required`. — LABS:docs/requirements.md R279 (2026-10-18)
- Known "hidden failure" modes seen live: Ready set before sign-in is checked, JSON-RPC errors dropped, approval requests never answered, and raw JSON errors shown while status stays "Ready". — LABS:docs/requirements.md R283 (2026-10-19)

### Behaviour and emotion signals (frustration and handoff)
- PSYCH signal taxonomy: profanity at tool, profanity at code, resignation ("I give up" means near-abandonment), escalating caps (urgency plus repetition), sarcasm (fatigue), short hostile ("fix it", "again", "no"), confused frustration. Explicit NON-frustration cases: celebration, profanity inside an error message, profanity in identifiers, positive surprise. — PD:docs/claude.kb/product/EMOTIONAL_TRANSLATION_LAYER.md (created 2026-03-31)
- `PsychOutput{fru: NONE|LOW|MED|HIGH|RAGE, tgt: TOOL|CODE|SELF|OTHER, urg: 0..1, rec: bool (recurring), pos: bool (positive)}`. — PD:docs/claude.kb/training/NORMALIZER_TRAINING_SPEC.md (2026-04-06; last touched 2026-10-16)
- Owner correction: PSYCH needs "a weight scale where heavy positive to heavy negative and all in between… not just focused on negatives". Joy, flow, streaks, fatigue and confusion carry equal weight, and the user's ebb and flow "is telemetry Clyffy absorbs… never a violation it punishes". — Drive: "CLYFFY_EBB_AND_FLOW.md" (2026-07-24, from the Nov–Dec 2025 doc trail)
- DIARY and PSYCH watch "conversation flow, reverts, smooth periods, error streaks". — Drive: "CLYFFY_EBB_AND_FLOW.md" Pulse 5 (2026-07-24)
- Markable trajectory events to auto-tag: "X process ran for Y", "ram spiked", "user got frustrated", "major milestone reached". — PD:scripts/docs/consolidation/LOCKED_DECISIONS.md F-3 (locked 2026-05-13)
- Outcome labels per interaction: `ACCEPTED | EDITED | REJECTED | ABANDONED`. ABANDONED (`/clear` or the user left) is a "strong negative signal". — PD:docs/claude.kb/training/NORMALIZER_TRAINING_SPEC.md (2026-04-06)
- Teacher signals the contract says should fire, but which no code emits (as of 2026-10-12): `terminology_gap`, `confusion_signal`, `mastery_evidence`, `competency_update`. — PD:.plan/0004-engine--wire-contextual-guidance.md (2026-10-12/16)
- Editor friction signals: `dwell_ms`, `committed:false` (abandonment), Undo rate, Reopened count. — LABS-H:phase3-analytics-program.md §3.4 (2026-07-29)
- `context-pulse` tool: warns at 30+ tool calls and "tracks correction spirals". — PD:docs/claude.kb/build-queue/BUILDABLE_TOOLS.md (2026-03-31)
- Claude Code failure catalogue CC-001…CC-014 (from the v2.1.88 source-map leak). Notable entries: CC-001 false completion ~29–30%; CC-002 compaction at ~167K tokens; CC-006 silent edit failure on stale context; CC-008 compaction disables after 3 failures, with one incident of "250K wasted API calls in a day"; CC-009 frustration regex triggers apologetic mode. — PD:docs/claude.kb/countermeasures/CLAUDE_CODE_COUNTERMEASURES.md (v1.1.0, updated 2026-04-20)

### Signal plumbing doctrine
- Signal doctrine (LOCKED 2026-07-26): per-domain enums on per-domain channels ("a flat global signal enum is FORBIDDEN"); emit-with-mutation, firing after the write succeeds ("signals report truth, never intent"); one envelope `{"domain","ms","type",…camelCase}` at every boundary; `Stamped<T>{seq, ms, ev}` with one global `seq` across all domains (the "flight recorder"); four traffic classes (FACTS lossy broadcast, PROGRESS polled latest-wins, COMMANDS guaranteed mpsc, STREAMS dedicated pipes); batching law ("a signal storm is a bug"). — LABS:docs/design/signal-doctrine.md (2026-10-12; first 2026-07-26)
- Anti-loop law: a surface that publishes a domain never subscribes to it (the coach domain, `build.*`). — LABS:docs/design/signal-doctrine.md; LABS:docs/plans/product-vision.md (2026-10-20)
- Public signal registry is OTel-semconv shaped, extends `gen_ai`, and is "ONE-SIDED: anything not declared here is kernel-only". Families are context, interaction, action, diagnostics and learning, each with a learning_visibility tier. — PD:systems/devpulse-engine/ip/signals.registry.yaml (2026-04-14)
- Registry details: `prompt.state` includes `constructive_rewrite` ("Rage or vulgarity reworded in place"); `prompt.target ∈ {clyffy_local, api_subscription, browser_assistant}`; `diagnostics.proximity ∈ {normal, caution, warning, critical}`; `diagnostics.action ∈ {pause, fork, purge, refresh_context, jit, continue}`; `lesson.kind ∈ {observation, tradeoff, anti_pattern, confirmation, alternative}`. — PD:systems/devpulse-engine/ip/signals.registry.yaml (2026-04-14)
- Every signal spec declares `delivery, sinks, audience, coalesce, version`; "a kind without a spec does not compile". Envelope is CloudEvents-style (provenance header plus payload, `correlation_id`). — PD:.plan/0004-engine--wire-contextual-guidance.md A2 (2026-10-12)
- Telemetry crate: `Envelope<T>{cx, payload}` is mandatory on every cross-task channel, enforced by clippy bans on raw flume/broadcast/`tokio::spawn`; OTel pinned to 0.31; local sink is OTLP JSONL. — PD:scripts/docs/adr/ADR-002 D4–D9 (2026-04-15); TEL:README.md (bootstrapped 2026-06-28; also consumed by `clyffyos`)
- Ledger row schema v1: `seq, ms, wall_ms, domain, type, body, class, dropped`. Additive columns only. `class` is reserved for classification. Bounded channel (4096); overflow drops and counts, never blocks. — LABS:crates/ledger/src/lib.rs (2026-08-12)

## (b) Learning-loop designs (supervised, unsupervised, recursive, evals, promotion)

- Self-generating training loop (locked Dec 25, 2025): log PSYCH/SCRIBE/TEACHER outputs, then capture the user reaction (ACCEPTED/EDITED/REJECTED/ABANDONED), write training pairs to DuckDB, train LoRA nightly, and promote weekly "if metrics improve". — PD:docs/claude.kb/training/NORMALIZER_TRAINING_SPEC.md (2026-04-06)
- "EDITED interactions are the BEST training data": the user's edit delta is the ground-truth correction. Quality scores: ACCEPTED 1.0, EDITED 0.7, REJECTED 0.0, ABANDONED −0.5. — PD:NORMALIZER_TRAINING_SPEC.md (2026-04-06)
- Translation-memory promotion: after 3 accepted uses at >0.8 confidence, a mapping becomes "auto-apply". — PD:NORMALIZER_TRAINING_SPEC.md (2026-04-06)
- learn-loop: log each error to `.claude/mistakes.md`, dedupe, count frequency, and promote to a rule at 3 occurrences. — PD:docs/claude.kb/build-queue/BUILDABLE_TOOLS.md (2026-03-31)
- "The slow path mints fast paths": tier 1 rules (regex, µs), then tier 2 similarity (encoder plus per-node classifiers), then tier 3 reasoning, which answers and mints a candidate tier-1 rule. — Drive: "RRFlow — Classification Compiler" (2026-09-24); "LFG_NOTES_v3" §4 (2026-09-26, LOCKED)
- Promotion posture: tier-3-minted rules land as `source: learned, is_active: false`, promoted in the Lab or by a hit-count threshold. User corrections (`memory.corrections`) also mint rules. "Nothing goes live on the model's say-so." — Drive: "RRFlow — Classification Compiler" §7 (2026-09-24, PROPOSED)
- Category lifecycle: narrated or minted → candidate → validated (grammar, sibling collision, exemplar count) → promoted (Lab or hit-count) → compiled into the next snapshot. Overrides of LFG answers are journaled as corrections, "which is the tuning signal". — Drive: "LFG_NOTES_v3" §2, §5 (2026-09-26)
- Supervised tier 2: classical ML (LR or GBM over TF-IDF, or a small encoder), trained per Choice node, with the journal as the labeled corpus. Disposable and rebuildable; low-probability answers still escalate. — Drive: "RRFlow + LFG — Notes v2.1 Addendum" A8 (2026-09-25, PROPOSED)
- Embeddings, not a single trained label classifier, "so new categories are new exemplars and not a retrain". — Drive: "LFG_NOTES_v3" §4 (2026-09-26)
- Research framing: "RAG isn't dead, it split". The open gap is "promoting into rules — cheap, inspectable, reversible, CPU-fast — is largely unbuilt". — Drive: "RRFlow — Classification Compiler" §12 (2026-09-24)
- Governance verification evaluators run in tier order (deterministic, then pattern, then similarity, then model for fuzzy checks only). Unevaluatable counts as fail; next step is `done | retry{attempt,max} | escalate`; "fail-then-pass pairs are promotion evidence". — Drive: "LFG_NOTES_v3" §8 (2026-09-26)
- Proof pattern (LOCKED): name the seam, freeze it with a golden file, prove it with an artifact CI keeps. "Anything not expressible this way is a guideline, not governance." — Drive: "LFG_NOTES_v3" §8 (2026-09-26)
- No fine-tune "until the journal holds thousands of real translations… Training first freezes guesses". Unsloth/LoRA "teaches format and vocabulary, not categorically different behavior". — Drive: "v2.1 Addendum" A11 (2026-09-25); "LFG_NOTES_v3" §15
- Closed-loop product shape (ratified 2026-07-28): observe (signals → ledger), understand (queries), then "recommend an action that can trigger the guided workflow itself… 'you really should do X' arrives with a button". Analytics "is a lens over the ledger files, never a store of record". — LABS-H:docs/plans/signal-ledger-and-lesson-foundation.md §9.1
- Determinism modes: Behavioral (default; the measurement mode, where A/B of lesson arms runs) and Strict (byte-identical evidence mode). Escalation is dynamic. — LABS-H:signal-ledger-and-lesson-foundation.md §9.2 (2026-07-28)
- Operator-intent gate: privileged transitions are authorized by keystroke provenance. "Model output is never re-parsed for commands", and `OperatorIntent` has no Clone, Default or serde. — LABS-H:signal-ledger-and-lesson-foundation.md §9.3 (2026-07-28)
- Experiment lifecycle: research note → feature-gated spike → journal PASS|KILL|AMEND → ADR only if law changes. Each manifest declares hypothesis, kill criteria, stamped signals and promotion target. — LABS-H:docs/plans/coach-lab-experiment-program.md (2026-07-31)
- Eval framework hypotheses: H1 context preservation (specialists vs generalist: success × token cost × latency); H2 handoff fidelity (typed A2A vs free-text); H3 KV-cache reuse (A/B LMCache on vs off). Tools named: VeriLA and promptfoo. — PD:scripts/docs/consolidation/LOCKED_DECISIONS.md §G (2026-05-13)
- Multi-LLM evidence gathered: majority voting accounts for most gains credited to debate; judge selection beat MoA synthesis; Self-MoA beat mixed MoA; multi-agent setups lose on sequential tasks; routers cut cost by 2× or more. Council output is "one selected answer, never a blend", with runner-up and dissent recorded. — LABS:docs/requirements.md R279 (2026-10-18); LABS:docs/plans/product-vision.md §2.3 (2026-10-20)
- Reversible hot-swap mechanism: encoder or adapter swapped atomically via ArcSwap, with pause → drain → load → validate → swap. If validation fails, it restores the old version. — PD:scripts/docs/specs/cortex/rro/RRD.md (v1.0.0, 2026-05-01); snapshot swap in Drive: "LFG_ENGINE_PLAN" §5 (2026-09-26)
- Incremental enforcement ladder: Stage 0 observe only → 1 redirect → 2 evaluation required → 3 approval receipt (digest) → 4 narrow mutation gate → 5 broader enforcement, each stage with an explicit rollback path. — Drive: "Shippin — Agent-Agnostic Seam, Hook Routing, and Evaluation Modal" (2026-09-18)
- Frustration-handling eval query: compare accept rate by frustration level. "If RAGE interactions have HIGHER accept rates than NONE… the normalizer is working." — PD:NORMALIZER_TRAINING_SPEC.md (2026-04-06)

## (c) AI subscription, cost and usage tracking

- "AI stats = cost/tokens per provider+model · latency/failure · attribution (what the AI was doing, was it kept) · prompt/response content. All four." (ratified). — LABS-H:phase3-analytics-program.md (2026-07-29)
- "Cost is derived; tokens are truth": always store raw tokens and model; `cost_micros` is computed from a versioned price table so it can be recomputed. — LABS:crates/fabric/src/lib.rs; LABS-H:phase3-analytics-program.md §3.2
- Price table design: `.userdata/analytics/pricing.v1.json`, holding a version, per-model rates and an effective date. A census test fails on a model seen in events but missing from the table. An unknown model gets `cost_micros=0` and the row is flagged. — LABS-H:phase3-analytics-program.md §3.3 (2026-07-29). **Status: not built.** No pricing file or table exists in LABS crates; code comments still say "price table (PR 2)". (LABS:crates/app/src/chat.rs, HEAD 2026-10-25)
- `ProviderUsage` trait, one adapter per provider: `reports() → UsageReporting{input_output_split, total_tokens, context_window, provider_priced, model, counts_are_cumulative, history_backfill}`, `usage_from_line`, `model_from_line`, `cost_micros_from_line`. — LABS:crates/chat/store/src/lib.rs (2026-08-03)
- Honesty mechanism: capabilities are "declared, never discovered", so a surface says "does not report usage" instead of showing a silent zero. Gemini declares `NOTHING`. — LABS:crates/chat/store/src/lib.rs; LABS:crates/app/src/ai_usage.rs (2026-08-03)
- Cumulative vs per-turn trap: Codex reports cumulative totals (do not sum), Claude reports per message (sum). "Getting this backwards silently under- or over-counts real spend." — LABS:crates/chat/store/src/lib.rs (2026-08-03)
- Cache tokens kept separate: cache reads bill at a fraction of fresh input and cache writes bill above it. "Folding them into input makes cost an overestimate". — LABS:crates/chat/core/src/lib.rs (2026-10-20)
- A provider-computed cost (Claude reports real money) is authoritative over DevPulse's own table. — LABS:crates/chat/store/src/lib.rs (2026-08-03)
- Census law: `every_provider_has_a_usage_adapter` fails if a provider manifest lacks a usage adapter. Providers today are Codex, Claude, Grok and Gemini. — LABS:crates/app/src/ai_usage.rs (2026-08-03)
- Piggyback backfill: walk existing Codex/Claude session JSONL to recover past spend. Idempotent via a manifest keyed `provider/native-session-id`; Grok ACP is live-only and excluded. — LABS:crates/app/src/ai_backfill.rs (2026-08-03); owner: "if the user already has active sessions we should be able to piggy back off them" (LABS-H:phase3 §3.2c)
- Native usage sources: Codex `thread/tokenUsage/updated` (`last, total, modelContextWindow`); Claude `rate_limit_event`; Grok ACP `Usage{used,size}`. — LABS:crates/chat/providers/codex/src/runtime.rs, grok/src/runtime.rs (2026-10)
- Subscription-only law: use official subscription-backed client surfaces (Codex app-server JSON-RPC, Claude Code stream-json, Gemini/Grok ACP); "DevPULSE never holds a key". — LABS:docs/plans/chat-provider-runtime-adr.md (RATIFIED 2026-07-23); LABS:docs/plans/product-vision.md (2026-07-30)
- Anthropic terms (re-verified 2026-10-18): use the unmodified `claude` binary with the user's own subscription; developers "may not collect, store, or intermediate Claude.ai credentials"; usage limits "assume ordinary, individual usage". — LABS:docs/requirements.md R279
- Gemini path (ruling 2026-10-18, "Both, subscription first"): Antigravity CLI headless on the subscription is the default, with Gemini API on Developer Program credits ($10 Pro / $40 Ultra monthly per current plan page) as fallback. Gemini CLI on Pro/Ultra ended 2026-06-18. — LABS:docs/requirements.md R280→R281
- Subscriptions do not include embeddings, which need a local model subprocess or a separate key. — LABS:docs/requirements.md R275 (2026-10-12)
- Provider model (PD): `ProviderDefinition{kind: subscription_agent|mcp_client|api|endpoint, adapter, detect, signed_in, version, update}`. Five of six subscriptions share the ACP adapter; Claude Desktop is MCP only and cannot power Clyffy. — PD:.plan/0001-engine--rebuild-onboarding-workflow.md (2026-10-10, updated 2026-10-23)
- Older lane budgets: internal lane at 60 rpm, 1M tokens/day, $0 cap; API lane at 20 rpm, 500K tokens/day, `cost_cap_daily_usd = 50.0`; 429 cooldowns; council escalation on rate limit. — PD:scripts/docs/sprints/claude-integration/SPRINT-01-FEATURE-GATE-CONFIG.md (2026-04-06, PLANNED)
- Council cost-runaway mitigation: "Per-invocation cost estimate BEFORE dispatch. Budget caps in config. Hard stop per day." — PD:scripts/docs/sprints/claude-integration/POAM-COUNCIL-MCP-COCKPIT.md (2026-04-06)
- Economic-protection pillar: "The user's wallet and time are the scarce resources we're defending — not compute, not tokens, not latency." Vibe coders pay a "learning tax" in tokens through failed iterations. — PD:scripts/docs/PHILOSOPHY.md (LOCKED v1.2.0, 2026-04-19; file touched 2026-10-24)
- Dalal (LLM gateway) keeps a "capability registry scored from journaled outcomes" and selects the strongest model per address. Its doc flags a risk: routing consumer subscriptions through a third party may break provider terms. — Drive: "LFG_NOTES_v3" §10 (2026-09-26)
- Turn duplicates are a cost risk: "Turns are paid inference… execute-the-duplicate would double-spend". Join makes a duplicate cost one journal read. — Drive: "adr-turn-identity.md" D4 (2026-08-13)

## (d) Target user (vibecoders), UX and onboarding principles

- "DevPulse is a dedicated dev platform for vibe coders (R-0.1)." First run should lower the barrier and not ask "questions machine code can answer". — PD:.plan/0001 (2026-10-10/23)
- User tiers: Tier 0 non-coder "vibe coder" is the primary target (accepts the first output, has the highest economic exposure); Tiers 1–3 calibrate TEACHER, SCRIBE and GUARDIAN levels. — PD:docs/claude.kb/user-tiers/TIERS_AND_PAIN_POINTS.md (March 2026)
- Cited figures: 0.92 correlation between prompt and output quality; −17% skill formation when delegating; 0–20% of tasks fully delegatable; 85.7% iterate but don't verify. — PD:docs/claude.kb/CONSOLIDATED_REFERENCE.md (2026-03-31)
- Emotional translation one-liner: "Every other product tells users to stop being human. DevPulse translates being human into better results." The user never has to change behaviour for the tool. — PD:docs/claude.kb/product/EMOTIONAL_TRANSLATION_LAYER.md (2026-03-31)
- Frustration response rules: acknowledge briefly, then act; this is user-toggleable (`acknowledge_frustration`, `frustration_translation`, `show_translation`). Validation-Bridge-Action protocol; never say "calm down". — PD:NORMALIZER_TRAINING_SPEC.md (2026-04-06); Drive: "CLYFFY_EBB_AND_FLOW.md" (2026-07-24)
- Teaching without lecturing: a "Show me what you translated" view lets users learn by watching; terminology unlocks by demonstrated mastery (6+ exposures, 80% mastery, 3+ contexts). — PD:NORMALIZER_TRAINING_SPEC.md; Drive: "CLYFFY_EBB_AND_FLOW.md" Pulse 3
- The visible task list is never trusted on the AI's word: "9 out of 10 it's going to lie to you and say task completed… likely 60% done". Verify, no TODOs left. — Drive: "CLYFFY_EBB_AND_FLOW.md" Pulse 2 (owner quote, Dec 2025)
- Onboarding: optional one-time "Teach Clyffy how you talk" calibration; a "Here's what I learned" snapshot; a pull-not-push Getting Started checklist that ticks "only when the real signal arrives"; "Every line on screen is something that actually happened". — PD:.plan/0001 Stages 4–5 (2026-10-12+)
- Guidance must come from deterministic rules over real signals, carry provenance ("Why am I seeing this?"), and show no invented gauges. Dashboards render only real values. — PD:.plan/0004 (2026-10-12)
- "The product is the EXPERIENCE, not the feature list." — LABS:docs/requirements.md R217 (2026-07-28)
- Honesty law: boot names what is on or off (for example, content capture opt-in). — LABS-H:phase3 §3.6; LABS:adr-014 law 5
- Success metrics: ROTI > 5.0, edit distance < 10%, verification rate > 80%, terminology adoption trending up, session continuity > 90%. ROTI = (Task + Teaching + Context value) / Time invested. "If it doesn't move ROTI, it's decoration." — PD:scripts/docs/PHILOSOPHY.md (2026-04-19); Drive: "CLYFFY_EBB_AND_FLOW.md" Pulse 8
- Guardrail UX: a card with what was blocked, why, a safe alternative and an override with a risk checkbox. It "never dead-ends". AI may propose but only the user confirms. — Drive: "CLYFFY_SPEC.md" §4, §8 (2026-07-24)
- Lazy vibecoders get pre-captured workflows; gates require manual approval at first, then the whitelist evolves. — LABS:docs/requirements.md (owner quote, ~line 1407)
- Retention is "a report, not a reaper": nothing auto-deletes operator material. User-initiated deletion shreds payloads but keeps the timing skeleton. — LABS:docs/design/adr-014-three-ring-root.md (2026-07-25); Drive: "adr-turn-identity.md" D6

## (e) Architecture decisions (Rust-first, local-first, storage, sync)

- Rust is the default; Python is allowed and is "the obvious choice for ML training, fine-tuning, and data pipelines" (owner 2026-09-23 and 2026-10-24). — PD:CLAUDE.md (2026-10-24)
- "AI = providers over endpoints": local runtimes are prebuilt, supervised subprocesses; subscriptions run the vendor's own CLI over ACP; "no new compiled-in inference". — PD:CLAUDE.md (2026-10-24)
- Local-first redefined as "a trust and latency property, not geography". Earlier code wrongly treated local-first as "no network" and cut the brain contract at token level. — Drive: "RRFlow — Classification Compiler" §9 (2026-09-24)
- PD storage today: Turso is SSOT; SurrealDB runs as a supervised subprocess "until RRFlow replaces it"; DuckDB handles analytics. — PD:CLAUDE.md (2026-10-24); PD:LOCKED_DECISIONS.md A-5 (superseded 2026-10-24)
- LABS storage: three rings (STATE `.userdata`, WORKING `assets`, CONFIRMED `content`). Turso holds metadata; analytics is Parquet, queried in-process by Arrow/DataFusion, and DuckDB is removed (owner 2026-08-03). — LABS:docs/design/adr-014-three-ring-root.md (amended 2026-08-03)
- DataFusion is not yet built; only the Parquet writer exists. Turso runs in-process with three DB files. — LABS:docs/requirements.md R276 (2026-10-13)
- Analytics port: `AnalyticsAdapter{name, health, query(Query)}` with a typed `Query` enum (`AiCost`, `EditorFriction`, `Timeline`…), not SQL strings; `RawSql` is an adapter-optional escape hatch. — LABS-H:phase3 §3.7 (2026-07-29)
- Redaction law: analytics consume redacted, versioned projections; credentials never reach any store. Scrub before hashing. Classification stores "the class, never the value"; HMAC handles identity without recovery. — LABS-H:phase3 §2.1a; LABS-H:signal-ledger-and-lesson-foundation §9.4–9.5
- Wide events over metrics for AI workloads; "for a local desktop app where the user IS the deployment, cardinality is free"; OTel is a view, not the write path. — PD:ADR-002 D8 (2026-04-15); Drive: "adr-turn-identity.md" D8
- Journal store per era: SQLite WAL in the Node era, fjall in the daemon era, with migration gated by a differential replay test. — Drive: "adr-turn-identity.md" D7 (2026-08-13)
- RRFlow (control plane) is dedicated AI persistence (Postgres behind a store trait) holding tree and revisions, rules, exemplars, journal, verdicts and model outcomes per address. It is bitemporal. Promotion runs against RRFlow, not inside it. — Drive: "LFG_NOTES_v3" §11 (2026-09-26, LOCKED role)
- LFG kernel is deterministic (same input plus same snapshot gives byte-identical output). Snapshot is heap-loaded with an atomic ArcSwap; tier-1 p99 target is < 1 ms. — Drive: "LFG_NOTES_v3" §2, §6; "LFG_ENGINE_PLAN" (2026-09-26)
- Schema source of truth is Rust-first: serde plus schemars produce JSON Schema, from which ArkType/Pydantic are generated. Shared fixtures must pass in all three. — Drive: "LFG_NOTES_v3" §7 (2026-09-26, PROPOSED)
- Agent-agnostic seam: thin provider hook stubs (Codex, Claude) normalize to a canonical `PromptSubmitted` event, then an agnostic seam, then a shared `EvaluationModalSpec` rendered conversationally and as a UI modal, then a pure reducer folds the validated event stream. — Drive: "Shippin — Agent-Agnostic Seam…" (2026-09-18)
- Older sync design: local-first memory with sync intake plus async heavy writes; enterprise sync via an outbox with checkpoints and idempotent apply to team Postgres/ClickHouse. — ALPHA:docs/architecture/adr-005, adr-010 (2026-05-08)
- RRO/RRD: in-process daemon, not a sidecar; "Cortex calls RRO. RRO does not call Cortex." — PD:scripts/docs/specs/cortex/rro/RRO.md (v2.0.1, 2026-05-20), RRD.md (2026-05-01)
- Shared telemetry crate is consumed by both the DevPulse desktop and `clyffyos` (headless daemon) via a git dep. — TEL:README.md (2026-06-28)

## (f) Product naming and positioning

- RRO is "also the standalone product sold via rroflow.ai to enterprise". The standalone test: "if you can sell a piece of functionality to enterprise (rroflow.ai) without the buyer also wanting Council/Fox/COSTAR+IC/minion personas — it belongs in RRO". Crates `kernel/rro-*` would be publishable to crates.io "for rroflow.ai customers". — PD:scripts/docs/specs/cortex/rro/RRO.md (v2.0.1, header 2026-05-20, last commit 2026-06-17)
- RRO vocabulary lock: RRO = "Reason Ready Object(s)"; "RRO = Orchestrator" is retired. — PD:RRO.md (2026-05-20)
- RRO reframed from "deep reasoning system / retrieval core" into a classification compiler, with first market "vibecoders — an environment where they grow into real developers". — Drive: "RRFlow — Classification Compiler" §1 (2026-09-24)
- Component names in prose: LFG, RRFlow, Dalal, Chatmaton, DevForge, Clyffy, Fox; never "the broker" or "the router". "Broker" became Dalal and "Judge" became Governance. — Drive: "LFG_NOTES_v3" §0, §12 (2026-09-26)
- Competitive notes: "Jev proves the market… Jev is one tier". Its gaps (hand-authored categories, no chaining, hosted, no teaching, no promotion loop) are described as RRFlow's moat. — Drive: "RRFlow — Classification Compiler" §4 (2026-09-24)
- Clyffy is "translator, not generator"; DevPulse "competes on success rate, cost protection, and architectural depth", not speed. — PD:scripts/docs/PHILOSOPHY.md (2026-04-19)
- Free-to-paid funnel: open-source `angryviber/claude-survival-kit` (verify-gate, psych-hook, learn-loop, hook-gen, claude-audit, context-pulse…) up-sells to DevPulse ("want them to talk to each other, remember your project, and get smarter over time?"). AngryViber blog angles include "Claude Code has a 30% lie rate". — PD:docs/claude.kb/build-queue/BUILDABLE_TOOLS.md (2026-03-31)
- Competitive gap on frustration: Claude Code makes it worse (apologetic mode); Cursor, Copilot, Bolt and Lovable don't detect it. — PD:EMOTIONAL_TRANSLATION_LAYER.md (2026-03-31)
- Naming laws: MAESTRO/Meistro/Gus are dead; Scribe was renamed Translator; the Council has 9 seats; "Minion" is the process term. — PD:LOCKED_DECISIONS.md §D (2026-05-13); PD:PHILOSOPHY.md
- Hosts in config: `rrflow.ai` is listed as a project site (PD:project.manifest.yml); `clyffy.rrflow.ai` and `api.rrflow.ai` sit behind Traefik/Authentik (ALPHA:docs/hosting/host-ownership-map.md, 2026-05); `oauth.rrflow.ai/mcp` is live (TPL:docs/ledger/poam.md, ~2026-07-29).
- No repo or Drive doc found describes rroflow.ai as "Sentry for your AI", or mentions Raindrop. The only rroflow.ai positioning in the sources is RRO-as-enterprise-substrate.

## (g) Superseded, outdated, and dated items

- DuckDB as the LABS analytics engine (R219, R239 of 2026-07-29) was superseded by R241 on 2026-08-03 (Arrow/DataFusion over Parquet). — LABS:docs/requirements.md; LABS:adr-014 amendment
- The "C libraries are subprocesses" claim was a prior-agent error and is not law; only ffmpeg *bindings* are denied. — LABS-H:phase3 §2.2 (2026-07-29)
- `ProviderEvent` was auth lifecycle only and chat crates had zero fabric calls, so "you cannot aggregate a sentence" (state as of 2026-07-29). `AiEvent` and usage adapters then landed 2026-08-03. — LABS-H:phase3 §1; LABS:crates/app/src/ai_usage.rs
- PD storage history: libSQL → Turso (owner 2026-07-01, "actually turso not libsql"). SurrealDB was "composed in, not forked" (05-13), then fork-and-own approved as RRFlow (2026-10-01), then upstream binary in a subprocess until RRFlow replaces it (2026-10-24). — PD:LOCKED_DECISIONS.md A-5, §I
- ALPHA ADR-009 (2026-05-08) named Postgres for transactional state, now superseded by Turso in PD and LABS.
- Normalizer spec storage (DuckDB training pairs, Turso `translation_memory`, 2026-04-06) has dead tables: plan 0004 C1 drops `attunement_*`, `translation_memory` and `expertise_level` in favour of a profile store (skill, lexicon, FSRS mastery). — PD:.plan/0004 (2026-10-12)
- PHILOSOPHY pillar 3, "Owning the forward pass is the moat" via Candle embedded (2026-04-19), conflicts with the current CLAUDE.md "no new compiled-in inference; local runtimes are supervised subprocesses" (2026-10-24, plan 0007).
- PD claude-integration API lanes with `cost_cap_daily_usd` and reqwest API keys (2026-04-06) predate the LABS subscription-only/no-API-key law (2026-07-23).
- LFG v1/v2/v2.1 superseded by v3 (2026-09-26): Judge moved into Governance, there are 5 operations not 3 answer types, Null became Gate, Broker became Dalal, the key excludes `@rev`, the snapshot is heap-loaded not memory-mapped, and version pins gave way to a latest-stable policy. The 177 rrd-core/rrd-lsm tests are "unit-only… not evidence that anything works end to end".
- Gemini CLI on consumer subscriptions ended 2026-06-18. The R280 "not yet: Antigravity" note was superseded the same day by R281. — LABS:docs/requirements.md
- Codex depot version gate `>=0.145.0,<0.146.0` caused a 400 error against the operator's 0.163.0 config (2026-10-19). An in-place, SHA-verified tool-update path is still missing. — LABS:docs/requirements.md R278, R283
- The 'baseline optimizing' goal in R213 (2026-07-25) "was never defined". — LABS:docs/requirements.md
- Claude Code countermeasure data comes from the v2.1.88 leak (2026-03-31). The current operator CLI is 2.1.296 (R283), so CC-001…CC-014 numbers may be stale.

---

## Conflicts and superseded (summary)

1. **What rroflow.ai is.** PD:RRO.md (2026-05-20) positions rroflow.ai as the enterprise channel for RRO, the reasoning/retrieval substrate. Drive notes (2026-09-24/26) reframe RRO/RRFlow as a classification compiler plus AI persistence (LFG, Governance, journal, promotion). Neither matches the current "Sentry for your AI" (capture, signals, spend, learning) framing; the closest pieces are the LABS AI ledger and usage adapters, and the Governance/journal/promotion design. The newest positioning needs its own canonical statement.
2. **Analytics engine.** PD keeps DuckDB (CLAUDE.md, 2026-10-24); LABS removed it for DataFusion over Parquet (2026-08-03), and DataFusion is still unbuilt. Two live repos disagree.
3. **Durable store for learning data.** Turso (PD/LABS) vs Postgres behind a trait (RRFlow, Drive 2026-09-26) vs SurrealDB (PD graph) vs fjall (journal daemon era, ADR-0001). Drive's open item "RRFlow vs Connectome / Qortex / Turso" is still unresolved.
4. **Learning mechanism.** April 2026: nightly LoRA adapters on 0.6B encoders from DuckDB pairs. September 2026: no fine-tune until thousands of journaled examples, with learning promoted into rules first and per-node classical classifiers second. The newer notes win.
5. **Inference location.** Candle in-process ("own the forward pass", April) vs endpoints and subprocesses (October). October wins.
6. **API keys.** Older PD lanes assumed API keys and daily USD caps. LABS law is subscription-only, and DevPulse never holds a key (Gemini on a paid API key is the one sanctioned exception). The Dalal doc also flags subscription routing through a third party as a terms risk.
7. **Persona and component names.** PSYCH/SCRIBE/TEACHER/ENFORCER/GUARDIAN/MAESTRO (March–April) vs Council seats Fox/Translator/Diary/Warden/Quincy (May) vs LFG/Fox/Dalal/Governance/RRFlow (September). Warden is "never a model", and Judge became Governance.
8. **Frustration-only vs bidirectional mood.** Early specs are negative-only (RAGE…NONE plus a `pos` flag). The owner's correction calls for a full positive-to-negative weight scale.

## Sources scanned

**Repos** (cloned shallow, read-only, into /home/claude/devpulse-research/):
- platform_devpulse — OK (HEAD 008546f, 2026-10-24). Read: docs/claude.kb/* (index, consolidated reference, emotional translation, tiers, buildable tools, normalizer spec, countermeasures, diff, prompt-extraction headings), scripts/docs/adr/ADR-002, scripts/docs/consolidation/LOCKED_DECISIONS.md, scripts/docs/specs/cortex/rro/RRO.md + RRD.md, scripts/docs/PHILOSOPHY.md, sprint docs (signals golden pattern, claude-integration S1/S2/POAM, dogfood day one), systems/devpulse-engine/ip/signals.registry.yaml, .plan/0001 and 0004, CLAUDE.md.
- devpulselabs — OK (HEAD 530dd17, 2026-10-25). Read: docs/requirements.md (§6 and §8 sections), docs/plans/product-vision.md, chat-provider-runtime-adr.md, docs/design/signal-doctrine.md, adr-014, docs/ops/references/signals-schema.md, crates/fabric, ledger, chat/store, chat/core, app/ai_usage, app/ai_backfill. Also, from tag `docs-history-2026-10-12` via the API: phase3-analytics-program.md, signal-ledger-and-lesson-foundation.md, coach-lab-experiment-program.md (fetched to /home/claude/devpulse-research/_history/devpulselabs/; providers-presence-and-the-brain, unified-ai-kernel-automaton-merge and workbench-experiments were fetched but not read).
- devpulse-telemetry — OK (HEAD 1f4c7d3, 2026-06-28). README plus crate layout.
- devpulse-labs — **empty repository** (no commits); nothing to scan.
- devpulse_alpha — OK (HEAD e57432f, 2026-05-08). docs/architecture ADR-004/005/009/010, hosting map.
- devpulse-template — OK (HEAD 89392de, 2026-08-17). docs/ledger/00-vision.md and rroflow grep. Mostly a Lovable/ColdLight template with little bearing on rroflow.

**Access notes:** `gh repo clone` failed (HTTP 403, GraphQL is blocked in this environment) and plain `git clone` failed (no credentials). All six repos were then attached with add_repo (read) and cloned with `git clone --depth 1`. Per-file dates came from the REST commits API. Nothing was modified or pushed. Secrets, .env files and node_modules were not read.

**Google Drive** (owner digi@angrygaming.org). Title searches for devpulse/telemetry/signals/rroflow/raindrop/vibecoder/learning matched only the folders DevPULSE_App and DevPULSE_News; the docs were found by full-text search. Read:
- "Shippin — Agent-Agnostic Seam, Hook Routing, and Evaluation Modal — Session Recovery Draft" (2026-09-18)
- "LFG_NOTES_v3" (2026-09-26)
- "LFG_ENGINE_PLAN" (2026-09-26)
- "RRFlow + LFG — Notes v2.1 Addendum" (2026-09-25)
- "RRFlow — Classification Compiler: Session Notes & Brainstorm" (2026-09-24)
- "adr-turn-identity.md" (2026-08-13)
- "CLYFFY_EBB_AND_FLOW.md" (2026-07-24)
- "CLYFFY_SPEC.md" (2026-07-24)

Found but not read: "RRFlow + LFG — Classification Compiler Notes v2" (superseded by v3), the Stack curriculum drafts (2026-09-30), "Domain Boundaries — user, admin, chat, AI" (2026-09-30), "shippin-canon-capture-2026-08-15.md", "CHANNEL_GOLDEN_PATTERN" (2026-08-07), and CLAUDE.md/LOVABLE_BRIEF/WIREDFRONT specs (2026-07). No Drive doc mentions Raindrop, Sentry or rroflow by name.
