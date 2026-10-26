# rrflow.ai public site — spec and build order

Date: 2026-10-19. Asked by Jesse: a true public web app that captures the eye and explains RRFlow (reasoning engine, RROFLOW, rrflowQL, rrflowDB, rrflowKV) with stimulating visuals, available through DevPulse and shippin.cloud. Visual concept: the Design canvas "RRFlow public site concept" (dark instrument look, six scenes). This record is the architecture, the data flow from a public demo estate, the ops item that serves it, and the build order. No visuals here.

Code basis: `rrflow/rrflow-development` `main` at `13ce338` (PR #18 merged 2026-10-19; PR #19 tasks not yet merged), read-only shallow clone. Every engine fact below names its file.

## Decided (2026-10-19)
- Surface: a public web app at rrflow.ai, separate from Connectome; Connectome remains the client, the site remains the story.
- Fidelity rule carried over from the design pass: every live panel reads a real estate; anything the engine does not serve yet is a labeled planned state, never sample data. The published concept marks its own sample panels "SAMPLE"; the built site has none.
- The site never exposes RRFlow to the browser. A gateway on the same host as the engine calls the loopback endpoint with a read-only principal; the browser talks only to the gateway.
- **Decision 1 — repository and names (Jesse, 2026-10-19):** `github.com/rrflow/site-development`, sibling of `rrflow-development` and `connectome-development`, bound to one engine revision by a pin, no submodule. Folders and naming as in "Names" below.
- **Decision 2 — stack (Jesse, 2026-10-19):** Rust gateway (axum + `rrd-client`) and an Astro static site with islands. Rejected: Next.js route handlers with `@rrflow/rrd-client` (credential in a Node process; one language) — revisit only if the Rust gateway slows the site down in practice.
- **Decision 3 — demo-estate host (Jesse, 2026-10-19):** a small VPS running `rrflow serve` + the gateway, static assets on an edge/CDN. Rejected: Gold behind a tunnel (home-network exposure, uptime dependence); Devstation (LAN only).

## Open
- Step 0 bootstrap is the next increment (see Build order). Repository creation under the `rrflow` org is Jesse's action or an explicitly authorized one from his workstation; local scaffolding lands at `D:\localdev\site-development` on Gus (separate folder under `D:\localdev`, like `rrflow` and `connectome`).
- VPS provider, region and size; the edge/CDN; the DNS cut-over for rrflow.ai — chosen when step 1 starts.

## Facts the architecture rests on

| Fact | Where |
|---|---|
| `rrflow serve` binds loopback only: `RrdHttpServer::bind` returns `RemoteBindDenied` for any non-loopback address; the network path is `bind_mtls` (mutual TLS) | `crates/transport/rrd-server/src/http/server.rs:73-95` |
| Endpoint presentations are `in_process`, `loopback_http_websocket`, `network_http_websocket`; a network endpoint "requires the accepted authenticated encrypted transport" and is "an explicit configuration addition" | `docs/reference/deployment/modes.md:34-60` |
| Principals (user, service, node) carry credential-verifier digests, validity windows, direct grants and roles; a grant binds an action to a resource prefix; most-specific grants apply; the installer generates one high-entropy API key; initial principals come from the install plan | `docs/reference/security/authority.md:62-72, 176-187, 221-263` |
| Security actions include `ServiceInspect`, `SessionCreate/Renew/Close`, `DiagnosticsRead`, `FlightRead`, `MemoryContextRead`, `QueryExecute`, `ChangefeedRead`, `ChangefeedFollow`, `VectorSearch`, `VectorCollectionList`, `VectorPointRetrieve`, `VectorPointScroll`, `EstateRead` — a read-only grant set exists | `crates/transport/rrd-contract/src/lib.rs` (`SecurityAction`) |
| 37 public operations at `13ce338`, including `capabilities-read`, `endpoint-catalogue`, `diagnostics-read`, `flight-launch/list/read`, `context-assemble`, `query-execute`, `changefeed-read`, `changefeed-follow`, `subscription-open/close`, `vector-search` | `crates/transport/rrd-contract/src/lib.rs` (operation catalogue) |
| `ProductCapabilityCatalogue`: per capability, one `SurfaceBinding` per surface with disposition `Available | Planned | Denied | Unavailable`, entrypoints and a reason; built from the endpoint catalogue plus six named capabilities; Connectome surface `Planned` everywhere ("scheduled by H-06"); no LFG, SWAG or RROFLOW row | `crates/transport/rrd-contract/src/capability_surface.rs:10-79`; `crates/authority/rrd-engine/src/capabilities.rs:6-99` |
| `AssembleContext {scope, query, valid_at, seeds, max_graph_depth, max_items, max_output_bytes, max_storage_keys}`; bounds: 256 seeds, 512 items, 768 KiB output, 1,000,000 keys, depth 32; the engine captures the read stamp per request (no caller-supplied known cursor) | `crates/transport/rrd-contract/src/memory_context.rs:6-30` |
| `ContextPacket {scope, valid_at, query_sha256, read: ContextReadStamp, plan (five stages: Seed, Lexical, Semantic, Graph, Fusion), selected_versions, read_evidence, items, output_bytes, truncated, packet_sha256}`; each item: `identity, score, values, evidence[]` with per-avenue `kind, source, source_rank, source_score, contribution, plan_sha256, evidence_sha256` | same file, lines 69-117, 252-313 |
| Flights: `LaunchPromptFlight {scope, prompt, provider (Observe only), context_mode (Fresh|Pruned), context, acceptance_marker}`; record stores `read: ContextReadStamp`, `context: {request_sha256, plan_sha256, packet_sha256, items, output_bytes, truncated, selected_versions}`, append-only events `{stage: Prompt|Context|Outcome, kind, label, detail, data}`; a pruned flight writes one `rrflow-prompt-flight-used` edge per record (rank, fused score, avenues) | `crates/transport/rrd-contract/src/flight.rs:48-329`; `claude/rrflow-build-order.md` (PR #18) |
| rrflowQL: `FROM <family>:<kind> AT VALID <n|$p> KNOWN <n|HEAD|$p> [WHERE …] [PROJECT …] [LIMIT n] [EXPLAIN CONTRACT|ANALYZE]`; `FROM traverse:<relation> START <kind>:<id> DIRECTION … DEPTH n`; live queries require `KNOWN HEAD`; traversal is a linear scan until E-01 | `crates/compute/rrd-query/src/syntax.rs:112-150, 700-760, 879-896`; `live.rs:60` |
| Changefeed: `changefeed-read {scope, after_cursor, limit ≤ 4096}` → page with `cursor, commit_sha256, commit_ordinal, at_unix_ms, actor, mutation (Claim | Data), change_sha256`; `changefeed-follow` long-polls ≤ 5,000 ms; `subscription-open` pushes `Changefeed` or `LiveQuery` streams over the multiplexed WebSocket | `crates/transport/rrd-contract/src/lib.rs:2527-2750` |
| `diagnostics-read` → `DiagnosticSnapshot {product_capabilities, graph, graph_difference, retention, vector catalogues, storage, changes, audit, sections}`; `storage.rrflow_kv` = memtable, write_ahead_log, maintenance, segment_set, page_cache, segment_io; counters are process observations, not bound to the read stamp, reset on reopen | `crates/transport/rrd-contract/src/diagnostic.rs:412-503` |
| `diagnostics-read` answers 409 `StorageConflict` (retryable) under concurrent audited traffic — POAM-029, open | `claude/rrflow-build-order.md` (step 0) |
| Vector search replays the change log per search — POAM-028, open; the demo estate's similarity map took 46.9 s for 270 searches | `claude/rrflow-build-order.md` |
| No embedding backend is installed (semantic stage always skipped); no ingestion (D-02..D-05); provider arms planned (Decision 15); RROFLOW and SWAG have no record | `claude/rrflow-capability-study.md`; `claude/rrflow-build-order.md` |
| Demo estate today: RRFlow's own docs loaded by a dev harness — 270 documents, 986 `links_to` relations, 270 lexical 256-d vectors | `claude/rrflow-build-order.md` (Development environment) |
| Connectome's contract-pin discipline: `Cargo.lock` pins one RRFlow revision; `vite.config.ts` reads it; the dependency-authority gate fails if the npm and Cargo pins differ; the TypeScript SDK is a pnpm git dependency `github:rrflow/rrflow-development#<rev>&path:sdks/typescript` | `claude/rrflow-build-order.md` (Connectome PRs #2, #4) |
| Config format decided for the platform: TOML validated by JSON Schema generated from Rust types with `schemars` | `claude/skills-and-routines-inventory.md` |

## Architecture

```
browser ──HTTPS──▶ edge (TLS, cache, rate limit)
                     ├─ static site (Astro build: HTML, CSS, islands)
                     └─ /api/*  ──▶ site gateway (Rust, axum, rrd-client pinned to one revision)
                                        │  loopback HTTP, read-only principal `site-reader`
                                        ▼
                                  rrflow serve (single_node_server · rrflow_kv · loopback_http_websocket)
                                        │
                                  demo estate (instance = rrflow.ai demo project)
```

Why this shape and not another:
- The engine stays on its loopback presentation, which is the shipped, qualified one; no `network_http_websocket` configuration, no mutual-TLS client in a browser, no public RRFlow port. The gateway is the only RRFlow client and holds the only credential.
- The gateway is where abuse is absorbed: an allowlist of operations, per-IP and global rate limits, single-flight caches, a small engine concurrency pool (POAM-029 makes concurrent audited reads conflict — the pool turns that into queueing, not 409s on screen).
- The site is static-first. Scenes render with real numbers from the gateway's cached JSON; the GPU field is an island that loads after first paint and degrades to an SVG frame under `prefers-reduced-motion` or without WebGL2.

### Names (Decision 1, decided)
- Repository: `github.com/rrflow/site-development`. Local checkout: `D:\localdev\site-development` on Gus.
- Folders: `site/` (Astro app), `gateway/` (Rust crate `site-gateway`, binary `rrflow-site-gateway`), `estate/` (install plan, principal declarations, canonical prompts, rebuild script), `config/` (TOML + generated JSON Schema), `scripts/` (pin check, estate rebuild).
- Everything a human may want to change is configuration, not code: engine base URL and scope, credential reference (secret name, never a value), cache TTLs, rate limits, budgets for `context-assemble`, the canned rrflowQL set, the canonical prompt set, scene order and copy keys. Defaults ship as TOML; the gateway's Rust types are the schema source (`schemars`), and the site's TypeScript types for `/api/*` are generated from that schema.

### Stack (Decision 2, decided)
**Rust gateway (axum + `rrd-client`) and an Astro static site with islands** — three.js WebGPURenderer/TSL with WebGL2 fallback, or cosmos.gl, for the field; GSAP ScrollTrigger + Lenis for the scroll flight. Reasons: the credential and the engine calls live in Rust beside the engine, the browser needs no RRFlow SDK, the API surface is small and typed from one Rust source, and it matches the platform's Rust-first rule. The field library (three.js vs cosmos.gl) is chosen at step 4 by a spike on the real graph snapshot, not now.

### Hosting the demo estate (Decision 3, decided)
A small VPS runs `rrflow serve` (loopback) and the gateway; static assets ship to an edge/CDN; only the gateway port is reachable from the edge; the `site-reader` credential comes from Infisical as a reference; backups run on the D-11 lane (functional for installed estates). The estate is disposable and rebuilt from the docs, so the VPS carries no irreplaceable state.

## Data flow per scene (what each panel reads)

| Scene | Panel | Gateway route | Engine operation(s) | Cache / limits | Labeled planned until |
|---|---|---|---|---|---|
| global | live / planned badges | `GET /api/capabilities` | `capabilities-read` → `ProductCapabilityCatalogue` dispositions | 1 h | LFG, SWAG, RROFLOW rows: the catalogue has none, so a `config/capabilities-overlay.toml` supplies them with `disposition = "planned"` and a reason; upstream ask: add the rows to the engine's catalogue |
| hero | estate field | `GET /api/estate/graph` | `diagnostics-read` (`graph` section) + `vector-point-scroll`; clusters computed in the gateway (Louvain, as Connectome does today) | 10 min; precomputed at estate rebuild | million-record scale (ingestion, E-01) |
| hero | flight trail + packet seal | `GET /api/flights/{id}` | `flight-read` (record, `rrflow-prompt-flight-used` edges, events) | immutable → cache by id | — |
| 01 engine | stage rail, Flight Ledger | `GET /api/flights`, `GET /api/flights/{id}` | `flight-list`, `flight-read` | list 60 s; detail immutable | answer column (no provider arms); semantic avenue "skipped: no embedding model bound" |
| 01 engine | "type a prompt" | `POST /api/context` | `context-assemble` (`MemoryContextRead`), no persistence | per-IP 6/min, global pool 2, budgets from config (defaults: items ≤ 24, depth ≤ 2, storage keys ≤ 100,000, output ≤ 128 KiB); result cached by `(query_sha256, read.runtime_cursor)` 60 s | semantic stage |
| 02 rrflowQL | query + scrubber | `POST /api/query/{name}` | `query-execute` with canned, parameterized statements only (`$valid_at`, `$known`); `EXPLAIN CONTRACT` for the plan panel | 30 s; free-form statements refused | — |
| 03 rrflowDB | commit anatomy | static (from the contract) | — | — | — |
| 03 rrflowDB | neighbourhood | `GET /api/graph/neighbourhood?start=&depth=` | `query-execute` `FROM traverse:links_to START … DEPTH n`; depth ≤ 2 | 60 s | fast traversal (E-01) |
| 04 rrflowKV | counters, pipeline | `GET /api/storage` | `diagnostics-read` (`storage.rrflow_kv`) | 5 s single-flight; on 409 serve the last good value with `stale: true` and age | — |
| pulse | commit ticker | `GET /api/pulse` (SSE) | one `changefeed-follow` loop in the gateway, fanned out to browsers | one follower per gateway; browsers ≤ 1 event/s | — |
| 05 RROFLOW | loop, tiers, promotion | static (contract enums) | — | — | the whole scene: IN DESIGN |
| get it | DevPulse / shippin.cloud / Connectome | static; waitlist `POST /api/waitlist` | — | per-IP 3/h; store in the gateway's own store, not the estate | shippin.cloud: COMING |

Every live panel carries a source chip (operation + read stamp or observed_at) rendered by one component; a panel without a source renders the planned state from the catalogue or overlay. This is the one rule that keeps the site honest as the engine grows.

What a visitor can and cannot do: read everything above; assemble context for a prompt they type (a read, not a flight); never write — flights, tasks, claims and vectors on the demo estate are owner-launched during estate rebuild.

## The demo estate
- Content: RRFlow's own documentation tree, as today (270 documents, 986 `links_to`, lexical vectors) loaded by the harness, until D-02..D-05 ingestion replaces the harness. Rebuild is a script in `estate/`, idempotent, run on each engine pin bump.
- Flights: a canonical prompt set in `estate/prompts.toml` (12 prompts about RRFlow, each answerable from the docs) launched with the `Observe` provider in `Pruned` mode after each rebuild, so the ledger always has real flights with real edges. When provider arms land (Decision 15), the same set is re-flown with a provider and the answer column goes live.
- Principals in the install plan: `operator` (existing), `site-reader` (service principal; grants: `ServiceInspect`, `SessionCreate`, `SessionRenew`, `SessionClose`, `EstateRead`, `DiagnosticsRead`, `FlightRead`, `MemoryContextRead`, `QueryExecute`, `ChangefeedRead`, `ChangefeedFollow`, `VectorCollectionList`, `VectorPointRetrieve`, `VectorPointScroll`, `VectorSearch`; no transaction, flight-launch, backup or admin actions), `estate-builder` (used only by the rebuild script; flight-launch and transactions).
- Semantic lane: off until the estate binds an embedding model (Decision 17: embedded `fastembed` or an endpoint); the gateway reads the capability catalogue to flip the label, not a config flag.

## Non-functional requirements
- Performance: static HTML first paint; the GPU island loads after; scroll work budgeted (target < 4 ms/frame on a mid-range phone); adaptive quality by device tier; Lighthouse ≥ 90 on mobile before launch.
- Accessibility: `prefers-reduced-motion` → static scenes; real links and buttons; contrast ≥ 4.5:1 (the concept's palette passes); every SVG labeled.
- Privacy: no third-party trackers; the waitlist stores an email and a timestamp, nothing else.
- Truth: no speed or superiority claim against SurrealDB, Qdrant or HydraDB until J-04 passes (OBJ-10 not established).
- Resilience: the site renders with the gateway down (last-good JSON baked at build time, marked stale); the gateway renders with the engine busy (409 → stale with age).

## Build order (each step one PR in `site-development`, CI green before merge)
0. Bootstrap (next): repository, folders, Astro shell, Biome, Playwright smoke, gateway crate skeleton, the contract pin (one engine revision in `gateway/Cargo.lock`, a check script that fails the build when any other file names a different revision), design tokens exported from the concept canvas.
1. Demo estate on the VPS: `rrflow install plan|apply` with the three principals, `serve` on loopback, rebuild script (docs load + canonical flights), backup schedule on the D-11 lane, Infisical reference for the `site-reader` key.
2. Gateway v1: `/api/capabilities`, `/api/storage`, `/api/flights`, `/api/flights/{id}`, `/api/estate/graph`, `/api/pulse`; caches, single-flight, rate limits, engine pool; `/healthz`; JSON Schema from the Rust types and the generated TypeScript types for the site.
3. Site v1, no GPU yet: all six scenes with live panels and planned badges; hero field drawn as SVG from `/api/estate/graph`; source chips everywhere; deploy to rrflow.ai behind the edge. This is the first public release: honest and complete, not yet spectacular.
4. Hero field island: cosmos.gl or three.js WebGPURenderer with WebGL2 fallback (spike decides); flight trail from `/api/flights/{id}`; reduced-motion and no-WebGL fallbacks keep step 3's SVG.
5. Scroll choreography: GSAP ScrollTrigger + Lenis; stage lighting on scene 01; mobile frame budget verified on a real phone.
6. "Type a prompt" (`/api/context`) with budgets and rate limits; the packet renders through the same Flight Ledger component.
7. rrflowQL scene: canned parameterized queries, VALID/KNOWN controls, `EXPLAIN CONTRACT` plan panel.
8. Launch polish: OG images, sitemap, Lighthouse and accessibility passes, uptime check on `/healthz`.
Then, as the engine grows (no site rework needed — the catalogue and the gateway absorb the change): embedding bound → semantic lane live; provider arms → answer column live; tasks (PR #19) → task threads in the ledger; ingestion → "point it at your repo" demo; LFG/SWAG/RROFLOW catalogue rows upstream → badges flip from overlay to engine truth.

## Upstream asks (engine repo, not site work)
- Capability rows for LFG, SWAG and RROFLOW in `product_capability_catalogue()` with `Planned` dispositions and reasons, so the site stops carrying an overlay.
- POAM-029 fix (diagnostics under audited traffic) — the gateway's cache and pool are the mitigation, not the fix.
- A caller-supplied known cursor for `context-assemble` (or an engine replay operation at a flight's recorded stamp), the capability the design's "replay seal" wants; today only `valid_at` is caller-supplied.

## Sources
- `rrflow/rrflow-development` at `13ce338`: `crates/transport/rrd-server/src/http/server.rs`, `crates/transport/rrd-contract/src/{lib.rs, memory_context.rs, flight.rs, diagnostic.rs, capability_surface.rs}`, `crates/authority/rrd-engine/src/capabilities.rs`, `crates/compute/rrd-query/src/{syntax.rs, live.rs}`, `docs/reference/deployment/modes.md`, `docs/reference/security/authority.md`
- Project records: `claude/rrflow-build-order.md`, `claude/rrflow-capability-study.md`, `claude/skills-and-routines-inventory.md`, `claude/compute-and-data-workload-classification.md`
- Rendering and platform facts: https://web.dev/blog/webgpu-supported-major-browsers (WebGPU in Chrome 113+/Edge, Firefox 141+, Safari 26; Linux in progress), https://github.com/cosmograph-org/cosmos (MIT, WebGL2 GPU force layout), https://tympanus.net/codrops/2024/10/30/interactive-3d-with-three-js-batchedmesh-and-webgpurenderer/ (three.js WebGPURenderer + TSL, WebGPU-only post-processing caveat)
