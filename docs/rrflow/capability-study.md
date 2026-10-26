# RRFlow capability study: SurrealDB and Qdrant capabilities for the developer devspace

Date: 2026-10-16. Basis: `rrflow/rrflow-development` `main` at `b275792`. Method: read-only audits of code, tests, roadmap gates, the POA&M, and the repo's own capability inventories. Nothing was built or benchmarked for this study. Jesse asked for three things: a feature map, the path to real-scale speed, and the path to a usable brain, delivered as this project doc.

## What RRFlow is for
Jesse (2026-10-16): RRFlow is "a collection of micro optimizations to bring together a smooth devspace and environment for the developer/context engineer."

SurrealDB-class and Qdrant-class capabilities are the means to that end. One engine holds a project's records, the links between them, and their vectors, so the context a developer or agent needs comes back fast and exact.

The repo's design matches this. rrflowDB is "one persistent project AI estate". Records, relations, BM25 and vectors "share one transaction coordinator, read-stamp model, canonical runtime log, and persistence authority" (`docs/architecture/system-overview.md`).

## Short answer
- **Designed: yes.** Two documents map every SurrealDB and Qdrant capability family to an RRFlow rule and the gate that must prove it:
  - `docs/research/surrealdb-capability-inventory.md`
  - `docs/research/qdrant-capability-inventory.md`
- **Built natively: only the storage foundation.**
  - Done: Gate A 7/7, B 5/5, C 6/7. Done in D through J: none.
  - Native today: the key grammar, the two storage profiles, and snapshot-isolation transactions.
  - Everything above storage runs on interim paths that read far more than a request needs. Graph traversal, indexes, vector search, analytics, and live results run end to end. Hybrid retrieval runs only inside the engine; it has no HTTP route.
- **Missing as a product:**
  - Nothing brings a project in (Gate D).
  - The installed server has no embedding model, so the semantic stage of context assembly is always skipped.
  - Nothing generates answers, and no roadmap item asks for it.
- **No proof of speed against either product.**
  - No benchmark against SurrealDB or Qdrant exists. The objective for comparative evidence, OBJ-10, is "Not established".
  - The only live estate on record holds 270 records. It was loaded by a dev harness, not by RRFlow ingestion.
  - The largest measured vector corpus is 10,000 × 128. It was measured at library level, on the previous HNSW format.

| Inventory | Native | Works, not native | Partial | Missing | Rejected by design |
|---|---:|---:|---:|---:|---:|
| SurrealDB v3.2.4 (25 families) | 3 | 5 | 14 | 2 | 1 |
| Qdrant v1.19.1 (16 families) | 0 | 3 | 11 | 0 | 2 |

What the statuses mean:
- **Native:** runs on the intended path, and its gate item is checked.
- **Works, not native:** runs end to end on an interim path, and the gate is open.
- **Partial:** some parts exist.
- **Missing:** not there.
- **Rejected by design:** the repo says RRFlow will not adopt it. Examples are SurrealQL compatibility, a Qdrant-compatible API, and the namespace hierarchy.

## 1. Feature map

### Already native (the foundation)
- **Ordered key grammar.** `RRKV0001` in `crates/persistence/rrd-store/src/key_codec.rs`. Malformed keys fail closed (C-01, C-05).
- **Two storage profiles, one behavior.** rrflowMX and rrflowKV run the identical transaction corpus (`rrflow_mx_and_rrflow_kv_execute_the_identical_transaction_corpus`).
- **Transactions.** Snapshot isolation with write-conflict detection. Runtime commits compare-and-swap the commit cursor, so they serialize (C-02, C-03).
- **One commit batch.** A single batch writes the change log, versions, both adjacency directions, scalar and unique entries, BM25 and vector deltas, the outbox, and audit (`crates/persistence/rrd-store/src/access/semantic_commit.rs`). This is the write side of Gate E. The read side does not exist yet.
- **rrflowKV segment v6.** Arrow-layout pages, persisted filters, and a scan-resistant page cache (C-06).

