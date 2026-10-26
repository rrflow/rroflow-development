# Compute and data workload classification

Date: 2026-10-15. Purpose: classify every part of the AI stack (models, vector store, databases, project brain, learning loops) by what it consumes, and reach each part only through a configurable endpoint.

## Decided
- The eight workload classes below are locked (2026-10-15).
- Fact (2026-10-15): 3 DGX Sparks are live, in the original switchless ring.
- Endpoints, not hosts (2026-10-15, correction from Jesse): every class is a service reached through a named endpoint binding. Which machine, provider or model serves it is configuration, never code. Local, subscription, API and self-hosted are interchangeable bindings and must not change functionality.
- First binding profile `ring-3` (2026-10-15): two Sparks bound to large-model serving, one Spark bound to the brain (classes 2–5 plus the eval runner), weight training in scheduled windows on the serving Sparks, control plane on Proxmox, 550B deferred until a 4-node fabric. This is one profile, not the architecture.
- UI-driven and managed (2026-10-15, requirement from Jesse): bindings, profiles and hardware inventory must be designed so they can evolve into being created, edited and managed through a UI, not only by editing files.
- Config home (2026-10-15): the master copy of managed config lives inside each RRFlow instance's own store as UI-edited, versioned revisions. The repository ships only the schema and default templates; TOML is the import/export and backup form. Jesse's own setup is one instance like any other; a hosted shippin.cloud instance and its local-first copy sync config like any other data.
- Canonical instance store (2026-10-16): rrflowKV, RRFlow's own store, holds the project brain and the config revisions. HydraDB, SurrealDB, Postgres and DuckDB become optional bound adapters or import sources. The rrflow-hydradb prototype's UI and Raindrop wiring are to be pointed at RRFlow's API, and its HydraDB data imported.
- RRFlow build order (2026-10-16): recoverability first, then managed config revisions, ingestion on today's indexes, answer generation through bound endpoints, and the prototype UI pointed at RRFlow; C-07, E and F after. Detail and step 1 findings: `claude/rrflow-build-order.md`.

## What the store decision requires of RRFlow (mapped to its roadmap)
| Need | Roadmap item today | State |
|---|---|---|
| Canonical data is recoverable | D-11 backup verification, restore into an absent target; C-07 durability qualification | Open. The failing kill-matrix test is a missed fixture update (fix verified locally); installed-estate backup resolution is still unbuilt |
| UI-managed config revisions | Extends the existing revisioned `EstateConfiguration` (`rrd-contract/src/deployment.rs`), sealed at install by D-01 | Revisions exist only at install time; no edit/activate/rollback after install |
| Getting data in | D-02 persisted jobs, D-03 project-tree inventory, D-04 parsing, D-05 normalize → embed → index → graph | All open. D-05 is written to wait until Gates E and F pass |
| Generating answers through bound model endpoints | Gates G (LFG) and H (context flow, live delivery) | Open; waves 9–10 of 12 |
| UI | H-06/H-07 Connectome; the prototype's TypeScript UI could call RRFlow's HTTP API | Open |

Fact relevant to reordering: RRFlow's engine data-flow record states that Gates E, F and H replace the current whole-snapshot BM25, exact-vector and graph costs with native incremental paths "without changing the public operation". Ingestion built on today's paths would not need rework when E and F land.

## Why classification comes first
A DGX Spark has no separate VRAM. CPU and GPU share one 128 GB LPDDR5x pool per node. A database's page cache, a vector index held in RAM, and a model's weights and KV cache all draw from the same memory. Unclassified, the largest model takes the whole pool and everything else either starves or moves off the box.

## Endpoint model
| Concept | Meaning |
|---|---|
| Class contract | What a class does and its request/response shape. Code depends only on this. |
| Endpoint binding | One entry: class, binding kind (`local`, `self_hosted`, `subscription`, `api`), endpoint URL, model or service identifier, credential reference (an Infisical secret name, never a value), declared footprint (memory, GPU required), and limits. |
| Profile | A named set of bindings, one or more per class (ordered fallbacks allowed). `ring-3` is the first. |
| Hardware inventory | Declared capacity per machine (memory, GPU, disk). A profile is validated against it: declared footprints on one machine must fit. |
| Eval arm | May swap a profile or a single binding (e.g. local 9B vs an API model for class 2) and is measured with the decided metrics. |

Format and schema follow the decisions in `claude/skills-and-routines-inventory.md`: TOML validated by JSON Schema generated from Rust types with `schemars`. The types belong in `rrd-contract` beside `EstateConfiguration`.

Existing RRFlow vocabulary to reuse rather than reinvent: deployment profiles already treat deployment form, storage profile and endpoint presentation as independent coordinates (`docs/reference/deployment/modes.md`), and `rrd-contract/src/model_manifest.rs` has `RouterRuntimeBinding` and `RouterModelManifest`.

