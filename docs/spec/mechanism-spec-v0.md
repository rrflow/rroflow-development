# RROFLOW mechanism spec v0

**Status:** design-time proposal, 2026-10-24. Not canon. Each numbered record below is a candidate ADR; it becomes canon only when reviewed, edited and saved through the ADR component and then landed in `rrflow/rrflow-development` as a pull request with its journal.
**Owner it fills in under:** the learning contract `docs/reference/inference/learning/controller-training.md` (loop: Outcomes → Candidate → Eval → Shadow → Proposal → `RrdEngine` admission → Active), the LFG/SWAG contract, the rrflowDB capability record (prompt graph families, experiments), Gate I's two RROFLOW packages. This document restates none of them; it fills the mechanism level beneath them with what Raindrop and REA show works.
**Basis:** `rrflow-development` `main` `94d0567` (PR #116, 2026-10-24); `sources/raindrop/raindrop-source-ledger.md` (228 sourced claims); `sources/rea/rea-source-ledger.md`; `sources/prototype/prototype-raindrop-ledger.md`; `canon/alignment-notes.md`. Every Raindrop or REA fact below is in those ledgers with a source; nothing is quoted from a logged-in workspace yet (live capture pending).
**Reading rule:** the latest explicit statement by Jesse wins over anything here.

---

## 0. Boundary and glossary

**Who does what.** RROFLOW **mints** (proposes definitions, candidates, findings). LFG **executes** the cheap tier (per-node classifiers on CPU, Decision 3). SWAG **decides and dispatches** model calls when a judge is needed. Governance **gates** every verdict and promotion. `RrdEngine` **admits and commits**; nothing else writes. Raindrop's own split is the same shape: a classifier is built once by a model-backed agent and then executed deterministically millions of times; ambiguous cases alone reach a model.

**D0 — the term.** `signal` is taken: in RRFlow it is the H-05 telemetry catalogue (`docs/reference/protocol/signal-catalogue.md`). This spec uses **behaviour signal** for Raindrop's "signal": a versioned classifier definition that labels flights with a named behaviour. Candidates if Jesse prefers another: *lens*, *behaviour rule*, *verdict rule*. Decision pending; all other records use the placeholder.

**Term map (Raindrop → RRFlow).** Keep the right column in all records.

| Raindrop | RRFlow | Note |
|---|---|---|
| project | estate (rrflowDB) | both isolate models and data per unit; Raindrop: "a model trained in one project does not leak into another" |
| org | account | account owns many estates (Decision 1) |
| user (`user_id`, traits, plan) | seat | Raindrop silently never ships an event without `user_id`; a flight must carry its seat |
| event (one turn: input, tool calls, output) | prompt flight | Raindrop's pricing unit is also one turn incl. tool calls and sub-agents |
| conversation (`convo_id`) | session (and task, when the prompts serve one named goal) | tasks group flights by goal; a `convo_id` groups by thread — both are needed, they are not the same relation |
| trace / span (`LLM_GENERATION`, `TOOL_CALL`, `INTERNAL`, `SYSTEM_PROMPT`) | flight stage events (vocabulary to extend, R1) | one temporal graph; never a second event store |
| property, feature flag | flight properties; experiment assignment/exposure events (H-09) | |
| signal (type `code`/`topic`/`regex`/`instrumented`/`metric`) | behaviour signal (R3); explicit verdict (R6); lexicon label; H-05 metric | one Raindrop noun splits into four RRFlow kinds |
| preset signal | bundled behaviour signal revision | shipped as the initial configuration revision, overridable |
| tool-error signal | derived behaviour signal over `TOOL_CALL` error events; deterministic, no model | |
| label signal (`sentiment: OTHER`, e.g. `intent_reorder`) | classification-family Answer (LFG) | describes, does not grade |
| stumble | one-flight finding (R5) | |
| issue | distribution-shift finding (R5) | |
| topic | lexicon-family label | |
| dataset (labelled snapshot) | labelled evidence set (R8) | snapshot at add time ≈ read stamp |
| experiment (cohorts of logged events) | cohort experiment (R7); the H-09 experiment covers arm assignment | |
| replay / simulation | governed replay (fresh work, R7) | the contract already separates inspection replay from re-invocation |
| Workshop (local trace viewer + MCP) | Connectome flight lens + `rrflow-mcp` | |
| write key / query key / MCP OAuth | endpoint binding credential reference; RRD session grants | secrets are Infisical names, never values |

---

## 1. Evidence substrate — what a flight records versus what a classifier needs

**Raindrop's unit of evidence.** One event = `event_id`, `user_id`, `event` name, `timestamp`, `ai_data{input, output, model, convo_id, tool_calls[]}`, `properties` (incl. SDK `$context` and `raindrop.app.commit_sha`), `attachments[]`, `is_pending`; plus OTLP spans linked by `traceloop.association.properties.event_id`. Server-side, an event row carries `tool_calls`, `tool_names`, `error_spans`, `token_usage`, `topics[]`, `signals[{id,name,score}]`, `user_traits`, `feature_flags`, `is_internal`, `is_excluded_user`. Spans carry `span_type`, `status`, `input_payload`, `output_payload`, tokens, `provider`, `model`, `raindrop.pricing.cost_usd`.

**What RRFlow records today.** A flight = lifecycle record + append-only stage events at one read stamp + `rrflow-prompt-flight-used` edges (rank, fused score, avenues) + `rrflow-task-includes` edge + context-packet digests. Provider arms (the real model generation) are deferred as governed opt-in adapters (PR #17). There is no model-generation event, no tool-call event, no system-prompt snapshot, no token or cost on a flight.

**Consequence.** Nothing in RRFlow is classifiable yet. Every behaviour signal in this spec needs flights that carry what the model saw, did and said. That is step 4 of the build order (answers through bound endpoints) plus provider arms, not a RROFLOW task.

**R1 — flight stage-event vocabulary.** Extend flight events (through the I-01 engine-event envelope when it lands; as flight events until then) with: `generation` (model identity, request/response payload references in the object tier, tokens in/out or explicit unknown, elapsed, status), `tool-call` (name, arguments and result references, status OK/ERROR, elapsed), `system-prompt` (digest + object reference, one per flight), `cost` (provider-reported or catalog-priced, with a coverage flag). Payloads above a configurable size live in the content-addressed object tier and the event holds the digest. The SWAG `Result` row in the LFG/SWAG contract already lists model/runtime identity, usage references, elapsed and outcome; R1 is that row persisted as flight events.
Facts that set the bar: Raindrop caps an event at 1 MiB and text fields at 1,000,000 characters with an explicit truncation marker; drops a finalized event with empty input and output (phantom guard); enriches asynchronously (topics, signals, error spans may lag); drops spans lacking a GenAI/AI-operation attribute. RRFlow's equivalents are configuration, not constants: payload cap, truncation marker, "what makes a flight classifiable" declared in the family definition.

**R2 — one store.** Raindrop's `traces` table is what RRFlow's events key family plus flight edges already are. No trace store, no span table, no SQLite sidecar. Connectome reads flights through `flight-read`; cost is the flight's own size (PR #18).

**Retention.** In the only controlled study of REA (community, rea-lab), agents proactively kept Evidence in 0 of 339 runs, so retention has to be designed in. Raindrop's Hobby tier keeps 14 days, and eval-run traces carry an expiry. The rrflowDB record lists "default retention per family" as an open decision; R1 adds the generation/tool payload families to that decision with a stated default and a per-estate override.

---

## 2. Behaviour signal minting — the supervised loop

**What Raindrop does (public).** Describe the behaviour → an agent studies traces and writes numbered policy rules → compiles a classifier → scans a sample → a review panel shows matches one at a time with a reason; the person labels Match / Not Match on a *sample, not a checklist* → "Run Again" folds labels into the rules → optional "Preview classifier code" → create → runs on every new event and backfills 90 days, sampled. Over MCP: `create_signal(intent, ≤25 positive/negative ids)` and `refine_signal(signal_id, ≤500 ids, comment)`, headless. Signals 2.0 (rd-signal-2): the compiled classifier is `run(event)` code that gathers evidence deterministically (e.g. find tool calls by name, test for three identical-input failures), early-returns non-matches without any model call, and hands only ambiguous candidates with compact context to a task-specific classification head plus an in-house reasoning model. Runs in an isolated per-customer sandbox with no credentials or egress; trace context fetched once and cached near workers; median 100 ms per trace; over 20 billion traces a month. Daily audit of a random sample against a frontier model; regressions retune. Customers can read and edit the code and prompt. Match-rate sensitivity: four one-sentence phrasings of one behaviour over 2,000 traces matched 0.9–4.6 %, and 67 % of traces matched by one phrasing were rejected by another. Reported benchmark: precision 71 % / recall 87 % at 1× relative cost versus 78 % / 91 % at 260× for the best frontier judge.

**R3 — behaviour signal definition.** A schema-registry definition (same mechanism as a prompt graph family) with: name; description; scope (`turn` | `conversation` | `task`); polarity (`negative` | `positive` | `label`); the numbered policy rules as text; the **evidence program**; the **judge binding**; seed and verdict references; revision and digest; provenance (who minted, from which finding or request). Activation is per estate in the configuration revision, like families. Bundled presets (User Frustration, Task Failure, Forgetting, Refusals, User Praise are Raindrop's) ship as the initial revision.

**R3a — the evidence program is rrflowQL, not JavaScript.** Raindrop generates JS over an `event` object. RRFlow's flight is already a graph; the deterministic gathering step is an rrflowQL read at the flight's read stamp over the evidence, classification and R1 stage-event families, with a predicate. Empty result = deterministic non-match, no judge call. The sandbox is the engine's existing `QueryBudget`, read-only authorization and no egress; the cache Raindrop builds near its workers is the flight's own neighbourhood, already materialized by `flight-read`. This is the RRFlow-native form of "compile once, run cheaply" and it is replayable at any historical stamp — Raindrop's `run(event)` is not.

**R3b — the judge is an LFG tier-2 classifier, escalating through SWAG.** `classify()` maps to Decision 3: a per-node classifier on CPU (class 2 binding) bound to the behaviour signal's revision, escalating to a class-1 endpoint only when confidence is below the configured floor. Its output is an Answer (value, probability, confidence, tier, provenance) — the type v3 §3 names and the repo lacks. Raindrop's per-match numeric `score` is probably this probability (inferred; range and threshold unpublished; observed null on instrumented signals). Confidence is reported only with a declared calibration (LFG/SWAG contract), and escalation goes to the tier-3 binding SWAG selects, not a fixed endpoint.

**R3c — verdicts are Governance-family records.** `{flight, behaviour signal revision, label: match | no_match | skip, labeler seat, read stamp}`. Labels are given by a person or an explicitly bound evaluator, never inferred from aggregates (Raindrop's skill rule; RRFlow's contract). Bounds default to Raindrop's (25 seeds at creation, 500 at refinement) and are configuration.

**R3-consent — minting reads only a labelled evidence set.** Collecting flights does not authorize training on them (learning contract). R3d's scan, R3c's verdicts, the R3b judge's training and R3e's audit samples all read an R8 labelled evidence set that carries consent identity, sharing scope and a split manifest. A flight outside every consented set is never sent to a judge endpoint or used as a training row. An audit sample sent to a remote frontier endpoint also needs that set's sharing scope to allow the endpoint's locality.

**R3d — minting is an I-03 routine.** Steps: draft definition (judge-model-backed through SWAG, budgeted) → scan a bounded sample of flights at one read stamp → review (verdict records) → refine (re-draft folding verdicts in) → create as a definition revision → backfill as a separate bounded, sampled job. Every step is an engine operation with a receipt; the routine can be killed and resumed (Gate I corpus). Raindrop's phases `preparing → scanning → reviewing → installing` are the routine's checkpoints.

**R3e — drift audit is a scheduled routine.** Sample recent matches and non-matches, judge them with an eval arm bound to a frontier endpoint, write regression findings (R5 shape), propose a retune revision. Never auto-promotes (Gate I exit condition).

Depends on: Answer type; H-09 schema-registry definitions; I-01 envelope; I-03 routines; endpoint-binding runtime resolution (step 2 config revisions); R1.

---

## 3. Discovery — the unsupervised loop (stumbles and issues)

**What Raindrop does (public).** Three disjoint layers since Issue Detection v2: a **stumble** is one flagged run (what happened, why, key moments; tags such as Logic Error, Hallucination; a Similar panel groups alike stumbles), detected on a per-org scan cadence, not in real time. An **issue** is the same failure recurring across runs and users: severity critical…low, status unresolved/resolved/ignored, affected event and user counts, "affected dimensions" = tools, models and signals overrepresented in matched events, a timeline, a report, matching events. Actions: resolve, ignore with a reason (not accurate, too broad, too specific, show less like this), reopen, duplicate-merge; any issue can be turned into a signal. Built on the same classifier architecture.

**R5 — findings are Governance-family records.** Two kinds, disjoint by construction: `one-flight finding` {flight, tags, why, key-moment event refs, detector revision, status} and `distribution-shift finding` {member findings, overrepresented dimensions computed by rrflowQL facets (tool names, model identity, behaviour signals, properties), severity, status, trend window, report}. Status starts as `proposed`; a seat resolves, ignores (with reason), merges or promotes. The contract's rule holds: a cluster is a hypothesis, never a verdict.

**R5a — the detector is a scheduled I-03 routine** with a configurable cadence over flights since the last stamp: tag through a class-2 judge binding; group by exact dimension facets first, then by vector similarity of finding descriptions in the estate's own vector subsystem. Raindrop's `cadence_minutes`/`last_run_at` become the routine's checkpoint and the Connectome lens says "as of <stamp>".

**R5b — issue → behaviour signal.** Promoting a distribution-shift finding hands R3d its member flights as **candidates for review, never as labels**. Only flights a seat has given a `match` verdict become seeds (bounded by the seed limit). Cluster membership is a hypothesis (learning contract; Gate I I-07). A trigger (I-02) may *propose* this when a finding crosses a configured threshold; it cannot run it.

**Topics and intents.** Raindrop attributes `topics[]` per event server-side and uses label signals (`sentiment: OTHER`) for intents; its MCP integration injects a hidden `_intent` parameter the model fills on the first tool call of a prompt. In RRFlow these are lexicon-family labels and classification-family Answers produced by LFG — already placed by the rrflowDB record; no new kind.

---

## 4. Feedback refinement — explicit and implicit

**What Raindrop does (public).** Explicit: `signals/track` with type `feedback` (comment), `edit` (after), `default`, `agent` (self-report), `agent_internal`; sentiment POSITIVE/NEGATIVE/OTHER; optional `attachment_id`. Implicit: tool-error signals derived from `TOOL_CALL` errors; agent self-diagnostics through an SDK-generated tool (`__raindrop_report`) with categories `missing_context`, `repeatedly_broken_tool`, `capability_gap`, `complete_task_failure`; review labels and `refine_signal` negatives; issue triage verdicts; feature flags and properties for cohorts.

**R6 — explicit verdict events.** Feedback, edit and self-report arrive as engine events (I-01 envelope) through Connectome, DevPulse, or a host-event adapter (I-04: Claude/OpenAI adapters, explicitly installed, never hooks). The self-report tool is an engine-provided tool definition the agent may call; its categories are configuration with Raindrop's four as the initial revision. Each verdict is a Governance-family record on the flight.

**R6a — derived behaviour signals need no model.** Tool errors: an rrflowQL predicate over R1 `tool-call` events with `status: ERROR`, shipped as a bundled definition. Repeated identical-input failures (Raindrop's canonical example) is the same shape with a count predicate.

**R6b — refinement re-mints.** Negative and positive verdicts feed R3d as examples; the new definition is a new revision; the old one stays readable. Knowledge corrections take the claim path the kernel already has (`valid_to`, tier, `PromotionState`, supersession by digest) once `AssertClaim` can express them (the H-09 gap).

**Rule carried from the prototype (Jesse's team, 2026-10-12) and the contract:** a verdict is evidence about behaviour, not automatic authorization to change a rule, prompt or weight.

---

## 5. Experiments, replay and labelled sets — the recursive loop

**What Raindrop does (public).** An **experiment** compares a baseline cohort and an experiment cohort of already-logged events (date ranges plus conditions: model, feature flag, property, tool); outcome = all or selected signals, measured as events % or users %; verdict "likely better/worse" requires a 95 % CI excluding zero and a sample check; "Mixed" when outcomes disagree; event stats (median cost per event, tools, duration, errors). Experiments never run the agent. **Change Reports** read the agent harness on merge, scope traffic by `raindrop.app.commit_sha`, and report after ~24 h which changed tools were called, negative signals on them, related stumbles. **Simulations** (early access) replay production traffic against a proposed harness change on every PR. **Workshop** replay: a replay server with `GET /health` and `POST /replay` on a reserved port range, stitched to the original by setting the SDK event id to the replay run id; the eval loop has the coding agent write code-aware evals asserting on tool-call sequence, state and files. **Datasets** are labelled snapshots of events/traces (Good/Bad/Needs review/Interesting or custom labels), exportable as CSV.

**R7 — cohort experiment and verdict rule.** The H-09 experiment covers configuration arms with assignment/exposure/outcome events. Add a cohort experiment over logged flights with no re-run: two cohort definitions (stamp range + predicates over flight properties, model identity, tool names, task), outcome behaviour signals, unit flights % or seats %, and a **verdict record** carrying the method name, interval, sample check and result (`likely better` | `likely worse` | `mixed` | `insufficient`). The statistical method is declared configuration (Raindrop's is unpublished — D7). Promotion of anything on the strength of a verdict goes through the contract's Proposal → admission → receipt path.

**R7a — governed replay.** Re-running a flight's prompt against a candidate configuration is fresh work through SWAG with its own budget and receipt (the contract's rule), linked to the original by the cohort digest flights already share. The replay-server contract Raindrop publishes (`/health`, `/replay`, stitched ids) is the shape of an outward `rrflow-*` adapter for projects whose agent runs outside RRFlow.

**R7b — change attribution.** A flight is to carry the project-tree snapshot digest once D-03 lands (planned; flights have no such field today) and, when known, the commit; a scheduled routine compares behaviour-signal rates before and after a commit and writes a finding. Raindrop's Change Report is this routine's report.

**R8 — labelled evidence set.** A record holding flight/event references pinned to a read stamp plus labels, with the contract's required manifest: source references, estate and sharing scope, consent, retention, immutable split manifest with digests and row/exclusion counts. Export is a public operation (Parquet is already the optional exchange form). "No dataset exists merely because a prompt flight was recorded" stands: a set is created by an explicit operation with consent identity.

---

## 5b. What the live workspace showed (2026-10-24, Query API, read-only)

Source: `sources/raindrop/live/live-capture-ledger.md`.

- **Raindrop already ships the R3 artifact.** `raindrop-ai@1.0.2` defines a portable evaluator: `format raindrop.eval-program`, `runtimeAbi raindrop-eval-v3`, program source plus SHA-256, a pinned trace-projection hash, and `executionMode deterministic | judge`. It runs in QuickJS with 32 MiB memory, a 1 s program timeout, and judge calls through a host callback **the caller binds**. R3 matches that shape: the evidence program is rrflowQL instead of guest JS, the judge is an endpoint binding instead of a callback, and the identity is a digest pinned to a flight-projection version.
- **Classifier code is not readable through the Query API.** Signal detail returns metadata only. Score is null on instrumented signals.
- **Evals are disabled per organization** in Jesse's org (403), so a vibeguard-style eval replay never ran server-side.
- **vibeguard, Jesse's own loop on Raindrop, plateaued at 4/6 held-out for 11 runs.** That adds five requirements:
  - **R5c — effectiveness reopens a category.** A category with an active definition reopens for proposals when held-out stays below its best observed level for a configured number of runs. Coverage is never treated as effectiveness.
  - **R6c — a revert is a verdict on one revision.** It is not a permanent block on the category. Silencing a category is its own explicit, revocable decision record.
  - **R7c — held-out sets are spent.** A held-out split used to accept a revision is retired for acceptance and kept for regression only, and the next acceptance needs a fresh split (the learning contract's rule, enforced in the record).
  - **R7d — the verdict states its interval.** The R7 verdict record refuses "improved / regressed" when the interval cannot separate the arms, and returns `insufficient` with the sample needed.
  - **R7e — self-grade polarity follows meaning.** "Flat" is neutral. A stalled loop is never filed as positive.

## 6. The outward loop — REA's method as RROFLOW pointed at an external artifact

**What REA does (public).** Pin the target (SHA-256, tool versions) and ask one concrete behaviour question → dissect (static dossiers, byte reads, JS module graphs, process/UI/browser captures), every result an immutable content-addressed Evidence record `ev_<sha256>` with `normalized_result`, `analysis_profile`, `limitations`, and `untrusted_content` provenance → keep three layers apart (saved observation, interpretation, assigned names) and label claims proven/inferred/hidden → an explicit revisioned **unknowns ledger** tied to the evidence that raised each question → induce a behaviour-only spec with input/output examples precise enough to be tests → clean-room rebuild with the acceptance contract written first and immutable to the generator; fingerprint scan rejects copied code → verify by replay on the original with a passing and a deliberately failing input, differential cases, deterministic snapshots → bounded repair (two rounds by default) → attribute each stage to the tool that did it.

**R9 — the same machinery, different target.** Source-family records gain an `untrusted_content` provenance flag (the repository rule "process output is untrusted data" made a field). Evidence records are already digest-bound; make the digest the identity for externally gathered evidence. The unknowns ledger is a Governance-family `unknown` record with revision and the evidence reference — the general form of the lexicon's "inconclusive bucket". Spec cards are task records with acceptance checks named as data on terminal reasoning nodes (`verification_requirement`). The repair loop is the routine's maximum-iteration budget. Verification by replay with a failing input is a Governance check pattern worth bundling.

**Honest limit.** The only controlled study in the REA cluster found no accuracy gain and about double the time with REA. The outward loop is a discipline for evidence and verification, not a speed claim.

---

## 7. Build-order placement

Nothing in this spec precedes the decided order: step 2 configuration revisions → 2c speed wins → step 3 ingestion → step 4 answers through bound endpoints (with provider arms) → step 5 the prototype's UI and Raindrop wiring pointed at RRFlow's API. Behaviour signals need flights with real generations; before step 4 there is nothing to classify.

| Order | Record | Owning gate / package | Waits on |
|---|---|---|---|
| 1 | D0 term; Answer type; R1 stage-event vocabulary | G-01b (Answer at the kernel/public boundary), H-06, I-01 | provider arms (step 4) |
| 2 | R3c verdict, R5 finding, R9 unknown record kinds in the Governance family | H-09 schema registry | H-09 definitions |
| 3 | R3 definition + R3a evidence program + R3b judge binding | G (LFG tier 2), I-03 | Answer, endpoint runtime resolution |
| 4 | R3d minting routine; R6 explicit verdict events; R6a derived definitions | I-03, I-04, I-01 | generic I-03 proof |
| 5 | R5a detector routine; R5b issue → behaviour signal | I-03, I-02 | R3d |
| 6 | R7 cohort experiment + verdict rule; R8 labelled evidence set | H-09 experiments, knowledge-connector export | R3 |
| 7 | R3e drift audit; R7a governed replay; R7b change attribution | I-03, G-02, D-03 | R7 |
| 8 | R9 outward loop | Gate I corpus | all of the above |

**Raindrop's place now (D5).** A class-7 endpoint binding of kind `api`: an outward `rrflow-raindrop` adapter exports flights as `events/track_partial` plus OTLP spans and reads Raindrop's behaviour labels and findings back through its Query API or MCP, so a production classifier labels RRFlow flights before RROFLOW's own exist and the two can be compared on the same flights later. That is build-order step 5 made concrete, and it is the honest basis for any later comparison. Reference-only is the fallback if the binding is not wanted.

---

## 8. What Raindrop and REA supply that canon lacks (absorbed, with the record it lands in)

1. Compile once, execute cheaply: deterministic evidence gathering with early return; a model only on ambiguous candidates (R3a/R3b).
2. Label a sample, not a checklist; bounds 25 at creation, 500 at refinement (R3c).
3. Phrasing sensitivity is real (0.9–4.6 % match across four phrasings; 67 % disagreement): a definition must carry its rules as text and its revision, and comparisons must pin the revision (R3).
4. Daily audit of a sample against a frontier judge; retune as a revision (R3e).
5. Issues and stumbles are disjoint catalogs; detection on a cadence with an "as of" stamp (R5, R5a).
6. Ignore reasons as data (not accurate / too broad / too specific) feed refinement (R5, R6b).
7. Per-project model isolation, matched by per-estate definitions and judges (R3).
8. A numeric score per match (probability) rather than a boolean (R3b).
9. Label signals that describe rather than grade (`OTHER`) — intents separate from quality (term map).
10. Phantom-event guard, payload caps with explicit truncation markers, asynchronous enrichment made visible (R1).
11. Cost with a coverage ratio: never quote a total over unpriced calls (R1 `cost`, R7 event stats).
12. Change attribution by commit identity (R7b).
13. A replay-server contract for agents that run outside the engine (R7a).
14. A normalization layer across SDK span shapes into `NormalizedLLMSpan` / `NormalizedToolSpan` (Workshop) — RRFlow's single flight vocabulary makes this unnecessary internally and necessary at the adapter edge (R1, D5).
15. REA: content-addressed evidence, untrusted-content provenance, an unknowns ledger, observation/interpretation/name layers, contract-first clean-room verification with a deliberately failing input (R9).

---

## 9. Decision queue (one per pass)

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D0 | Term for Raindrop's "signal" in RRFlow | behaviour signal · lens · behaviour rule · verdict rule | behaviour signal — plain, scopes the H-05 collision away |
| D1 | Where generation and tool-call evidence lives | flight stage events (R1) · a new span record kind · derive from provider receipts only | flight stage events, payloads in the object tier |
| D2 | Evidence program language | rrflowQL + predicate (R3a) · sandboxed JS/Wasm like Raindrop · both | rrflowQL; Wasm functions exist if a predicate outgrows the query |
| D3 | Judge binding default | class 2 CPU classifier escalating to class 1 · class 1 only | class 2 with escalation; confidence floor is configuration |
| D4 | Who may promote a definition or finding | seat with Governance verdict · qualified policy for bounded auto-promotion · never automatic | seat now; policy later per the contract |
| D5 | Raindrop's place in the profile | class-7 `api` binding (export + read-back) · reference only · both | binding, so RRFlow is measured against production classifiers |
| D6 | Retention defaults for R1 payload families | per-family default + estate override · one estate-wide default | per family |
| D7 | Cohort verdict statistics | declared method in configuration (two-proportion interval as default) · adopt a library · leave to eval arms | declared configuration, method named in the verdict record |
| D8 | Term for REA-style outward loop | RROFLOW outward · "dissect" family · none yet | park until R1–R7 exist |

---

## 10. What this spec cannot claim

No behaviour signal can be minted today: flights carry no generations. No speed, recall or cost figure here is RRFlow's; every number is Raindrop's or REA's and is sourced in the ledgers. Live-workspace facts (classifier code shape, score semantics, issue grouping, review-panel counts) are pending capture; the P0 list is `sources/raindrop/live-capture-checklist.md`.