### SurrealDB capability families

| Family | RRFlow today | Status | Gap | Gate |
|---|---|---|---|---|
| One modular engine | The product binaries (`rrflow`, `rrd-server`, `rrflow-mcp`, the estate controllers) go through `RrdEngine`. The cluster node and a durability test binary open the store directly | Partial | Recorded bypasses: security writes storage directly and audit commits are split (POAM-016), a local estate authority (POAM-017), estate and process control reach the store directly (POAM-019, POAM-021), the cluster runtime (POAM-023), the Kubernetes controller (POAM-024) | A-07, C-02 |
| Storage abstraction | rrflowMX and rrflowKV only | Native | — | C-01–C-05 |
| Ordered key grammar | `RRKV0001` | Native | BM25 term families are defined but unused | C-01 |
| Model families | Records, relations, events, vectors, series, geo, objects | Partial | Values are stored as JSON bytes; reasoning trees are not persisted | C, E, G |
| Multi-model commit | One batch, as above | Partial | No posting or HNSW deltas, no reasoning family, no commit-derived live impact | E, G-04, H-03 |
| Transactions and isolation | Snapshot isolation plus write conflicts | Native | J-02 qualification | C-02, C-03 |
| Physical write fan-out | Full effect set in one plan; up to 4,096 mutations per commit | Partial | No admission estimate of keys, bytes, time, or memory | C-07, F-04 |
| Graph traversal | Adjacency keys are written but never read. Traversal loads every relation of the kind and every record, then scans linearly | Works, not native | Adjacency range reader | E-01 |
| Scalar and unique indexes | Entries are committed, but the planner decodes a whole JSON index artifact | Works, not native | Native seek and range read | E-02 |
| Schema and stored behavior | Revisioned schema; QuickJS and Wasm functions | Partial | Engine events, triggers, routines, skills | D-02, H-05, I |
| Full-text and vector indexes | BM25 is a JSON artifact or is rebuilt for each query and each context call. HNSW is built only on an explicit call | Works, not native | Persisted postings; HNSW generations committed atomically | E-03, E-04 |
| Index lifecycle | Generations and states exist, but a build is one synchronous full query | Partial | Leases, checkpoints, resumable batches | C-07, E, I-03 |
| Query planning | Read stamp first, policy rewrite, rule-based choice | Partial | Cost and selectivity; security checked when candidates are produced | E-05, F |
| Query languages | rrflowQL. GraphQL lowering exists but has no route | Partial | GraphQL route. SurrealQL and GQL are rejected | H-04 |
| Analytical execution | DataFusion 55 runs over fully materialized rows; every filter is `Unsupported` | Works, not native | Streaming provider and pushdown | F-01, F-02 |
| Live results | The query runs twice and the results are diffed, polled every 25 ms | Works, not native | Commit-impact deltas | H-03 |
| Security | API keys, JWT, grants, row and field policy as a query rewrite | Partial | Authorization per effect. Anonymous loopback mode is an open deficiency (POAM-016) | D-01, H-04, H-05 |
| Extensions and models | JS and Wasm functions, an embedding registry, router contracts | Partial | Router and LFG execution; the reasoning runner reports `Unavailable` | G, I |
| RPC, SDKs, MCP | 33 HTTP operations; Rust plus 5 generated SDKs; MCP has one tool; WebSocket dispatch fails closed | Partial | Shared dispatch across surfaces | H-04, H-07 |
| Install and readiness | `rrflow` version, install plan/apply, serve, ready, verify quick | Partial | Attunement, full verify, signed bundles, Windows build | D-01–D-06 |
| Ops and recovery | WAL recovery, compaction counters, logical archive, backup, restore into a new directory | Partial | Restore publication, repair, salvage, uninstall | C-07, D-09–D-11 |
| Interactive UI | Connectome, kept in its own repository | Missing here (separate by design) | Client built only on public operations | H-06 |
| Topology and multitenancy | One `instance:<id>` scope | Rejected by design | — | — |
| Distributed operation | OpenRaft behind a non-default feature; `clustered_server` unavailable | Partial | Engine-owned apply; multi-host proof | No gate yet |
| SurrealDB 3.3 preview features | Watchpoint only | Missing | PostgreSQL wire protocol, gRPC | None |