## What "UI-managed" requires of the design
1. Schema-driven forms. The UI renders from the same JSON Schema that validates files. Rust doc comments on the types become field descriptions in the generated schema, so labels and help text have one source.
2. Secrets as references only. The UI picks an Infisical secret name; values never enter config, the UI, or logs.
3. Versioned revisions with explicit activation. Every save creates a new revision; a revision is validated (schema, hardware fit, endpoint reachable) before it can be activated; rollback means activating an earlier revision. Mirrors RRFlow's governed-effects rule: exploration is free, durable changes are explicit.
4. Runtime resolution. Services look up the active profile at request time or on change notification, never at compile time, so an activation takes effect without a rebuild.
5. Observed state beside declared state. The UI shows each binding's health (reachable, latency, memory in use) next to its declared footprint, so the hardware inventory becomes measured, not guessed.
6. TOML stays the file form: shipped templates, import/export, backup. Files and UI share one schema.

### Precedents already in the ecosystem
- Warden (warden.rrflow.ai) has a live service catalog page and docker-compose templates with an `x-warden` extension schema covering SurrealDB, Qdrant, the Harrier embedder via TEI, Infisical and Authentik; its state store is SurrealDB (ADR 0005).
- The rrflow-hydradb prototype's dashboard creates databases and saves their Personal/Shippin assignment in HydraDB through the UI.
- Jesse's ADR rule: an ADR is edited and saved through an interactive component, then written to the repo; it is not canon until saved that way.

## Class 5 candidate facts (basis for the store decision)
| Candidate | What it gives today | Limits found | License |
|---|---|---|---|
| rrflowKV (RRFlow Rust engine) — chosen | All-or-none transactions across records, graph, vector and index deltas; bi-temporal versions with read stamps; deterministic, budgeted context assembly with per-item evidence; 832 of 833 tests pass (833 with the verified fixture fix); single binary target | No ingestion path; no answer synthesis; no UI; heavy per-change process | MIT OR Apache-2.0 (yours) |
| HydraDB managed cloud (`api.hydradb.com`) | What the working prototype uses: knowledge/memory ingestion, context graphs, hybrid retrieval, SaaS connectors | Hosted service; per the team's own research its SDK methods and connectors "should not be assumed to exist on a local graph node", so a local-first copy would not have the same surface | Proprietary service |
| HydraDB OSS engine | Rust; SlateDB plus object storage; snapshot-pinned reads; OpenCypher subset | Auto-commit only, so a sequence of requests is not one rollback unit; bare ID reads can return rows for nonexistent nodes; plain relationship creation can duplicate edges | AGPL-3.0 (modifying it and offering it over a network requires offering the modified source to those users) |
| SurrealDB v3 | Multi-model (document, graph, vector); already Warden's state store; DevPulse persistence plan | Vector store planned to be replaced by the Qdrant fork | BSL 1.1: may not be used "as a Database Service", meaning letting third parties "create, manage, or control schemas or tables"; converts to Apache 2.0 on 2030-01-01 |
| Postgres + DuckDB (three-tier plan) | Mature, permissive | No native temporal graph or context semantics; RRFlow-style logic would sit on top | PostgreSQL License; MIT |

## Hardware facts
| Item | Value | Source |
|---|---|---|
| Memory | 128 GB LPDDR5x unified (not DDR7), 256-bit, 273 GB/s | NVIDIA DGX Spark hardware overview |
| CPU | 20 Arm cores: 10 Cortex-X925 + 10 Cortex-A725 | same |
| GPU | Blackwell, 6,144 CUDA cores, up to 1 PFLOP FP4 with sparsity | same |
| Storage | 1 TB or 4 TB NVMe | same |
| Network | ConnectX-7, two QSFP ports; 10 GbE | same |
| Power | 240 W supply; GB10 SoC 140 W TDP | same |
| NVIDIA model guidance | Up to 200B parameters on one Spark, 405B on two | same |
| Live now (stated by Jesse) | 3 Sparks in the switchless ring; ~119 GB usable per node ≈ 357 GB total | Jesse |
| Planned (stated) | 4-node fabric, then 7–8 nodes on MikroTik CRS804 at 200G | Jesse |
| Large-model target (stated) | Nemotron 3 Ultra 550B via SGLang, TP=4/EP=4, ~42–43 tok/s aggregate at concurrency 8 | Jesse |

### What the 550B target consumes (arithmetic)
- At 4-bit weights, 550B parameters ≈ 275 GB. At 8-bit ≈ 550 GB.
- On 4 nodes (~476 GB): ≈ 69 GB of weights per node, ≈ 50 GB per node left, which the inference server takes for KV cache. 8-bit does not fit.
- On the 3 live nodes (~357 GB): the planned TP=4/EP=4 layout needs four nodes, so it cannot run as configured. Even re-laid-out, 4-bit weights leave ≈ 27 GB per node.
- Conclusion: the 550B target, as stated, consumes every Spark it runs on.

## Classification dimensions
1. Role: what the part does.
2. Bottleneck resource: memory bandwidth, memory capacity, GPU compute, CPU, or NVMe I/O.
3. Latency class: interactive (a person waits), always-warm (must answer in milliseconds, so it stays loaded), or batch (can wait, can be scheduled).
4. State: canonical (source of truth; must be backed up), derived (rebuildable from canonical), or ephemeral (lost on restart, fine).
5. Binding: the endpoint(s) a profile assigns to the class. Configuration only.

