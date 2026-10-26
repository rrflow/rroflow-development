# rrflow-development salvage assessment

Date: 2026-10-13. Revision assessed: `cfa0e80` (2026-09-15, "feat(rrflow): converge installed startup authority").
Method: shallow-to-5000 clone, full `cargo test --workspace --locked --no-fail-fast` on toolchain 1.98.0 (debuginfo off), source and roadmap read.

## Correction (2026-10-15)
This assessment covered only `rrflow/rrflow-development`. It missed `EonsofStupid/rrflow-hydradb` (public, published 2026-10-12): a working TypeScript RRFLOW prototype (Next.js, React Flow Connectome, A2UI guidance) where HydraDB cloud is the authoritative store, Qdrant Cloud holds vectors, Raindrop records real events and feedback, Infisical supplies credentials, and Playwright verifies it (53 cases per its TEAM.md). It was deployed on Modelence with a password-gated owner workspace and ran as an event submission. Its README states classification is conservative routing with human review and that autonomous prompt promotion and model training are not implemented. "No usable version" below applies to the Rust engine only.

## Verdict
Salvageable. Keep the engine. The missing piece is the product loop, not the foundation.

## Measured facts
| Fact | Value |
|---|---|
| History | 498 commits, 2026-08-12 → 2026-09-15 (5 weeks); no commits since. First commit: "vyrm: initial import — claims kernel, substrate adapter, routing projection, operator CLI" |
| Rust | ~200k lines, 20 crates (kernel, persistence, compute, authority, transport, operations, adapters, evaluation) |
| SDKs | Rust plus Python, TypeScript, Go, Java, .NET |
| Tests | 832 passed, 1 failed (`rrflow-cli` `backup_controller_kill_matrix_converges_after_the_archive_effect_gap`: controller exits before the injected hold boundary; backup is a known open D-01 gap) |
| End-to-end proof | `rust_client_negotiates_authenticates_queries_and_reads_audit` writes records and calls `POST /v1/context/assemble` against a real server process; passes |
| Installed product | `installed_engine_plans_applies_serves_discovers_authenticates_and_reopens` passes |
| Docs | 32,870 lines of Markdown; 198 of 498 commits touch no code |
| Paired agent eval (2026-08-18, n=8) | Stale-evidence: Claude 136,117 → 61,573 input tokens, 7 → 1 tool calls. Post-compaction: no gain (102,625 → 104,589). No regressions. |

## What works today (Rust engine)
- `context-assemble`: one scope, explicit valid time, seeds, hard budgets; snapshot BM25 + exact vector + bidirectional graph BFS + reciprocal-rank fusion; per-item evidence digests, read stamp, truncation flag, packet digest; deterministic.
- rrflowKV: WAL, MVCC, segment v6 (Arrow-layout pages, authenticated filters, adaptive LZ4), crash/reopen fault matrix, scan-resistant cache.
- Bi-temporal claims with supersession.
- Embeddings: feature-hash default; optional local FastEmbed ONNX adapter (user-supplied model bytes).
- Install plan/apply, serve, authenticated ready, quick verify; HTTP, WebSocket, MCP (one tool: `rrflow_context`).

## Why the Rust engine has no usable version
1. Dependency spine orders storage perfection (Gate C, C-07) before the product loop. Project ingestion (D-03) is wave 5, native indexes wave 6, DataFusion wave 7, live context wave 10.
2. No ingestion path: nothing turns a project tree or document set into records (D-03 open).
3. No answer synthesis: RRFlow returns context packets only; "reasoning runner remains truthfully unavailable".
4. Process weight: each change requires a planning-only commit, active-change JSON, linked journal, navigation regeneration.

## Position against Pinecone Nexus / KnowQL 0.1.0
| Capability | RRFlow | Nexus / KnowQL 0.1.0 |
|---|---|---|
| Temporal reads (`as_of`) | Shipped (bi-temporal, read stamps) | Future work |
| Budgets | Shipped (items, bytes, depth, scanned keys) | Future work (`budget`) |
| Provenance | Per-item evidence digests | Document-level citations only |
| Determinism | Same stamp → same packet digest | Explicitly probabilistic |
| Local / self-hosted | Yes, single binary target | BYOC only, Enterprise, hyperscaler |
| Ingestion connectors | Missing | Shipped |
| Compiled artifacts (summary/entity/topic/event) | Missing | Shipped |
| Answer synthesis + `shape` | Missing | Shipped |
| Multi-context scope | Single scope per request | 1–10 contexts |

## Path to a working Rust version (as of 2026-10-13)
1. Ingest: project tree / exported knowledge JSONL → records + embeddings through existing data/vector operations.
2. `crates/adapters/rrflow-knowql`: KnowQL 0.1.0 query document → `context-assemble` → provider-neutral model call → `output` / `output_json` + citations from evidence. Retrieval-only modes map to the raw `ContextPacket`. KnowQL `scope` (1–10) must fan out over RRFlow's single-scope operation and fuse.
3. MCP: add a KnowQL query tool beside `rrflow_context`.
4. Demo corpus: RRFlow's own docs; rerun the paired eval at larger n.

## Open decision
Whether to reorder the spine so the walking knowledge loop precedes C-07/E/F, and how much per-change ceremony applies during it. Superseded in part by the class-5 canonical-store question in `claude/compute-and-data-workload-classification.md`.