### Qdrant capability families

| Family | RRFlow today | Status | Gap | Gate |
|---|---|---|---|---|
| Data model: points, payloads, named dense, sparse, and multi-vectors | All present. The collection catalogue is one JSON control key outside the runtime commit | Works, not native | Catalogue inside the commit | E-04 |
| Updates and visibility | Vector, head, and source delta are written in one commit; HNSW has an exact overlay | Partial | Nothing in production reads the source deltas | E-04, H-03 |
| Segment lifecycle | rrflowKV segments and compaction; immutable artifact generations | Partial | Background vector optimizer. Reclaim is never called. Canonical vectors are stored as JSON | C-07, E-04 |
| Exact and approximate (HNSW) search | Exact search, HNSW, exact rerank; HTTP `vector-search` | Works, not native | Every search reads every vector version in scope and replays the change log twice (see Corrections) | E-04, E-05 |
| Filtered approximate search | Filters checked during traversal; cost-based switch to exact | Partial | Physical payload indexes, payload-aware edges, ACORN | E-02, E-05, F-02 |
| Lexical and sparse retrieval | Sparse and multi-vectors scored exactly only; BM25 through `MATCH` | Partial | Sparse inverted index; persisted postings | E-03 |
| Hybrid and multi-step query | RRF, MMR, recommend, discover, groups, facets, matrix, all in Rust collections; engine-only | Partial | DBSF, score formulas, HTTP surface, native operators | F-03, H-01, H-04 |
| Quantization | Scalar, product, binary, TurboQuant, with a lifecycle | Works, not native | Full rebuild every time | E-04 |
| Memory placement | Pinned, LRU, or cold mmap per named vector | Partial | Placement per component; HNSW stored as JSON cannot be mmapped | F-04, F-05 |
| Multitenancy | One scope per instance | Rejected (alpha) | — | — |
| Distribution and recovery | Backup and restore; OpenRaft behind a non-default feature | Partial | Qualified replication | J plus a new gate |
| Resource protection | Key caps, read budgets, vector pressure | Partial | Time, memory, and spill budgets; write quotas | F-04, J-02 |
| API and SDKs | 5 vector operations over HTTP; no gRPC | Rejected (Qdrant compatibility) | HNSW ensure, quantization, payload indexes, retrieval, and collection delete have no HTTP route, so approximate search cannot be set up over HTTP | H-04 |
| Inference | Feature-hash backend; FastEmbed (ONNX) behind a feature the engine does not enable | Partial | The server installs no backend; no remote provider | D-05, G |
| Edge | Offline mmap exact index | Partial | Writes, snapshots, sync, approximate search | H-07, J |
| Security and operations | Loopback by default; mutual TLS; API key and JWT; health and diagnostics | Partial | Installed lifecycle; metrics and OpenTelemetry | D, H-05 |

