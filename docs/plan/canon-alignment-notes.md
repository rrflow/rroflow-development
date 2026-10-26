# Canon alignment notes for the RROFLOW mechanism spec

Date: 2026-10-24. Basis: the four project docs named below, read in full. Purpose: fix the vocabulary, the existing types RROFLOW builds on, the types it still lacks, and where the spec lands in the decided build order, so the mechanism records cannot drift from canon.

Sources: `claude/compute-and-data-workload-classification.md` (WC), `claude/rrflow-capability-study.md` (CS), `claude/rrflow-build-order.md` (BO), `claude/rrflow-definitions-and-schema-audit.md` (DA). Canon rule (DA, decided 2026-10-17): the latest explicit statement wins.

## 1. Terms the spec must use (and never rename)

| Term | Canonical meaning | Source |
|---|---|---|
| RRFlow / RRD | Reason Ready Flow, the engine and platform; Reason Ready Daemon = `rrd-server` + embedded `RrdEngine`, the only writer (ADR-0001) | DA §2 |
| estate / rrflowDB | one project's database, served by one instance; an account owns many estates; a workspace is a set of estates grouped above the engine (Decision 1) | DA §2, BO |
| RRO | Reason Ready Object: a persisted, versioned unit of knowledge RRFlow reasons over (record, relation, claim, vector, reasoning node); the retired provisioning-role sense is gone (PR #5/#6) | DA §2, C9 |
| RROFLOW | the recursive self-improvement configuration in which RROs improve themselves; a plug-in configuration with no roadmap owner yet; needs its own record before Gate G | DA §2, BO |
| LFG | deterministic kernel of compiled rules (no model, golden-file proven) + configured extension on escalate: tier 2 encoder and per-node classifiers on CPU, tier 3 a small CPU model or a remote model, all endpoint-bound; every answer carries value, probability, confidence, tier, provenance; SWAG decides, LFG suggests (Decision 3) | DA Decision 3 |
| SWAG | the System One controller and model gateway: decides, selects, dispatches through configured endpoints; needs its own record before Gate G | DA §2 |
| Governance | verification checks, permission gate, proof pattern (`verification_requirement` on terminal reasoning nodes) | DA §2, C10 |
| memory | rrflowDB's records, relations and bi-temporal claims; no second database | DA §3.5 |
| flight | a prompt flight: strict lifecycle record + events in one runtime commit; `flight-launch/list/read`; exact replay by session+key; `observe` arm with fresh/pruned context at one read stamp (PR #17) | BO |
| prompt-flight use edge | `rrflow-prompt-flight-used`, identity `<flight-id>.<rank>`, carries rank, fused score, avenues; written in the flight's commit (PR #18) | BO, DA §8 |
| task | named task record `rrflow-task` (title, goal, status) + `rrflow-task-includes` edge from a flight; `task_id` on flights (PR #19, Decision 16) | BO |
| context packet | `ContextPacket` with read stamp, evidence, evidence digest, packet digest | CS §3 |
| endpoint binding / profile / hardware inventory | `rrd-contract/src/endpoint_binding.rs` (PR #7): eight closed workload classes; kinds local, self-hosted, subscription, API; credential = Infisical secret name, never a value; declared footprint and limits; revisioned, digest-bound profiles with ordered fallbacks | WC, BO |
| workload class 7 | learning: behaviour updates — traces → signals → issues → experiments → changes to prompts, skills, routines, configs; evidence canonical, classifiers derived; little GPU. Class 6 = weight updates. Raindrop is the named class-7 reference | WC |
| prompt graphs | lexicon, evidence, source, knowledge (= memory), governance, classification, reasoning, plan/task; the set is configurable | project memory, 2026-10-19 |

Collision watch: BO mentions "generated SDK/signal surfaces" — the repo may already use "signal" for a generated surface. Verify in `rrflow-development` before the spec adopts Raindrop's "signal" unqualified; if it collides, the spec's term is **behaviour signal**.

## 2. Existing types RROFLOW builds on (do not reinvent)

- `Claim` with `valid_from`, `valid_to`, `tx_time`, `Tier`, `PromotionState` (`rrd-core/src/claim.rs`) — the shape for memory consolidation; nothing runs it (DA §3.5, §7).
- Flight record + events + use edges + task edges (PRs #17–#19) — the evidence graph of what a prompt used; `flight-read` cost is the flight's own size.
- Events key family 0x1f; commit/changefeed stream; audit chain (two control-journal entries per audited op — retention item) (DA §3.5, BO).
- Reasoning tree contract (A-02): nodes, typed edges, recipe, cursor, decision evidence, `verification_requirement`; frozen fixture, no persisted execution (DA §5).
- `EstateConfiguration` revisions (install-time only; step 2 adds edit/validate/activate/rollback) and the config-home rule: master copy lives in the instance store as UI-edited, versioned revisions; TOML is import/export (WC).
- Gate I (unbuilt): triggers, durable routines, skills; I-07 "reacts to recurring failure signals"; exit condition: automation explicit, bounded, replayable, removable, subordinate to `RrdEngine` — every self-improvement lands as a versioned, authorized, reversible change, never an automatic hook (BO).
- Gate G (unbuilt): G-04 commits recipe selection and branch navigation with the tree state (BO).
- Schema rule: `schemars` on `rrd-contract` → JSON Schema → OpenAPI, digest-pinned, five generated SDKs; schema-driven UI forms (WC, DA §3.7).

## 3. Types RROFLOW needs that do not exist yet

| Needed | Status | Note |
|---|---|---|
| Answer (Gate, Choice, Score, Extract, Compose; probability, confidence, tier, provenance) | missing (v3 §3) | required by LFG and SWAG; waits per Decision 5 |
| Address (stable `CategoryId` segments, revision, 64-bit key, alias table) | PROPOSED, not ratified (v3 §5) | no contract built on it yet |
| Journal record / rule source / promotion | missing | name collision with `Claim.PromotionState` (claim tiers) must be resolved by naming |
| Check / Evaluator / Verdict | partial (`verification_requirement` only) | Governance |
| PermissionScope | missing | depends on Decision 1 (done) |

Consequence: the spec's mechanism records are design-time records with explicit dependencies on these types; they do not pretend the types exist.

## 4. Where this lands in the decided order

- BO: "SWAG record and RROFLOW record: before Gate G" — deferrable, named stage. Current order: 1 recoverability (served path done; D-11 open) → 2 config revisions → 2a Parquet → 2b benchmark harness → 2c speed wins (E-01, E-02, catalogue direct read) → 3 ingestion (embedding backend first) → 4 answers via bound endpoints → 5 prototype UI **and its Raindrop wiring pointed at RRFlow's HTTP API**. Decision 14 inside that: per-prompt graph + tasks → E-01 → folder → vector DB → real provider flights.
- WC open item 3: "Deep study of Raindrop's loop for class 7 design." This spec is that item.
- DA §7 placements, already decided: memory consolidation = RROFLOW's first mechanism; per-node classical classifiers belong to LFG's extension after the journal exists; rule promotion from reasoning traces ("the slow path mints fast paths") = RROFLOW, needs Answer, Address and journal types first.
- WC mapping already on record: signals ≈ configurable metrics; experiments ≈ eval arms; traces ≈ evidence graph; issue definitions ≈ lexicon and reasoning graph content. Raindrop is already wired into the rrflow-hydradb prototype (real events + feedback, read-back verified, TEAM.md 2026-10-12).

## 4b. What the repo already defines (checked on `main` `94d0567`, 2026-10-24 — PR #116; the project docs above stop at PR #28)

Sparse checkout at `canon/rrflow-development` (docs + `rrd-contract` + `rrd-core`). These are canon and the spec builds under them:

- **RROFLOW is defined** (`docs/architecture/system-overview.md` terminology row): "committed reasoning outcomes, verdicts, and corrections propose better rules, exemplars, claims or model candidates; each promotion is an authorized, versioned, reversible engine change, never an automatic hook. Unimplemented." Bound to G-06 (candidate qualification) and I-03/I-07 (governed improvement).
- **Learning contract exists**: `docs/reference/inference/learning/controller-training.md` (sources reviewed 2026-10-20). Loop: Outcomes → Candidate (rules/examples/model) → versioned Eval → authorized Shadow → exact Promotion proposal → `RrdEngine` admission + receipt → Active → Outcomes. Five stages with required evidence: observe/baseline, supervised improvement, unsupervised discovery, recursive improvement, promote/monitor. Hard rules: consent for training is separate from telemetry ("No dataset exists merely because a prompt flight was recorded"); Python cannot own runtime state or promotion; no second event store ("reuse the existing task/flight and causal evidence boundaries"); clustering is hypothesis, never verdict; held-out sets are versioned and never reused for tuning; replay-for-inspection ≠ re-invoking models. Raindrop Workshop and Arize Phoenix are cited as informing H-05/H-06 projections.
- **LFG/SWAG contract exists**: `docs/reference/inference/lfg-swag.md`. LFG = optional CPU analysis producing versioned observations (complexity, language, modality, task category) with declared units and confidence only when calibrated; deterministic rules admissible, learned classifier optional with pinned manifest. SWAG = policy over authorized endpoint candidates (modality, capacity, locality, quality floor, price ceiling, deadline, health) → proposal → engine recheck → dispatch → receipt. User-selected effort budgets (quick/balanced/thorough/custom) are required runtime behaviour. Note: the system-overview LFG row still says "whether a model runs inside it is undecided", lagging Decision 3 — a doc correction, not a conflict.
- **Prompt graph families are canon (H-09)**: `docs/reference/data/rrflowdb-capability.md` — lexicon, classification (Answer nodes with value, probability, confidence, tier, provenance), reasoning, plan/task, evidence (flight-used edges live), source, knowledge (= memory), governance (checks, verdicts, permission decisions). Definitions live in the schema registry; activation per project in the estate configuration revision; **experiments** = two or more arms (activation revisions), assignment rule, rubric, minimum sample, threshold; each flight records assignment, exposure and outcome as separate events; promotion = authorized reversible configuration revision.
- **Gate I RROFLOW packages** (`docs/roadmap/rrflow-1.0/gate-i.md`): "I-07 learning candidates" (a repeated failure proposes a bounded rule/example/model experiment with source and consent identities; clustering never a correctness verdict) and "I-03 RROFLOW promotion" (race two promotions, accept one; kill/restart matrix; duplicate receipt cannot deploy twice; rollback = new compatible revision). I-01 canonical engine-event envelope; I-02 triggers; I-03 routines; I-04 host-event adapters (Claude/OpenAI adapters, no automatic hooks).
- **"Signal" is taken.** `docs/reference/protocol/signal-catalogue.md` is the generated H-05 telemetry catalogue (diagnostic levels off/normal/detailed/profile; bounded metric dimensions; cardinality limit 2,000). Raindrop's "signal" (a learned behaviour classifier) therefore needs its own term in the spec. **D0 resolved by fact: use "behaviour signal"** (or a better name Jesse picks) and never bare "signal".
- **Still absent in the repo**: Answer and Address types, verdict/correction/dataset record kinds, any classifier representation, the experiment record, the engine-event envelope (I-01). The spec's records name these as dependencies, not as existing.

Consequence for the spec: it is the mechanism-level fill beneath the learning contract — for each Raindrop mechanism, what canon already covers, the gap, the proposed record in which family, the owning gate/package, and the fact Raindrop supplies that canon lacks. It does not restate the contract.

## 5. Standing constraints every mechanism record must satisfy

1. Nothing hardcoded beyond overridable defaults; today's hardcoded RRF constants (k = 60; weights 2.0/1.0/1.0/0.5) are the pattern to avoid, not follow (DA §3.7).
2. Every model and classifier reached through an endpoint binding; local vs remote changes no behaviour (WC).
3. Every change lands as a versioned, validated, activatable, rollback-able revision (WC "UI-managed"); automation subordinate to `RrdEngine` (Gate I).
4. Real instead of synthetic: planned states are labelled, never simulated (Jesse, 2026-10-18).
5. Built for scale: millions of records/vectors per estate; cost must not grow with estate history (POAM-028 is the anti-pattern).
6. Schema first: new types in `rrd-contract` with `schemars`; no standalone schemas outside it.
7. One PR per step, CI green, one journal under `docs/evidence/change-journals/`.
8. An ADR is canon only when saved through the interactive component.