## Workload classes (locked)
Components listed are current candidate bindings, not fixed choices.

| # | Class | Current candidate bindings | Bottleneck | Latency | State | Needs GPU |
|---|---|---|---|---|---|---|
| 1 | Large-model serving | 550B cluster target; 14B primary reasoning model | Memory bandwidth (decode) and capacity (weights + KV cache) | Interactive | Ephemeral | Yes |
| 2 | Always-warm small models | LFG intent model (150–250M, INT8, CPU); LFG boundary model (9B, yes/no logits); 0.6B minions | Latency; small footprint (LFG is CPU) | Always-warm | Ephemeral | Partly (9B and minions yes; LFG intent no) |
| 3 | Embedding and reranking | Harrier 0.6B via TEI (1024-dim) | GPU compute; batches well | Batch at ingest, interactive at query | Ephemeral | Yes |
| 4 | Vector store | RRFlow native vector indexes; `cortex/qdrant` fork (RRO); Qdrant Cloud (used by the rrflow-hydradb prototype) | Memory capacity (vectors + HNSW graph) and CPU; NVMe when on-disk | Interactive query, batch index build | Derived (rebuildable from canonical records) | No |
| 5 | Canonical store / project brain | rrflowKV (chosen); adapters or import sources: HydraDB, SurrealDB v3, Postgres, DuckDB | NVMe I/O, CPU, page-cache memory; durability | Interactive and batch | Canonical | No |
| 6 | Learning: weight updates | LoRA of minions; DPO; Unsloth/Axolotl; distillations (2B → 150–250M, ~27B → 9B) | GPU compute; bursty | Batch, scheduled | Produces versioned model artifacts | Yes |
| 7 | Learning: behaviour updates | Traces → signals → issues → experiments → changes to prompts, skills, routines, configs; Raindrop (already wired into the rrflow-hydradb prototype) | CPU, storage, small classifiers, embeddings for clustering | Continuous / batch | Evidence is canonical; classifiers derived | Little |
| 8 | Control plane and ops | Warden, Proxmox, Authentik, Infisical, Caddy | CPU, small | Always-on | Canonical config | No |

### Rough footprint of classes 2–5 when bound locally (estimate; assumptions stated)
| Part | Assumption | Memory |
|---|---|---|
| LFG intent model | 150–250M parameters, INT8 | 0.15–0.25 GB |
| Harrier embedder | 0.6B, BF16 | ~1.2 GB |
| 0.6B minion | BF16, per model | ~1.2 GB |
| 7B voice model | 8-bit | ~7 GB |
| 9B boundary model | 8-bit | ~9 GB |
| 14B reasoning model | 8-bit | ~14 GB |
| KV cache for the above | modest context and concurrency | ~10–20 GB |
| Vectors (Harrier, 1024-dim) | float32 = 4 KB per vector; 1M vectors ≈ 4.1 GB; int8 scalar quantization ≈ 1 KB per vector | 4–20 GB for 1–5M vectors |
| Database page cache | working sets of the bound stores | 16–32 GB budget |
| Total | | ≈ 70–100 GB |

Roughly one Spark's ~119 GB holds classes 2–5 at single-developer scale. These become declared footprints in bindings and are replaced by measured values once data exists.

### Reliability fact for profiles
On unified memory, an inference server that runs out of memory can take co-located processes down with it. Profiles should bind the canonical store (class 5) to a different machine from large-model serving (class 1).

## Raindrop (reference for class 7)
- What it is: hosted observability and debugging for AI agents. Self-hosting in beta. Pro is $299/mo plus $0.003 per event up to 1M.
- Loop: trace every run and tool call → explicit signals (thumbs, regenerate, copy) and implicit signals (narrow yes/no classifiers for refusals, task failure, laziness, memory failures, user frustration, wins) → cluster into issues and user intents → Deep Search (semantic search for candidates, LLM classifies each) → lightweight classifiers trained on user labels → experiments comparing a variant to a baseline → simulations on pull requests → MCP auto-fix loops that patch, replay the failed trace and verify.
- It does not train model weights. It is class 7, not class 6.
- Already integrated: the rrflow-hydradb prototype records real Raindrop events and feedback with read-back verified (per its TEAM.md, 2026-10-12).
- Mapping to decisions already made: signals ≈ configurable metrics; experiments ≈ eval arms; traces ≈ evidence graph; issue definitions ≈ lexicon and reasoning graph content.

## Open (one decision at a time)
1. How changes land in `rrflow-development` (see `claude/rrflow-build-order.md`).
2. Binding schema details (fields above are a draft).
3. Deep study of Raindrop's loop for class 7 design.

## Sources
- https://docs.nvidia.com/dgx/dgx-spark/hardware.html
- https://www.raindrop.ai/
- https://www.zenml.io/llmops-database/production-monitoring-and-issue-discovery-for-ai-agents
- https://github.com/EonsofStupid/rrflow-hydradb (README.md, TEAM.md, research/HYDRADB.md at 2026-10-12)
- https://github.com/surrealdb/surrealdb/blob/main/LICENSE