### Corrections to the repo's own records (found by this study)
1. **Vector search contradicts the C-04 closure claim.**
   - Each search calls `reopen_vector_runtime_metadata` (`crates/authority/rrd-engine/src/engine/vector/search.rs`, `crates/authority/rrd-engine/src/runtime/vector_catalog.rs`).
   - That function replays the scope's change log from cursor 0 twice:
     - through `vector_artifact_catalog_entries`, using `changes_since`;
     - through `quantization_artifact_catalogue`, using `read_changes`.
   - The C-04 closure test (`crates/authority/rrd-engine/tests/direct_read_source_closure.rs`) misses both replays:
     - It never searches for `changes_since(`. The same blind spot covers a cursor-0 replay in `crates/operations/rrd-maintenance/src/lib.rs`.
     - It allowlists the `read_changes(` call as an "explicit quantization projection-catalogue rebuild", although that call runs on every search.
   - The C-04 evidence says vector-search admission has no causal-log source (`docs/roadmap/rrflow-1.0/gate-c-evidence.md`).
   - `docs/reference/vector/search.md` does mention catalogue reconstruction before planning. Still, the POA&M has no entry for it, and no test catches it.
   - Search cost therefore grows with the estate's whole history.
   - The repo's rules require a verified deficiency to be added to the POA&M.
2. **Facts tables partly stale.** Both inventories' "current implementation facts" tables were revised on 2026-09-09 to 2026-09-12 as C-02 to C-04 closed, but several rows were left stale.
   - SurrealDB inventory: lines 197, 201, 202. For example, "Relations are keyed only by relation identity": adjacency entries are now written in the same commit.
   - Qdrant inventory: line 162, which names a symbol that has been removed.
3. **Unused writes.** Adjacency entries and BM25 and vector deltas are written at commit, but only tests read them.
4. **A collection with an HNSW index can never be deleted.**
   - Delete rejects any collection whose artifact catalogue has an `hnsw-{collection}-{vector}` entry (`crates/authority/rrd-engine/src/engine/vector/collection.rs`).
   - The catalogue returns every generation ever published, with no retirement filter.
   - So even a future HNSW retire operation would not unblock deletion. RrdEngine has no such operation today.
   - This is derived from the code; no test covers it.
5. **Vector reference docs disagree on C-04.** `memory-tiers.md`, `hnsw-projection.md`, `compact-dense-artifact.md` and `context/retrieval.md` treat it as future work. `search.md` says it is done.

## 2. Real-scale speed: the micro-optimizations still to land
Each row is one developer moment, what the engine does now, and the gate item that fixes it.

| Developer moment | What the engine does today | Fix | Proof the gate requires |
|---|---|---|---|
| Find similar code or docs | Reads every vector version in scope, JSON-decodes and proof-checks each one, then runs two full change-log replays per search | E-04: atomic vector delta and persisted HNSW generation. E-05: cost planner. Reading the artifact catalogue directly, without replay, is this study's addition; E-04's text does not name it | Recall@k, filtered search, reopen, interrupted build, RSS and latency on declared corpora |
| Follow links | Loads every relation of the kind and every record, then scans linearly at each step | E-01: adjacency prefix reader over keys already written | Work grows with edges visited, not with estate size |
| Filter or look up by field | Decodes the whole JSON index artifact | E-02: native scalar and unique seek (the write side exists) | Index and record never drift, including across reopen |
| Keyword search | rrflowQL reads a whole JSON BM25 artifact when a BM25 query index exists, and otherwise rebuilds BM25 for the query. Context assembly rebuilds BM25 on every call | E-03: persisted incremental postings | Incremental result equals a full rebuild |
| Assemble context for an agent | Reads the whole scope, rebuilds BM25, runs breadth-first search both ways, and fuses with RRF (k = 60). Past its key budget it fails rather than truncating. The default estate ceiling and the MCP tool's default are both 100,000 keys | E-01 to E-05, F, and H-01 routing | Plan evidence for each retrieval avenue |
| Analytics in rrflowQL | Materializes every row in memory; no filter pushdown | F-01 and F-02: streaming provider and pushdown | Fixed memory when the data exceeds query memory |
| Watch changes live | Runs the query twice and diffs, every 25 ms | H-03: commit-impact deltas | Each delta delivered once, with no rescans |
| Write | 98,356 ops/s median (claim appends in batches of 128). WAL sync takes 69.70% of physical write time at p50. Has not passed the strict Fjall comparison | C-07 qualification | Crash matrix; sustained-write RSS |

