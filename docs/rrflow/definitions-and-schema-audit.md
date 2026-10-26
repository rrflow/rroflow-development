# RRFlow and Connectome: definitions and schema audit

Date: 2026-10-17. Asked by Jesse: before any extension (SWAG, RROFLOW, endpoints), "truly ensure that rrflow and connectome are becoming truly what my notes are describing", built for scale, configurable and extensible, with schemas and definitions established for estate/project, the vector DB, its connectors, vector storage, the memory DB, and the graph links between memories and vectors.

Method: WF-01 "Evidence and Context Reconciliation" (Notion, 2026-10-15), Jesse's own procedure: source inventory, claims register, terminology map, contradiction register, decision queue. Pass criteria from WF-01: every claim has provenance; every contradiction has a disposition; no unknown is presented as settled; no secrets; no whole-project completion is inferred from component evidence.

Canon rule (decided by Jesse, 2026-10-17): **the latest explicit statement wins.** Order: 2026-10-17 statements > WF-01 and Review Build 01 (2026-10-15) > 2026-10-13 to 16 statements > `LFG_NOTES_v3` (2026-09-26) > the August Shippin component registry. Older notes hold where newer ones are silent. Every override is a proposed update to the older page (section 9), not a silent rewrite.

Code basis: `rrflow/rrflow-development` `main` at `238e883` (PR #5, merged today as `1b9c4eb`, changed one crate comment and three docs); `rrflow/connectome-development` `main` at `b388e48`. Two read-only code audits were run; load-bearing facts were re-checked in code before use. Nothing was built or benchmarked.

## 1. Source inventory

| Source | Date | Read | Role under the canon rule |
|---|---|---|---|
| Jesse's statements in this project (chat), incl. 2026-10-17 | Oct 13–17 | Full | Highest |
| Notion: Reusable Workflow Library, Review Build 01; WF-01 | 2026-10-15 | Full | Confirmed operator decisions: "Connectome is the UI for RRFLOW. RRO means Reason Ready Objects and is part of RRFLOW. Dalal is replaced by Swag" |
| Project docs: capability study, build order, workload classification, salvage assessment | Oct 13–17 | Full | Records of Jesse's October decisions |
| Google Drive: `LFG_NOTES_v3` | 2026-09-26 | Full | Supersedes v1, v2, v2.1 by its own header; "DRAFT — not canon" |
| Google Drive: Notes v2, v2.1 Addendum | 2026-09-25 | Full | Superseded by v3; used only where v3 is silent |
| Notion: Component Registry, "Connectome" | 2026-08-16 | Full | Oldest; overridden on the meaning of Connectome |
| RRFlow repo: AGENTS.md, README, system-overview, instance-topology, deployment modes, Gate A–J chapters, POA&M, vector/context/storage references, rrd-contract, rrd-core, rrd-store, rrd-vector, rrd-inference, rrd-engine | `238e883` | Code audit (read-only) | Implementation evidence |
| Connectome repo: src-tauri/src/rrd, src/rrflow, src/screens/connectome, src/vendor, package.json, vite.config.ts, docs, sprint journals | `b388e48` | Code audit (read-only) | Implementation evidence |
| Not read: "Session Notes & Brainstorm (2026-09-24)" (v1, superseded), `shippin-canon-capture-2026-08-15.md`, Notion HQ / Intake Protocol / Bundled Stack / Demo Pilot pages | — | Search highlights only | Recorded limitation; none of them is cited below |

## 2. Terminology and alias map (proposed canon)

| Term | Latest statement (source) | Earlier meanings | Repo today | Proposed canonical definition |
|---|---|---|---|---|
| **RRFlow** | "one true platform", SurrealDB- and Qdrant-class capabilities authored as RRFlow (Oct 17) | v3 §11: "dedicated AI persistence (Postgres)"; v2 §13: superseded "RRO as reasoning database" | README: Reason Ready Flow, the engine and bundled adapters; one `RrdEngine` authority | The engine and platform: rrflowDB, vectors, graph, retrieval, context, configuration, and the plug-in configurations below |
| **RRD** | — | — | Reason Ready Daemon: `rrd-server` plus embedded `RrdEngine` | Keep |
| **Estate / rrflowDB** | Decision 1 (Oct 17): one project ↔ one estate ↔ one instance; an account owns many estates | v2 §8.1: `"estate": "proj-7c1"`, an address target; Shippin "estate" = his physical infrastructure | instance-topology.md:25: "one independent AI estate for one project"; Connectome glossary: "One project's database in RRFlow" | One project's database, served by one instance. An account (shippin.cloud user) owns many |
| **Project** | — | v3 §8 permission scope "Project (default)" | `project_id` in `InstalledEstateIdentity`; "Existing-project attunement" scaffold in Connectome | The developer's codebase or knowledge base an estate serves |
| **Instance** | — | — | One logical RRD instance serves one estate; every request scope is `instance:{instance_id}` | Keep |
| **Workspace** | Decision 1 (Oct 17): a set of estates, routed above the engine | v3 §8 permission scope "Workspace: whole workspace root" | instance-topology.md:58: a derived build topology, not a tenancy boundary; `ResourceKind` has no Workspace | A set of estates an account groups; a client and account concept, never an engine scope. The topology's "build topology" sense must be renamed or removed (section 9) |
| **Domain** | — | v2 §12: a category below operation and part | Only "consensus and placement domain" (cluster) | Not an RRFlow data concept; reserve for the LFG category tree |
| **Tenant / namespace / database** | — | — | `ResourceKind` has Tenant, Namespace, Database; topology says reserved; schema-catalogue defaults both to `"default"`; key slots always None | Reserved; align the two docs (section 9) |
| **Connectome** | "Connectome is the UI for RRFLOW" (Oct 15); knowledge trees and connectors in it (Oct 17) | Aug registry: "the fused SurrealDB + Qdrant fork… the memory and retrieval substrate beneath Clyffy" | A separate Tauri desktop client over public operations (AGENTS.md; H-06) | The RRFlow client. The August meaning now belongs to RRFlow itself |
| **RRO** | "Reason Ready Objects… part of RRFLOW" (Oct 15); "self improved reason ready objects" (Oct 17) | Repo: a retired provisioning role (removed by A-07; last comment fixed in PR #5) | Only the retired sense existed; now gone | Reason Ready Object: a persisted, versioned unit of knowledge RRFlow reasons over (record, relation, claim, vector, reasoning node). Needs a glossary entry before RROFLOW work |
| **RROFLOW** | "the recursive self improvement configuration" (Oct 17) | v2 §17: continual learning promoting into rules; A1: Raindrop as reference | No roadmap item; nearest Gates G (reasoning trees) and I (triggers, routines, skills) | A plug-in configuration; needs its own record |
| **LFG** | Decision 3 (Oct 17): deterministic kernel of compiled rules plus a configured extension (tier 2 encoder and classifiers, tier 3 small CPU or remote model) used on escalate; feeds SWAG | v3 §2: "System 1 controller… deterministic kernel… no LLM inside" + an extension; Oct 13: "CPU-based local LLM" | Gate G: one adapter behind `RouterBackend`, three grammar-constrained routing decisions; terminology row says the model question is undecided | A CPU-based, optional plug-in configuration: kernel + extension, every answer with provenance; SWAG decides. Gate G and the LFG row are rewritten to this |
| **SWAG** | "the System One controller that can be plugged into RRFLOW"; "DALAL is swag… concept stays the same. endpoint system one controller" (Oct 17) | v3 §10 Dalal: LLM gateway (credentials, capability registry, model selection, dispatch); Oct 15: "some wild ass guess" | No record | The System One controller and model gateway: decides, selects and dispatches models through configured endpoints. Needs its own record |
| **Governance** | — | v3 §8: verification checks (formerly Judge), permission gate, proof pattern; "Warden formalized" | `verification_requirement` on terminal reasoning nodes (A-02); authorization in `RrdEngine`; no permission-scope type | Keep v3's definition; map to Gate G/I when scheduled |
| **Memory** | "the actual DB dedicated to memories" (Oct 17) | Aug: SurrealDB `clyffy.ai_memory`; v2 §9: `memory.corrections` as a rule source | No memory key family; records, relations, claims, events are the stored knowledge; `VectorMemoryTier` is cache residency; seats are records | **Memory = rrflowDB's records, relations and bi-temporal claims.** No second database |
| **Vector DB** | SurrealDB- and Qdrant-class capabilities (Oct 17) | Aug: Qdrant (LXC 106) | Collections, named dense/sparse/multi vectors, payload indexes, exact and HNSW search, quantization | The vector subsystem of the one engine |
| **Vector storage** | "flat files associated" → decided Oct 17: binary pages inside rrflowKV by default, plus a configurable file-segment tier in the same binary | v2.1 A4: "flat file, Postgres or others" behind a store trait | Vectors are JSON values in rrflowKV; index artifacts are files in the object tier | Section 3.4 |
| **Connector** | "its connectors" → decided Oct 17: embedding endpoints and ingestion sources | Oct 17 also: Connectome's connectors to SurrealDB/HydraDB as knowledge trees | No connector abstraction in either repo | Engine side: endpoint bindings and ingestion sources. Client side: a later record |
| **Endpoint / binding / profile** | "endpoints configured… nothing hardcoded" (Oct 13–17) | Workload classification: class contract, endpoint binding, profile, hardware inventory | None exists: "No model or endpoint configuration exists anywhere" (code audit); limits-only TOML | Section 3.3 |
| **Knowledge tree** | SurrealDB and HydraDB "as knowledge trees in Connectome" (Oct 17) | — | Not in either repo | A Connectome view of an external store; parked |
| **shippin.cloud instance** | one platform that synchronizes with its shippin.cloud instance (Oct 17) | Oct 13: hosted instance per user + local copy, two-way binary sync | Loopback-only `rrflow serve`; mesh adapter never owns state (H-07); no sync protocol | Parked; needs a decision record |

## 3. Claims register: what you asked for against what exists

Verdict scale: **Native** (intended path, gate checked), **Works, not native** (interim path, gate open), **Partial**, **Missing**, **Conflict**.

### 3.1 Estate / project

| Claim | Provenance | Code | Verdict |
|---|---|---|---|
| One project ↔ one estate ↔ one instance | README:48; instance-topology.md:25-26, 55-56 | `InstalledEstateIdentity {project_id, estate_id, instance_id}`, pairwise distinct (`rrd-contract/src/deployment.rs:25-41`); all three derived from one install seed (`installation.rs:350-360`) | Native for the alpha lock |
| Data is estate-scoped | instance-topology.md:92 | Request scope must equal `instance:{instance_id}` (`engine/core.rs:72-78`); kernel `ScopeId` accepts any string | Conflict (doc vs code) |
| Permission scopes Project / Workspace / Allowlist / Unrestricted | v3 §8 | No `PermissionScope` type; no workspace concept | Missing; depends on Decision 1 |
| Hosted instance per shippin.cloud user | Oct 13 | One estate per project; no user or account concept in the engine | Conflict → Decision 1 |

### 3.2 Vector DB

| Capability | Code | Verdict | Gate |
|---|---|---|---|
| Collections with named dense, sparse and multi-dense vectors, metric, dimensions, embedding model, memory tier | `EnsureVectorCollection`, `NamedVectorDefinition` (`rrd-contract/src/lib.rs:837-886`); catalogue is JSON at control key `server/state/vector-collection-catalogue/{scope}` | Works, not native: the catalogue sits outside the runtime commit | E-04 |
| Point ↔ record link | `PutVector.subject: DataReference`; the subject must be a current record in scope or written in the same commit (`semantic_commit.rs:385-424`); returned in every hit | Native. **This is the seamless link you asked for; it exists at the schema level** | — |
| Exact search | `search.rs`; reads all vector versions in scope and replays two catalogues from cursor 0 per search (POAM-028) | Works, not native; 174 ms per search over 270 vectors | E-04, POAM-028 |
| HNSW | Built only on explicit call; artifact `RRDHNS02` is canonical JSON; dense-only | Partial | E-04 |
| Quantization | `RRDQNT01`, TurboQuant `RRDTQ001`; dense-only; lifecycle records exist | Partial | E-04, G-04 |
| Sparse and multi-vector | Exact scoring exists for sparse (`rrd-vector/src/exact.rs:294 score_sparse`) and multi-dense with `MaxSim` (`contract.rs:50`, `exact.rs:223`); HNSW and quantization are dense-only | Partial: exact path only | E-04 |
| Payload (filtered) search | Search requests carry a filter (`engine/vector/search.rs:148-151`); HNSW declares `filter_properties` (`hnsw.rs:62`) | Partial; filtering inside the graph walk not checked | E-02/E-04 |

### 3.3 Connectors: embedding endpoints and ingestion sources

| Claim | Code | Verdict |
|---|---|---|
| Every model reached through a configured endpoint; local vs remote changes nothing (Oct 13–17; v2 §11 "Local is an endpoint") | `EmbeddingBackend` trait and a process-local, never-persisted registry (`rrd-inference/src/lib.rs:459-505`); no product binary installs a backend except `rrflow-edge`; FastEmbed behind feature `fastembed-local`, not forwarded by `rrd-engine`; `RouterRuntimeBinding` carries digests only, no URL or path; install profile says embed is `provider-unconfigured` | **Missing.** No endpoint, binding or profile type exists. The semantic stage of context assembly is always skipped |
| Bring a project in (tree inventory, parsing, normalize → embed → index → graph) | 11 `AttunementPhase` values in the contract; install commits one `Pending` job; no code advances any phase; no attunement crate; D-01 to D-06 unchecked | Missing (Gate D) |
| Knowledge import (KB-06) | `KnowledgePackageV1` contract and `scripts/knowledge/export.py` exist; no importer | Missing |
| HydraDB / SurrealDB imports (build order step 5) | Nothing in either repo | Missing |

### 3.4 Vector storage

Facts: vector heads and versions are written with `put_json` (`rrd-store/src/access/vector.rs:146-173`), so every f32 is a JSON number; HNSW, quantized and compact-dense artifacts are content-addressed files under `<storage-root>/immutable/objects/sha256/<2hex>/<sha256>`; artifact generations are runtime records replayed from cursor 0 on every call.

Decision (Jesse, 2026-10-17): **binary vector pages inside rrflowKV by default, plus a configurable file-segment tier, both inside the one `rrflow` binary.** Design constraint from the repo's own rules: the file tier must follow the object-tier pattern (immutable segment files referenced by the commit) so `RrdEngine` stays the only writer (ADR-0001); a second write authority would reopen the single-engine decision. This is E-04 work (atomic vector delta, persisted generations) plus a storage-profile axis in deployment modes; it is not scheduled yet.

### 3.5 The memory DB

Facts: no memory key family exists. The stored knowledge is records (0x10/0x11), relations with both edge directions (0x12/0x13), events (0x1f), bi-temporal claims (`Claim` with `valid_from`, `valid_to`, `tx_time`, `Tier`, `PromotionState`; `rrd-core/src/claim.rs:17-81`), series, geo and objects, all as JSON bytes. The public `AssertClaim` omits `valid_to` and tier.

Reading applied: **memory = rrflowDB's records, relations and claims in the same store as the vectors.** A separate memory database would contradict "one true platform" and ADR-0001. If you meant a distinct store, say so and it becomes a decision.

Gaps: no first-class "memory" or "reason ready object" vocabulary; claims cannot be asserted with their full bi-temporal shape over the public contract; reasoning trees are a frozen contract fixture (A-02) with no persisted execution.

### 3.6 Graphs ↔ memories ↔ vectors

| Claim | Code | Verdict |
|---|---|---|
| One commit across records, relations, vectors and indexes | `TransactionMutation` set committed as one semantic batch | Native |
| Vector hit expands to graph neighbours in one operation | `assemble_context_at` (`engine/context.rs:70-452`): seeds → snapshot BM25 → exact vector → bidirectional BFS over in-memory relations → weighted RRF (k = 60; weights seed 2.0, text 1.0, vector 1.0, graph 0.5, all constants) | Works, not native: relations are loaded whole; adjacency keys 0x12/0x13 are written but read only in tests |
| Retrieval algebra with a graph leaf | `retrieval_query.rs:460-504`: weighted RRF only, no graph leaf | Partial |
| Traversal in rrflowQL | Linear scan over materialized relations (`rrd-query/src/execute.rs:806-840`) | Works, not native (E-01) |
| Clusters / communities | Computed in the Connectome client (Louvain, `src-tauri/src/rrd/cluster.rs`), not in the engine | Client-side only |

### 3.7 Configurable, extensible, built for scale

| Requirement (Jesse) | Fact | Verdict |
|---|---|---|
| Nothing hardcoded beyond best-practice defaults | Hardcoded: bind `127.0.0.1:9477` (server, CLI, Kubernetes 0.0.0.0:9477), feature-hash id and model name, edge dims 384 / seed 7 / top_k 10, RRF constants, vector residency 256/64 MiB, retrieval 8 MiB output, HNSW bounds, `QueryBudget::default`, `EstateConfiguration::default` duplicating the TOML. Connectome: `localhost:9477`, Auth0 URL, session and request timeouts, page sizes, map limits | Partial. Defaults are acceptable only once they are overridable through configuration; today most are not |
| UI-managed, versioned config revisions | `EstateConfiguration` holds reasoning/recall/query limits only; revision fixed at 1 by install; "changing it requires a future governed configure operation" | Missing (build-order step 2) |
| Schemas as one source of truth | `schemars` derives in `rrd-contract` only; OpenAPI generated in code, digest-pinned, 5 SDKs generated and conformance-tested. Matches v3 §7 ("Rust-first… ArkType and Pydantic generated from the committed JSON Schema"). Gaps: no schema for `rrd-core`, `rrd-vector`, `rrd-store` types; hybrid, retrieval, vector-index, quantization, embedding and `ReadDataSnapshot` types absent from OpenAPI; model families reach OpenAPI only through `CommitTransaction` | Partial; the right foundation |
| Extension points: triggers, routines, skills, adapters | Gate I unbuilt; Gate G unbuilt; attunement phases never advance | Missing |
| Scale proof | OBJ-10 not established; POAM-012 open; harness is step 2b | Missing; measured losses only |

### 3.8 Connectome

| Fact | Verdict |
|---|---|
| Consumes session, capabilities, diagnostics snapshot, query, vector collections/points/search/map. Records and relations come from the diagnostic snapshot's `graph` section, not a dedicated read | Works |
| Never calls context assembly, audit read/export, changefeed, estate, backup, transactions (tests only) | Partial: the "brain" surface is not shown |
| `src/vendor/rrflow-client.ts`: `export { Surreal as RRFlow, surql as rrflowql } from "surrealdb"`, imported by 58 files; SurrealQL in 31 files; `src-tauri/src/database/mod.rs` launches a SurrealDB-shaped server (`start --bind … --user --pass`, `rocksdb://|rrflowkv://|tikv://`) | Conflict with "one true platform"; Sprint 007 Slice E-6 is still open |
| Glossary: "Estate (rrflowDB): One project's database in RRFlow"; `RrdInstalledEstateView {project_id, estate_id, instance_id}` | Aligned |
| No connector abstraction; HydraDB and Postgres absent; Qdrant only in a March draft roadmap | Missing (knowledge trees parked) |
| Env var case mismatch: renderer reads `VITE_Connectome_PREVIEW`, `VITE_Connectome_DOCKER`, `VITE_Connectome_WEBSITE_BASE`; build and native read `VITE_CONNECTOME_*` | Defect; small fix |

## 4. Contradiction register

| # | Contradiction | Disposition under "latest wins" |
|---|---|---|
| C1 | Connectome = fused SurrealDB+Qdrant substrate (Aug) vs the RRFlow UI (Oct 15) | UI. Propose updating the Notion registry page; the August role is RRFlow's |
| C2 | RRFlow persists to Postgres behind a store trait (v3 §11) vs rrflowKV canonical, Postgres an optional adapter (Oct 16) | Resolved by Decision 12: rrflowKV canonical; Postgres reaches rrflowDB through a Postgres-wire SQL face (adapter crate). Update v3 §11 |
| C3 | System 1 controller = LFG; Dalal = LLM gateway (v3 §1, §2, §10) vs SWAG = System One controller, LFG = CPU feeder (Oct 17) | SWAG. LFG feeds it. Gate G's text matches neither and must be rewritten before Gate G starts |
| C4 | LFG has "no LLM inside" (v3 §2) vs "CPU-based local LLM" (Oct 13); Oct 17 says only "CPU based optional configuration" | Decided (Decision 3, Oct 17): kernel with no model plus a configured extension whose tier 3 may be a small CPU model or a remote model. Both notes hold |
| C5 | "Do not continue [rrflow-development] toward its 1.0 gates as the product" (v2 §13) vs the October salvage, canonical-store and build-order decisions and PRs #1–#5 | Superseded by October. v3 already softened it: "storage core central again… cluster, Kubernetes and SDK layers stay parked" |
| C6 | One project per estate (repo) vs Workspace permission scope (v3) vs one hosted instance per shippin.cloud user (Oct 13) | Decided (Decision 1): one project per estate; an account owns many estates; workspace lives above the engine |
| C7 | Repo doc vs code: data "estate-scoped" vs instance-scoped keys; schema-catalogue "one namespace and database" vs topology "reserved"; `PLATFORM_TERMS` roles vs topology table; `Organization` vs `Organisation`; multi-model-object-contract.md says no bidirectional adjacency and cursor-0 snapshot vs code writing both directions and reading directly; rrflowkv-current-format.md "v5" vs "v6"; vector and retrieval references describe cursor-0 scans where code now reads directly; `claim.rs` cites a `SPEC.md §9.2` that does not exist | Code is the fact; each doc gets a correction in one documentation PR (section 9). None changes behavior |
| C8 | Connectome re-exports SurrealDB as `RRFlow` and launches a SurrealDB-shaped server (code) vs one platform (Oct 17) | Known gap (Slice E-6); schedule after the engine's query and schema reads cover the used behavior |
| C9 | RRO = retired provisioning role (repo) vs Reason Ready Objects (Oct 15) | Reason Ready Objects. The retired sense is gone as of PR #5; add the glossary entry |
| C10 | "Judge" (v2) vs Governance verification checks (v3) vs `verification_requirement` on terminal reasoning nodes (repo) | Same mechanism, three names. Canon: Governance (v3); the repo keeps its type name and gains the term in the glossary |

## 5. Schema status register

| Concept | Contract type | JSON Schema / OpenAPI | Persistence | Status |
|---|---|---|---|---|
| Estate identity and configuration | `InstalledEstateIdentity`, `EstateConfiguration` (`rrd-contract/src/deployment.rs`) | schemars yes; OpenAPI via install operations | control key `server/state/{instance}/configuration/active`, revision 1 | Established; revisions after install missing |
| Record, relation, event, vector, series, geo, object, claim | `TransactionMutation` variants (`lib.rs:5599-5691`); kernel types in `rrd-core` | OpenAPI only through `CommitTransaction`; kernel types have no schemars | key families 0x10–0x1f, JSON values | Established as wire shapes; no standalone schemas |
| Vector collection and point | `EnsureVectorCollection`, `NamedVectorDefinition`, `PutVector`, `VectorSearchHit` | schemars yes; vector-index and quantization types absent from OpenAPI | catalogue control key; vectors 0x19 as JSON | Established; encoding and catalogue placement to change (3.4) |
| Context packet and retrieval | `ContextPacket`, retrieval algebra | present; hybrid/retrieval types absent from OpenAPI | n/a | Established; no HTTP route for hybrid retrieval |
| Reasoning tree | `reasoning_tree`, nodes, typed edges, recipe, cursor, decision evidence, verification requirement (A-02) | golden fixture `reasoning-tree-v1.json` | not persisted | Frozen contract; no execution |
| Answer (Gate, Choice, Score, Extract, Compose) with probability, confidence, tier, provenance | none | — | — | **Missing** (v3 §3); required by LFG and SWAG |
| Address (stable `CategoryId` segments, revision, 64-bit key, alias table, namespaces `rr1:core / op.<operator> / com`) | none | — | — | **Missing** (v3 §5) |
| `PermissionScope` | none | — | — | **Missing** (v3 §8) |
| Check, Evaluator, Verdict | `verification_requirement` only | — | — | Partial (v3 §8) |
| Rule source, promotion, journal record | `Claim.PromotionState` exists with a different meaning (claim tiers) | — | — | **Missing**; name collision to resolve |
| Endpoint binding, profile, hardware inventory | `WorkloadClass`, `BindingKind`, `EndpointLocator`, `CredentialReference`, `DeclaredFootprint`, `BindingLimits`, `EndpointBinding`, `EndpointProfile`, `HardwareInventory`, `MachineLoad` (`rrd-contract/src/endpoint_binding.rs`, PR #7, `9598d39`) | schemars yes; not in OpenAPI (no operation uses them yet) | none yet; a revisioned record beside `EstateConfiguration` once config revisions exist | Established (Decision 4); runtime resolution missing |
| Attunement plan, job, phases | `AttunementPlan`, `AttunementJobState`, 11 phases | yes | `server/state/{instance}/attunement/*` | Established; never advanced |
| Knowledge package | `KnowledgePackageV1` | yes | exported JSONL only | Established; no importer |

## 6. Verdict: are the two repos becoming what the notes describe?

**Design: yes, on the parts that are written.** The one-store premise of the August note ("relational context and semantic similarity resolve in the same substrate") is RRFlow's founding rule and is enforced in code: one transaction, one key grammar, a vector that must point at a record. The v3 schema rule (Rust contract crate, generated SDKs, shared fixtures) is already the repo's practice.

**Implementation: the storage foundation only.** Gates A–C are the only checked gates (A 7/7, B 5/5, C 6/7 with C-04 reopened). Every read path above storage is interim and reads far more than a request needs; vectors are JSON numbers; the HNSW artifact is JSON; the vector catalogue sits outside the commit; no embedding backend is installed; nothing brings a project in; no endpoint or model is configurable; no answer, address, permission, binding or journal type exists; Connectome shows the storage and vectors but not the brain, and still carries the SurrealDB client and server shape. None of this is hidden: the roadmap, POA&M and this week's PRs record it.

**Scale: unproven.** OBJ-10 is not established and the only engine measurement is a loss (POAM-028). The step 2b harness is the first proof.

**Configurable and extensible: the shapes exist, the mechanisms do not.** Config is a limits-only TOML sealed at install; endpoints are not a concept yet; extension points (triggers, routines, skills, the LFG adapter) are unbuilt.

## 7. Machine-learning tactics: what applies, what exists

Terms you will meet, and where RRFlow stands. Priority is judged against your scale (one developer's estates, then shippin.cloud users), not against a hyperscaler's.

| Tactic | What it buys | RRFlow today | Priority |
|---|---|---|---|
| **Bi-temporal knowledge** (valid time + transaction time; "what did we believe at r42, as known then") | Reproducible context; corrections without loss; the basis of promotion and audit | Native in `Claim` and read stamps; the property temporal knowledge graphs such as Graphiti market, already in your kernel | Keep; expose `valid_to` and tier over the contract |
| **Scalar/binary quantization with rescoring** (int8 or 1-bit vectors for the first pass, full vectors to re-rank the top candidates) | 4–32× less memory and bandwidth at near-identical recall | `RRDQNT01`, TurboQuant `RRDTQ001` artifacts, dense-only; lifecycle records | High; lands with E-04 and the binary page encoding |
| **Matryoshka embeddings** (models trained so the first N dims are a usable shorter vector) | Cheap coarse pass, fine pass on the full vector | Absent; needs a model that supports it and a dims-truncation option on the collection | Medium; a configuration option once endpoints exist |
| **Hybrid retrieval with RRF** (reciprocal rank fusion of BM25, vector and graph rankings) | Robust recall without tuning score scales | Native shape; constants hardcoded (k = 60, four weights) | Make the weights configuration |
| **Learned sparse retrieval** (SPLADE-style term weights as sparse vectors) | Lexical precision with semantic expansion | Sparse vectors scored exactly (`score_sparse`); no sparse index; no model produces them | Medium; after E-03 postings and an endpoint |
| **Late interaction / multi-vector** (ColBERT, ColPali: one vector per token or patch, MaxSim scoring) | Strong reranking, multimodal pages | `MultiDense` with `MaxSim` scored exactly; not indexed | Low until a reranking stage has a model |
| **Cross-encoder reranking** | Precision on the top 20–50 | `RetrievalQuery::Rerank` exists with deterministic stages such as `ScoreBoost` (`retrieval_query.rs:506-516`); no model-backed stage | Medium; needs an endpoint |
| **Filtered ANN** (payload filters inside the graph walk; ACORN-style) | Fast "similar, but only in this folder" | Filters on the search request; HNSW declares `filter_properties`; behavior inside the walk not checked | High for a developer estate |
| **Disk-resident graph index** (DiskANN/Vamana) | Billions of vectors on NVMe | Absent; relevant to the file-segment tier you chose as an option | Low at your scale; the tier makes it possible later |
| **Graph-aware retrieval** (expand hits along relations; community summaries, "GraphRAG") | Context that follows structure, not just similarity | BFS expansion exists in context assembly; communities only in the Connectome client | High: move clustering into the engine when E-01 lands |
| **Memory consolidation** (episodic → semantic promotion, decay, supersession) | A brain that gets sharper, not just bigger | `Claim` tiers and `PromotionState` give the shape; nothing runs it | RROFLOW's first mechanism |
| **Per-node classical classifiers** (logistic regression / gradient boosting over a journal) | Microsecond tier-2 decisions, retrain one node | Absent; v3 §4 places it in LFG's extension | Belongs to LFG, after the journal exists |
| **Rule promotion from reasoning traces** ("the slow path mints fast paths") | Continual learning into rules instead of weights | Absent; v2 §17 calls it the open gap in the market | RROFLOW; needs the Answer, Address and journal types first |

The stern line: none of these tactics pays off before the fundamentals: direct index reads (E-01, E-02, POAM-028), binary vector pages (E-04), a configured embedding endpoint, and ingestion. A quantized index over JSON-encoded vectors replayed from history is still slow.

## 8. Decision queue

**Decision 1 (decided by Jesse, 2026-10-17): one project ↔ one estate ↔ one instance stays.** A shippin.cloud account owns many estates; a "workspace" is a set of estates routed above the engine; context across projects is federated later. Consequences: the Workspace permission scope (v3 §8) is a client- or account-level concept, not an estate one; the unused tenant/scope key slots stay unused; sync is per estate. Canonical update: instance-topology.md gains the account → estates relation and the workspace definition (section 9, item 7).

Rejected: one estate per account holding many projects (would rewrite the identity triple, request scope, authorization and every gate's text); deferring until ingestion.

**Decision 2 (decided by Jesse, 2026-10-17): the settled definitions and the six corrections landed as one documentation PR (RRFlow PR #6).**

**Decision 3 (decided by Jesse, 2026-10-17): LFG = kernel + configured extension.** The kernel holds compiled rules only, deterministic and golden-file proven (v3 §2 stands). The extension runs on escalate and is bound as configured endpoints: tier 2 encoder and per-node classifiers on CPU, tier 3 a small CPU model or a remote model. Every answer carries value, probability, confidence, tier and provenance; SWAG decides, LFG suggests. The Oct 13 "CPU-based local LLM" remark is satisfied by the extension's tier 3 binding. Canonical updates: the repo's LFG terminology row ("whether a model runs inside it is undecided") and Gate G's text, in the Gate G rewrite PR.

**Decision 4 (decided by Jesse, 2026-10-17): the endpoint binding and profile schema landed first, as RRFlow PR #7 (`9598d39`).** Next schema candidates: Answer and Address (LFG/SWAG), then PermissionScope and Check/Verdict (Governance), then the journal record.

**Decision 5 (decided by Jesse, 2026-10-17): back to the build order; recoverability (step 1) before more schema.** The Answer schema waits; the Address format is PROPOSED in v3, not ratified, so no contract is built on it yet. Landed: RRFlow PR #8 (`726117b`, installed backups and restore staging beside `roots/`), PR #9 (`1239ce1`, restore recovery without a second writer; POAM-031 recorded), PR #10 (`5d1f87d`, backups carry the installed control state as a replayable control-journal archive), and PR #11 (`c8e4447`, offline `restore plan|apply` publication with quarantine; POAM-031 Closed).

**Decision 6 (decided by Jesse, 2026-10-18): publishable restore design = A.** A (chosen): close POAM-031, then publish by swapping the verified staging root into the installed root's own name with the old root quarantined under `restores/<instance>/`, operator digest accept, durable renames; keeps the identity triple and the single-root recovery rule. Rejected: B (publish under a new candidate storage-root id and rewrite the installed record, plan, managed paths, locator and recovery validator to follow it) and C (defer publication). Both steps landed: PR #10 (backups carry the control state) and PR #11 (publication). POAM-031 is Closed. Tracked in `claude/rrflow-build-order.md`, Step 1 status.

**Decision 7 (decided by Jesse, 2026-10-18): POAM-006 estate-controller backup/restore lane = A, converge in one PR.** Facts that forced the decision: the local estate driver installs managed instances canonically (`instances/<id>/.rrflow/config.toml` locator, `.rrflow/rrd/roots/<root-id>/` with `RRD.TOKEN`), but `EngineLocalBackupDriver` still opened `instances/<id>/.rrflow/rrd` as a store with a zero token key (so an estate backup of a real managed instance fails), and prune/restore kept catalogues under `<state_root>/backups/<id>` and candidates under `<state_root>/restores/<id>/` — a second catalogue the served engine never lists and candidates `rrflow restore plan` cannot see. A (chosen): the driver reads the locator, binds the distribution executable (new `--distribution-executable` on `rrd-backup-controller` and `rrd-recovery-controller`, matching `rrd-server`), opens the instance via `open_installed`, and uses the installed `.rrflow/rrd/backups|restores/<instance>` roots; the `<state_root>/backups|restores` lane is removed; the engine_authority kill-matrix and backup_controller fixtures install canonically. Rejected: B (two PRs — leaves main half-converged with two catalogue lanes between them, which AGENTS.md rule 12 forbids) and C (defer; estate backups stay non-functional for real managed instances). Implemented as RRFlow PR #14 (open at the time of writing); PR #13 (`7890ee4`) landed the `installed_control` wire field and the digest-citation check before it. Tracked in `claude/rrflow-build-order.md`, Step 1 status.

**Decision 8 (decided by Jesse, 2026-10-18): first Connectome increment = A, launcher + pin bump + guided install.** Context: Connectome's "Start serving" still ran SurrealDB's launch line (`rrflow start …`), which the `rrflow` CLI rejects, and its contract pin was 26 commits behind. A (chosen): replace the lane with the contracted sequence (install plan → apply --expect → `--json serve --bind 127.0.0.1:0` → ready → connect), bump the pin to `4705941`, explain each stage. Rejected: B (pin bump + honest errors only) and C (memory/AI loop first). Landed as Connectome PR #2 (`8c1daa2`, Slice A) and PR #3 (Slice B, open). Acceptance statement that frames every Connectome decision from here: RRFlow is 0.0 until Connectome shows every tab, browses estates, drills into graphs, and shows prompts processed by a frontier AI with memories in rrflowKV/rrflowDB that the AI is forced to use.

**Deferrable (named stage):**
- The roles of LFG/SWAG/Governance (C3) in Gate G's text: rewrite before Gate G, after Decision 3.
- SWAG record and RROFLOW record: before Gate G.
- File-segment vector tier design: with E-04.
- Connectome SurrealDB removal: no longer deferred — sprint 009 Slices B–D (Decision 9); the env-var case fix can land alone.
- shippin.cloud sync and the no-auth stance: decision records, as parked.

**Researchable without asking:** whether HNSW applies `filter_properties` inside the walk or after it; a binary artifact format for HNSW; which SDK types miss OpenAPI.

**Decision 9 (decided by Jesse, 2026-10-18 13:05): Connectome ownership — holder AngryVibes LLC, identifier `dev.rrflow.connectome`, sibling repositories under `github.com/rrflow/` bound by the one pin (no submodule), inherited changelogs replaced by authored ones, versioning as one contracted variable starting at ".05 alpha" (read as version `0.0.5`, channel `alpha`) with no movement until the product works.** Context: Jesse's 13:00 instruction (the naming has to be Connectome's, the repo has to belong to `github.com/rrflow/`, the code properly his, licensing exposure raised). Facts from the tree: a Surrealist snapshot import with SurrealDB's MIT as the only license text; sprint 006 renamed the word, not the product (SurrealDB's release notes shipped as "Connectome 3.7.x"; SurrealDB's JS client exported as `RRFlow` to 76 files; 120 links to a GitHub home that is not the repo's; 105 files of Surreal Cloud; `@surrealdb/ui` in 240 files; three of those npm packages publish no license at all). Decision 9 supersedes the 3.8 row above as the plan of record and the "Connectome SurrealDB removal (Slice E-6)" deferral: removal is now sprint 009 (`.agents/sprints/009-connectome-ownership.md`), Slice A (home/license/notice/version contract/identity gate — Connectome PR #6), B (delete the Cloud surface), C (replace the UI kit), D (move each view onto the RRD SDK in staircase order and delete `src/vendor` with the last importer). Tracked in `claude/rrflow-build-order.md`, Connectome section.

**Decision 10 (decided by Jesse, 2026-10-18): Connectome's visual identity = Connectome theme now, ColdLight tokens later.** Facts: after Slice C1 the inherited kit was imported only for its theme and font links, and that theme was SurrealDB's visual identity (brand gradient as `defaultGradient`, 117 vendor variant rules, runtime Google Fonts). Chosen: Connectome's own Mantine theme and `--connectome-*` tokens, self-hosted OFL fonts, built-in variants with Connectome defaults; ColdLight's generated tokens map into `mantine.tsx` when ready. Rejected: ColdLight-first (blocks kit removal on design-system plumbing), keep-the-look (reproduces their design). Landed as Connectome PR #9 (Slice C2), which also replaced the vendor's application icon, wordmark and imagery found during the work. Tracked in `claude/rrflow-build-order.md`, Connectome section.

**Decision 11 (2026-10-18 15:15): pause Connectome Slice D — superseded at 15:17** by Jesse's direction recorded under Decision 12.

**Decision 12 (decided by Jesse, 2026-10-18): "the pure Rust Postgres DB for AI" = rrflowDB with a Postgres-wire SQL face.** Context: Jesse's 15:17 statement — inherited code properly owned by RRFlow, rrflowQL confirmed and built as intended, the graphs, only LFG/SWAG/Connectome beside RRFlow, SurrealDB reference-only, and RRFlow defining all his capabilities including the Postgres DB for AI related to the project's vector database. Chosen: one adapter crate speaking the Postgres wire protocol (pgwire) with DataFusion SQL over rrflowDB, pure Rust, so Postgres tools reach the same estate, vectors and graph; not a second database. This resolves C2 in favour of rrflowKV as canonical and replaces v3 §11's "dedicated AI persistence (Postgres)". A capability record precedes code. Rejected: a separate Postgres-compatible store, or Postgres as persistence. Slice D: resume now, delete-first.

**Decision 13 (decided by Jesse, 2026-10-18): D5.3 in RRFlow now, Connectome D1 after.** Context: Jesse's 15:33 statement — Vyrm is RRFlow; the language was to be migrated and branded as RRFlow; his code was dumped rather than authored as one system. Facts from history: the Vyrm-era workbench's prompt flights were cut (`98e6551`, `1483e57`, `5448722`, `b5db6c2`) without the planned D5.3 engine absorption. Chosen: re-author prompt flights inside `RrdEngine` from the recovered `flight.rs` as the specification, record the loss as POAM-032, regenerate contract and SDKs, then rebuild Connectome's flight lens on the contract. Landed as RRFlow PR #17 (CI green, awaiting merge approval). Tracked in `claude/rrflow-build-order.md`, Connectome section.

**Decisions 14–17 (decided by Jesse, 2026-10-18, from his 17:18 direction: real instead of synthetic, built for scale, rrflowQL graphs per prompt and per major task, and a folder picker that builds the estate's vector DB).**
- 14: the order is per-prompt graph + tasks → E-01 edge reader → folder → vector DB → real provider flights.
- 15: provider arms are subscription CLIs (through the `subscription` binding) and API keys (from Infisical, in server memory) together.
- 16: a major task is a named task record (title, goal, status), and flights link to it.
- 17 (amended 20:47): the folder embedding model is configured per estate — embedded (`fastembed`, bundled) or an endpoint — through the endpoint-binding contract.

New definition from PR #18: **prompt-flight use edge** = the strict relation `rrflow-prompt-flight-used` from one flight to one canonical record its context packet included. Its identity is `<flight-id>.<rank>`, and it carries rank, fused score, and the contributing avenues. It is written in the flight's own commit, and claim items get no edge. Full sequence and status: `claude/rrflow-build-order.md`.

## 9. Canonical updates

Applied in RRFlow PR #6, merged to `main` as `947089f` (Decision 2, 2026-10-17):
- `docs/architecture/system-overview.md`: rows for RRO, RROFLOW, LFG, SWAG, Governance and workspace; the memory row names rrflowDB as the memory database; Connectome's row names it the client; RRFlow inference reaches every model through a configured endpoint.
- `docs/architecture/instance-topology.md`: workspace = a set of estates grouped above the engine; an account may own many estates; the estate scope is spelled `instance:{instance_id}`; the reserved namespace/database fields are reconciled with the schema catalogue.
- `rrd-contract/src/platform.rs`: workspace takes the `ClientGrouping` role; the module comment names the owning records and POAM-020. The table's replacement stays with POAM-020.
- Six stale implementation records corrected: the HNSW, quantization and retrieval references (direct version reads, only the POAM-028 catalogue replays remain), the multi-model object contract (versioned and adjacency keys exist; the snapshot reads directly), the rrflowKV current-format reference (segment v6), and the `rrd-core` claim-tier comment (SPEC.md "Claims" instead of a nonexistent §9.2).
- Found on the way: ten other code comments cite numbered `SPEC.md` sections that no longer exist (rrd-core lib.rs, reference.rs, temporal.rs; rrd-store examples and error.rs). Recorded in the journal for a follow-up under A-05.

Still proposed (each needs your yes):
1. Notion Component Registry, "Connectome": role → the RRFlow client; the substrate role → RRFlow.
2. `LFG_NOTES_v3` §1, §2, §10, §11: SWAG as System One controller and gateway; LFG as CPU feeder; RRFlow persistence = rrflowKV, Postgres optional; add RRO = Reason Ready Objects.
3. Gate G chapter: rewrite G-01 to G-06 around the LFG/SWAG split before any Gate G work; needs Decision 3 (LFG internals) first.
4. Connectome: `VITE_Connectome_*` → `VITE_CONNECTOME_*`.
5. `EstateAuthorityResourceKind::Organisation` / `AuthorityResourceKind::Organisation`: left as is; both serialize to the wire, and POAM-020 slates the types for removal.

## 10. WF-01 pass check

- Every factual claim has a source path, date or revision: yes.
- Every contradiction has a disposition: yes; C4 is dispositioned as an open decision, not settled; C6 was decided on 2026-10-17.
- Unknowns marked: filtering inside the HNSW walk is "not checked".
- No secret values: none appear.
- No whole-project completion inferred: Gates A–C only; everything else is stated as interim or missing.
