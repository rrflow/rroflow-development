# Research adoption and implementation ownership

Reviewed against development source `655525c88c85f33ba6041abaf7021c9c164830e0`
on 2026-10-31 UTC. This is the absorption inventory for the preserved research,
not a claim of feature parity, Texas activation or completed learning. The
[capability map](capability-map.md) owns the full feature backlog. The
[v0.4.0 verification record](../releases/v0.4.0.md) owns the released test evidence.

**Adopt** means accept a principle or requirement. **Rewrite** means implement it
in RROFLOW with its own types and tests. **Archive** means preserve the source for
historical context. **Reject** means do not inherit that behavior. None of these
dispositions imports a retired repository, its database, credentials or runtime.

## Source-to-destination inventory

| Source and subject | Disposition | Destination owner and current evidence | Remaining acceptance |
|---|---|---|---|
| [DevPulse ledger](../research/devpulse/devpulse-research-ledger.md): raw optional usage, explicit outcomes and separate content | Adopt principles; rewrite event representation | `rroflow-core` owns [record](../../crates/rroflow-core/src/record.rs), [capture](../../crates/rroflow-core/src/capture.rs) and [activity](../../crates/rroflow-core/src/activity.rs) contracts. [Activity tests](../../crates/rroflow-core/src/activity/tests.rs) include `multiple_terminals_never_sum_or_guess_their_scope`. | Full AiEvent/OTel interoperability and effective-dated pricing/census remain open. A local enum is not a general OTLP receiver. |
| DevPulse historical unknown-model `cost_micros=0` fallback | Reject numeric-zero fallback | Unknown cost must remain distinguishable from measured zero; [capability C19](capability-map.md) owns the pricing gap. | Implement and test the price table and coverage views before displaying derived spend. |
| DevPulse per-domain signal doctrine; [Raindrop authoring capture](../research/raindrop/signal-authoring-capture.md) | Adopt evidence-first semantics; rewrite rule catalog | `rroflow-core` owns [signals](../../crates/rroflow-core/src/signals.rs) and [custom predicates](../../crates/rroflow-core/src/custom.rs). Catalog, bounds, positive/negative mood and evidence tests live beside the implementation. | The current catalog is not the entire DevPulse channel/compiler model. Semantic classifiers and measured held-out precision are open. |
| [Raindrop live capture](../research/raindrop/live-capture-ledger.md): event identity and finished spans | Rewrite local/hosted ingest semantics | Collector and `rroflow-ingest` own delivery and immutable SDK events. [Service tests](../../crates/rroflow-ingest/src/tests.rs) include `capture_events_are_immutable_isolated_and_survive_restart`. | General OTLP mapping and real-provider version coverage remain open. Raindrop's observed upsert/span behavior is not inherited as our contract. |
| DevPulse backfill; owner requirement for capture across projects and disconnects | Adopt source-side capture; rewrite durable delivery | `rroflow-cli` owns [sources](../../crates/rroflow-cli/src/sources.rs) and retry. Tests cover `all_projects_offline_restart_and_changed_turn_are_durable` and `receipt_failure_restart_and_account_change_never_lose_or_redirect_records`. | Real account-linked hosted capture, sustained load, disk exhaustion and supported browser/mobile coverage remain open. A remote dispatcher does not automatically expose its own private history. |
| Raindrop MATCH/NOT MATCH review and generated-classifier refinement | Adopt review vocabulary; reject immediate activation from a label | `rroflow-ingest` owns [reviews](../../crates/rroflow-ingest/src/reviews.rs) and [SDK activity](../../crates/rroflow-ingest/src/sdk_activity.rs). Service tests pin historical evidence and reject stale reviews; [SDK tests](../../crates/rroflow-ingest/src/sdk_activity/tests.rs) verify projection recovery and owner isolation. | ACCEPTED/EDITED/REJECTED/ABANDONED correction pairs, trained classifiers and promotion evaluation remain open. A saved review does not alter an active classifier. |
| [Raindrop onboarding](../research/raindrop/ux-onboarding-and-gating.md) and [settings](../research/raindrop/ux-settings.md) | Adopt progressive explanation; rewrite member experience | Product UI owns [onboarding](../ux/vibe-coder-onboarding.md), [signal workspace](../ux/signal-workspace.md) and [source health](../ux/source-health.md). Packaged desktop/phone cases are retained by release CI. | Complete live member onboarding, editable redaction policy and broader customization remain open. Illustrations and fixtures must never be labelled live capture. |
| Raindrop dataset/evaluation research; DevPulse journal and corrections | Adopt provenance; rewrite consented metadata snapshots | `rroflow-ingest` owns [metadata datasets](../spec/metadata-datasets-v1.md). [Dataset tests](../../crates/rroflow-ingest/src/datasets/tests.rs) cover private immutable recoverable/revocable jobs and separate SDK consent with pinned review revisions. | This is a partial metadata implementation. Enterprise collaboration, correction content, qualification, split leakage checks and training remain open. |
| DevPulse nightly LoRA, fixed heuristic label weights and repeated-use auto-apply proposals | Archive as historical designs; reject implicit activation | [Automatic dataset design](../spec/automatic-datasets-v0.md) and [mechanism spec](../spec/mechanism-spec-v0.md) retain separate collection/training/promotion permission and held-out admission requirements. | No nightly trainer or production promotion exists. Dataset eligibility, thresholds and evaluator artifacts must be specified and tested before training or activation. |
| LFG tiered classifiers, unexplained-failure clusters and recursive improvement in the DevPulse ledger | Adopt design direction; defer implementation | RROFLOW owns monitoring-derived candidates and their evidence. [Mechanism spec](../spec/mechanism-spec-v0.md) records the design; [capabilities C32–C37](capability-map.md) retain the gaps. | Reproducible clustering, supervised training, independent holdouts, regression monitoring and reversal have no end-to-end implementation proof. Cluster membership is never a truth label. |
| [RRFlow canon notes](canon-alignment-notes.md): engine admission, snapshots and promotion receipts | Archive the dated engine inspection; adopt the independent boundary | RRFlow owns its engine and runtime promotion contract. RROFLOW owns monitoring and proposed improvements. No source imports by filesystem path. | A released, versioned integration contract and destination-native engine tests are needed. Historical notes are not a deployed interface or current engine audit. |
| [Raindrop prototype ledger](../research/raindrop/prototype-ledger.md) and retired DevPulse repositories | Archive as research; reject wholesale runtime absorption | All eight Raindrop notes remain in `docs/research/raindrop/`; the DevPulse source ledger remains in `docs/research/devpulse/`. The Research view indexes these versioned files. | No inherited database, secret store, license grant or deployment topology is authorized by their presence. |

## Ownership and evidence limits

`rrflow/rroflow-development` owns this inventory, application source, product
specifications and releases. Shippin's registry and ADRs own the integration
relationship, identity/subscription boundaries and portal integration. The
Shippin integration records do not move RROFLOW application source into its
umbrella repository. The original product plan remains a historical source;
current acceptance is explicit in the capability map and implementation specs.

Named tests above are concrete destination-native checks, not claims that every
source design is implemented. Their released execution is linked in the v0.4.0
record. Source review for this inventory did not run live provider calls, alter
the current local preview or prove public sign-in. Preserve each dated research
observation; use new evidence when revising a current capability claim.