Unverified estimate (derived from code by the audit, not measured): a scope can hold about 50,000 vector versions before one search exhausts the 1,000,000-key budget.

The only vector scale evidence is historical, and the repo labels it that way. `docs/evidence/m5-vector-local-10000x128.json` (10,000 × 128) was measured at library level on the previous HNSW format (`e79aa87`, crate `vyrm-vector`). Nothing measures the engine path.

Before RRFlow can claim to be as fast as Qdrant or SurrealDB, the repo's comparative-evidence contract requires fixed-hardware benchmarks (J-04, POAM-012). Until then the repo states that it "makes no ease, performance, durability, recall, or superiority claim against Qdrant", and the same for SurrealDB.

## 3. Usable brain: data in, answers out

| Step | Today | Missing | Gate |
|---|---|---|---|
| Install | `rrflow` install plan, apply, serve, ready, verify quick (built from source) | Signed bundle, Windows build, hostile paths and ACLs | D-01 (active) |
| Bring a project in | Generic transactions API: up to 4,096 mutations per commit, 1 MiB per HTTP body. A script exports Markdown to JSONL, but nothing imports it | Attunement executor (D-02), tree inventory (D-03), Tree-sitter parsing (D-04), the normalize → embed → index → graph phases (D-05), knowledge importer (KB-06, POAM-001) | D-02 to D-05 |
| Embed | Feature-hash backend; FastEmbed (ONNX) behind a feature the engine does not enable | The installed server never installs a backend, so the semantic stage is always skipped; no remote provider | D-05, G |
| Retrieve context | `context-assemble` returns a `ContextPacket` with a read stamp, evidence, and digests. MCP exposes one tool, `rrflow_context` | Native indexes; hybrid retrieval has no HTTP surface | E, F, H |
| Answer | Nothing. The reasoning runner reports `Unavailable`. ADR-0002 leaves synthesis to the calling model | An answer step; no roadmap item exists | Decision needed |
| See it | Connectome lenses (separate repository) | Plain-language lenses parked on `rrflow/connectome-development` branch `agent/layman-lenses-wip` (`f5b2cb4`) | H-06 |

The decided build order already runs ingestion on today's indexes rather than waiting for E and F (`claude/rrflow-build-order.md`). The repo's dependency spine still places D-05 after E and F.

## 4. Hosted plus local copy (Jesse's v1 plan) vs the repo
- **Stated plan:**
  - A hosted instance per shippin.cloud user, plus an optional local-first copy, with two-way binary sync.
  - RRFlow does no authentication; the upstream identity provider handles it.
- **Repo today:**
  - A single-node server is the only service shape.
  - `rrflow serve` binds loopback only: `RrdHttpServer::bind` returns `RemoteBindDenied`.
  - Network serving needs the internal `rrd-server` binary with mutual TLS.
  - "Exactly one instance serves one estate", with an exclusive writer.
  - No sync is specified. `deployment-mobile-edge` and `managed-cloud-control-plane` are listed as planned, with no implementation.
- **Authentication:**
  - The installed path requires credentials.
  - A credential-less loopback mode exists at library level, and the POA&M counts it as a deficiency (POAM-016).
- **Both points conflict with the stated plan.** Each needs its own decision record (parked).

## 5. Plan: small increments
Each increment is one pull request with green CI and one journal. The decided order stays:

1. Recoverability
2. Config revisions
3. Ingestion on today's indexes
4. Answers
5. Prototype UI

This study adds two increments the repo's own rules require, and leaves one placement open.

