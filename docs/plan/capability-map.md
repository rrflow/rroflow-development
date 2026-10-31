# AI capability baseline: Raindrop, Sentry, PostHog and RROFLOW

Updated 2026-10-28. Owner: this capability map. The minimum product target is the
AI-relevant capabilities of all three products, followed by enterprise datasets
and governed supervised/unsupervised learning. This is an implementation backlog,
not a statement that RROFLOW already has parity. No row is complete because a menu
item exists. Newly discovered capabilities must get a row and an acceptance gate;
this inventory is a maintained baseline, not proof of exhaustive product parity.

**Status:** tested = implementation with test evidence, partial = useful subset,
designed = owned contract without the complete worker/service, missing = not yet
implemented. All statuses concern development source. Texas deployment and real
member onboarding remain unverified. Test fixtures do not establish provider
compatibility or production scale.

## Sources and changed assumptions

- **R0:** [Raindrop documentation inventory](https://www.raindrop.ai/docs/llms.txt), inspected 2026-10-28: capture/integrations, platform, organization, query/MCP and privacy families.
- **R1:** [Signals](https://www.raindrop.ai/docs/platform/signals/): drafting, presets, instrumented checks, refinement, backfills and evidence navigation.
- **R2:** [Datasets](https://www.raindrop.ai/docs/platform/datasets/): collections of events/traces/uploaded rows, annotations and export.
- **R3:** [Dashboards](https://www.raindrop.ai/docs/platform/dashboards/): saved query panels and filter-preserving drill-downs.
- **S1:** [Sentry agent dashboards](https://docs.sentry.io/product/agents/dashboards/): overview/model/tool views and AI/full-trace navigation. Retrieved directly from the official site when the browsing extractor could not read its Markdown response.
- **P1:** [PostHog AI Observability](https://posthog.com/docs/ai-observability): trace/session context, costs, tools, feedback, replay/error links and query access.
- **P2:** [PostHog evaluations](https://posthog.com/docs/ai-evals): code/model/sentiment evaluations, backfills, trace reviews, datasets and offline experiments.
- **P3:** [PostHog product analytics](https://posthog.com/docs/product-analytics): trends, funnels, retention, paths, cohorts/identity, dashboards and experiment/flag correlation.
- **U1:** owner direction, 2026-10-28: these capabilities are a minimum for AI; clean drill-down dashboards, every signal, enterprise datasets and actual supervised/unsupervised evolution are essential.
- **H1:** all eight dated [Raindrop research notes](../README.md#research), plus the [DevPulse ledger](../research/devpulse/devpulse-research-ledger.md). Historical observations remain preserved rather than rewritten as current competitor claims.

The old comparison is superseded: Raindrop now documents coding-agent setup,
positive presets, dataset collections and custom dashboards. PostHog has online
and offline evaluations and datasets. These are baseline requirements, not gaps
we can claim competitors lack. RROFLOW's intended distinction must be demonstrated
through reliable source capture, account ownership, truthful coverage and governed
improvements. SDK, browser, mobile, managed-agent and subscription access are
separate surfaces; owning a subscription does not expose all its history.

## Capability inventory and acceptance gates

The source column names a capability family, not a claim that the products have
identical semantics. Supporting enterprise/platform requirements are explicit
RROFLOW obligations even where a competitor's detailed implementation has not yet
been investigated. No unverified competitor security claim is inferred.

| ID | Required capability | Basis | RROFLOW status / owning implementation | Evidence needed to close the gap |
|---|---|---|---|---|
| C01 | Account-linked, system-wide coding-agent capture | R0, U1 | partial: CLI sources/startup | Real member links each supported source; work across projects reaches the same account |
| C02 | Provider/framework SDK wrappers | R0, P1 | partial: JavaScript Claude/Codex/Grok adapters | Normalize SDK journals into shared turn/trace/signal views; real streaming, tool, cancellation and error compatibility per SDK/version |
| C03 | HTTP, OTLP and import interoperability | R0, S1, P1 | partial: events/turn HTTP APIs; no general OTLP receiver | Versioned mappings, identity and finished-span validation; round-trip fixtures and real imports |
| C04 | Browser/mobile/remote execution capture | R0, U1 | partial: source-side collector independent of dispatcher | Explicit coverage per app; remote execution identity, offline replay and phone-disconnect proof |
| C05 | Durable delivery, retry, health and loss visibility | U1, H1 | partial: queue, cooldown, startup, source reports | Disk-full/backpressure, bounded retention, sustained load and gap accounting |
| C06 | Nested traces, handoffs, tool catalog and timing | R0, S1, P1 | partial: turns/tools/subagents and SDK journal | Full correlated waterfall, available-tool schemas, causal links and clock uncertainty |
| C07 | Private content, scrubbed projections and diagnostics | R0, H1 | partial: opt-in text, redaction, doctor | Member-editable policies, versioned redaction, leakage tests across every capture surface |
| C08 | Activity search, sessions and projects | R0, S1, P1 | tested: filtered/paginated turns and inspector | Model/tool/user/release/property dimensions, saved views and scalable query indexes |
| C09 | Readable conversation/tool/evidence drill-down | R1, S1, P1 | partial: turn inspector and pinned reviews | Correlated multi-turn/multi-agent trace view with deep links, missing-span states and keyboard paths |
| C10 | Built-in, custom and historical signals together | R1, H1 | tested: signal library, scoped custom predicates, recorded-definition fallback | Versioning/edit/archive/import and access controls; no lost historical interpretations |
| C11 | Signal trends and agent/project drill-downs | R1, R3, S1 | tested in this increment: full-cohort stats API and evidence links | Broader model/tool/release facets and production-scale latency evidence |
| C12 | Signal drafting from natural language | R1, H1 | designed: tier-3 candidate mechanism | Sandboxed candidate generation, preview coverage, human review and held-out gates |
| C13 | Semantic/example-trained classifiers and positive/negative mood | R1, P2, H1 | partial: deterministic mood; semantic tier designed | Label provenance, uncertainty, per-version evals and measured precision/recall |
| C14 | Instrumented feedback and agent self-diagnostics | R0, R1, P1 | partial: native SDK event journal | Typed outcome/feedback events, provenance, explicit semantics; self-reports are not truth labels |
| C15 | Backfill and re-evaluation | R1, P2 | missing: new custom signals do not scan history | Durable bounded jobs, frozen policy/version/scope, coverage and restart/retry accounting |
| C16 | Issue grouping, triage, ownership and lifecycle | R0, S1, H1 | partial: findings, no issue lifecycle | Group fingerprints, assign/resolve/ignore/reopen, regression state and linked evidence |
| C17 | Alerts, thresholds, anomaly detection and notifications | R0, P3, H1 | missing | Versioned rules, deduplication, routing, delivery receipts, rate limits and permission checks |
| C18 | Overview, model, tool and performance dashboards | S1, R3, P1 | partial: overview and signal dashboard | Latency percentiles, first-token timing, model/tool health and evidence-preserving filters |
| C19 | Spend, cache usage and unknown-cost handling | R0, S1, P1, H1 | partial: raw optional token fields | Effective-dated price table, model census, cache semantics, cost coverage and corrections |
| C20 | Subscription windows and wasted-work accounting | U1, H1 | partial: usage-limit signal | Supported plan-window adapters; explicit attribution and unknowns; no subscription/API-billing conflation |
| C21 | Custom saved/shareable dashboards and visualizations | R3, P3 | missing | Persisted panel/query contracts, units, filter semantics, ownership/sharing and drill-down proof |
| C22 | Trends, funnels, retention, paths and cohorts for AI | P3, U1 | missing | AI task/outcome identity, comparable denominators, censoring and cohort definitions |
| C23 | Users/organizations and product-to-agent correlation | R0, P1, P3 | partial: account-owned devspaces | Scoped end-user identity, groups and traits with consent and deletion semantics |
| C24 | Session replay, exceptions, logs and external trace links | S1, P1, P3 | missing | Permission-checked correlation IDs, redaction, retention and useful cross-system navigation |
| C25 | Releases, flags, experiments and before/after comparison | R0, P2, P3 | missing | Exposure records, pinned variants/cohorts, uncertainty and reproducible comparisons |
| C26 | Investigation assistant, query API, MCP and safe A2UI | R0, P1, P2 | partial: REST queries and local allowlisted A2UI | Scoped read/write tools, query budgets, provenance and audited actions; no arbitrary UI execution |
| C27 | Scheduled briefs, change reports and root-cause/repair support | R0, H1 | partial: doctor repair text | Durable schedules, evidence-linked reports, controlled repo context and verified fix/revert loop |
| C28 | Shared curated datasets, uploads, annotation and export | R2, P2, U1 | partial: private metadata/review datasets and authenticated export | Real isolated collections, immutable snapshots, structured labels, safe import/export and lineage |
| C29 | Automatic dataset policies and durable build jobs | U1, H1 | partial: [consented metadata worker](../spec/metadata-datasets-v1.md), leases and revocation | Purpose/consent revision, cutoff, leases, idempotency, staging validation and atomic publication |
| C30 | Enterprise dataset access, audit and collaboration | U1 | missing implementation | Project/team RBAC, approval/audit history, retention, deletion propagation and explicit sharing |
| C31 | Dataset revisions, quality, provenance and split hygiene | U1, P2, H1 | partial: immutable metadata snapshots, digests and exclusions; qualification/splits open | Reproducible hashes, exclusions, frozen source revisions, deduplication and session/fork leakage tests |
| C32 | Online/offline code, model and sentiment evaluations | P2, R1, H1 | partial: deterministic detection and preview only | Versioned evaluator artifacts, budgets, typed results, coverage, backfill and experiment linkage |
| C33 | Review queues and outcome/correction labels | R1, R2, P2, H1 | partial: MATCH/NOT MATCH against exact snapshot | ACCEPTED/EDITED/REJECTED/ABANDONED with actual correction pairs, reviewer provenance and disagreement handling |
| C34 | Prompt/configuration versions and playground | P1, P2, H1 | missing | Replay-safe test inputs, pinned provider/configuration and comparable eval results |
| C35 | Supervised improvement | U1, H1 | designed | Consented journaled labels, bounded training jobs, holdout intervals, candidate evidence and monitored rollout |
| C36 | Unsupervised discovery | U1, H1 | designed | Eligible unlabelled data, reproducible embeddings/clusters, novelty review; clusters never become truth labels |
| C37 | Reversible promotion and recursive improvement | U1, H1 | designed: mechanism/RRFlow learning contract | Inactive candidates, independent held-out admission, versioned deployment, monitoring and revert receipts |
| C38 | Free/paid entitlements and quota behavior | R0, U1 | partial: [server-owned monthly allowances](../spec/account-allowances-v1.md), expiring grants, replay-safe enforcement and member status | Shippin entitlement synchronization, billing, rate/burst and retention policies remain open |
| C39 | Member source/key management and onboarding | R0, U1 | partial: auth, public entry, setup guide and milestones | Live signup, self-service source credentials/revoke, same-account readback and no manual operator key handoff |
| C40 | Organization/project RBAC, SSO and provisioning | R0, U1 | partial: isolated Nomen-owned devspaces | Shared platform identity contracts; roles, team/project permission tests and enterprise provisioning |
| C41 | Audit, retention, deletion, residency and export controls | R0, U1 | missing service controls | Owned policies, complete audit trail, deletion/revocation propagation and operational verification |
| C42 | Hosted scale, recovery and GitHub delivery | U1, H1 | partial: immutable artifacts/readiness/rollback fixtures | Verified Texas host, authenticated load/capacity, backup restore and real release/rollback receipt |
| C43 | Live debugging and real-time activity | R0, U1 | partial: polling and SDK event journal | Bounded streaming, reconnect cursors, partial-event presentation and completed-span authority |
| C44 | Human-friendly access on desktop/phone | U1, H1 | partial: responsive vector UI, modals, tooltips and guided paths | Real-user acceptance, assistive technology, keyboard drill-downs and measured performance budgets |

## Delivery sequence and proof

1. **Service foundation:** finish Texas identity/deployment and real member/source
   onboarding alongside server-enforced entitlements (C01, C05, C38-C42).
2. **Monitoring depth:** complete signal, issue, trace, spend and model/tool
   dashboards. Every count must open the same evidence cohort (C06-C20, C43-C44).
3. **Analytics and operations:** saved boards, cohorts/outcomes, correlation,
   alerting, experiments and scoped agent tools (C21-C27). These are real features,
   not visual imitation of PostHog or Sentry.
4. **Enterprise datasets:** implement collections first, then policy-driven builds,
   immutable revisions, review/correction workflows and enterprise governance
   (C28-C34). No training starts because a person labels a signal.
5. **Learning:** bounded supervised experiments and unsupervised candidate discovery;
   held-out evaluation, reversible promotion, regression monitoring and recurrence
   close the loop (C35-C37). External model fine-tuning is a separate supported
   provider capability, not assumed from a consumer subscription.

Monitoring may ship before learning, but dataset lineage and purpose/consent must
be designed into capture/review from the beginning. The dataset implementation
contract remains [automatic-datasets-v0](../spec/automatic-datasets-v0.md); learning
mechanisms remain [mechanism-spec-v0](../spec/mechanism-spec-v0.md). This map links
those owners and does not replace their schemas.

The current signal-dashboard increment is documented in
[signal drill-down](../ux/signal-drilldown.md). It closes a concrete monitoring gap;
it does not close the entire capability baseline or the enterprise learning gates.