| # | Increment | Why | Size |
|---|---|---|---|
| 0 | Add `changes_since(` to the C-04 closure test and record the vector-search replay in the POA&M; refresh both inventories' facts tables | A verified deficiency must enter the POA&M; status must match code | Small |
| 1 | Recoverability (in progress): installed backup path; restore into an absent target with publication | Decided step 1 (D-11) | Medium |
| 2 | Managed config revisions | Decided step 2 | Medium |
| 3a | Install a configured embedding backend in the server (local FastEmbed, model bound by digest) | Without it the vector DB holds only caller-supplied vectors and context never uses meaning | Small |
| 3b | Knowledge importer (KB-06), then D-02 executor, D-03 inventory, D-04 parsing, D-05 phases | Decided step 3; projects flow in | Large, several PRs |
| S | Speed wins whose write side already exists: E-01 adjacency reader, E-02 scalar and unique reader, then the vector catalogue read directly (no replay) | Context and search stop scanning the whole estate | Medium each |
| 4 | Answers through bound model endpoints | Decided step 4; needs a roadmap item | Medium |
| 5 | Prototype UI on RRFlow's HTTP API; HydraDB data imported | Decided step 5 | Medium |
| 6 | C-07, E-03 to E-05, F-01 to F-05, H-03; then the comparative benchmark (J-04) | Real-scale speed, and the right to claim it | Large |

### Decisions (one at a time)
1. **Decided (Jesse, 2026-10-16):** the speed wins (row S) land right after config revisions and before ingestion. Ingestion will grow the estate, and today's paths read the whole estate on every request.
2. **Parked:** the answer step. The engine could call bound model endpoints, or the calling model synthesizes (ADR-0002).
3. **Parked:** hosted plus local sync, and the no-auth stance. Each needs a decision record.

## 6. Upstream drift
- **SurrealDB:** pinned v3.2.4.
  - Newest stable tag: v3.3.0. The pinned 3.2 line also has a patch, v3.2.5.
  - The inventory treats 3.3 features (PostgreSQL wire protocol, gRPC streaming, object backends, bitmap fusion, prefiltered vector work) as a watchpoint, to be re-pinned when a relevant gate begins.
- **Qdrant:** pinned v1.19.1; newest stable tag v1.19.2, a patch release.
- Source: `git ls-remote --tags` against each GitHub repository on 2026-10-16.

## Not verified by this study
- Whether HTTP backup and restore works on an installed estate end to end.
- Engine-path vector latency, recall, and RSS at any size.
- The corpus size at which default context assembly exceeds its key budget.
- Whether the `rrd-cluster` tests currently pass.

## Sources (repository paths at `b275792`)
- `docs/research/surrealdb-capability-inventory.md`, `docs/research/qdrant-capability-inventory.md`
- `docs/roadmap/rrflow-1.0/gate-c.md` to `gate-j.md`, `executable-dependency-spine.md`, `current-alpha-objective-and-remediation.md`, `docs/poam/rrflow-1.0-alpha.md`
- `docs/architecture/system-overview.md`, `docs/architecture/instance-topology.md`, `docs/reference/deployment/modes.md`
- `crates/authority/rrd-engine/src/runtime/vector_catalog.rs`, `crates/authority/rrd-engine/src/engine/vector/search.rs`, `crates/authority/rrd-engine/src/engine/vector/collection.rs`, `crates/authority/rrd-engine/tests/direct_read_source_closure.rs`, `docs/roadmap/rrflow-1.0/gate-c-evidence.md`
- `crates/persistence/rrd-store/src/access/semantic_commit.rs`, `crates/persistence/rrd-store/src/repository/runtime.rs`, `crates/compute/rrd-query/src/fusion.rs`, `crates/compute/rrd-query/src/live.rs`, `crates/transport/rrd-server/src/http/server.rs`, `crates/transport/rrd-contract/src/lib.rs`
- Evidence: `docs/evidence/change-journals/gate-c/c-07-native-batch-write-tail-attribution.md`, `docs/evidence/m5-vector-local-10000x128.json`, `docs/evidence/m6-edge-local-10000x128.json`, `docs/evidence/change-journals/gate-h/h05d-rrflowkv-storage-diagnostic-observation-journal.md`
- Upstream tags: https://github.com/surrealdb/surrealdb, https://github.com/qdrant/qdrant
