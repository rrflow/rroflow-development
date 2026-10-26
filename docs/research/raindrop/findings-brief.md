# What we learned from Raindrop — findings for validation

Prepared by Jesse (hades), RRFlow, 2026-10-24. Written to be read by Raindrop's team. Purpose: you tell us where we read your product right and where we read it wrong, so the learning layer we are designing (RROFLOW) borrows the right lessons and credits them correctly.

**Labels used on every claim.** `[public]` — read from raindrop.ai docs, blog, Query API OpenAPI, or the `raindrop-ai` GitHub org on 2026-10-24. `[observed]` — seen in our own workspace (project `rrflow-hydradb`). `[inferred]` — our reading, not stated by Raindrop. Nothing here was obtained by probing anything beyond public pages and our own account.

## 1. What we think Raindrop is

- Production observability for AI agents: events (one turn: input, tool calls, output), OTLP traces linked to events, signals that label events, issues and stumbles discovered automatically, experiments over logged cohorts, a Triage agent, Workshop for local debugging, an MCP server, a Query API. `[public]`
- The mechanism that matters most to us is Signals 2.0 (rd-signal-2, 2026-08-11): for each described behaviour an agent writes numbered policy rules and compiles a classifier as `run(event)` code that gathers evidence deterministically, early-returns non-matches without a model call, and sends only ambiguous candidates with compact context to a task-specific classification head plus an in-house reasoning model; isolated per-customer sandbox without credentials or egress; context fetched once and cached near workers; 100 ms median per trace; more than 20 billion traces a month; daily frontier-model audits of a sample with retune on regression; customers can read and edit the code and prompt. `[public: blog "Signals 2.0 — frontier-grade classification"]`
- Your published benchmark: precision 71 % and recall 87 % at 1× relative per-trace cost, against 78 % / 91 % at 260× for the best frontier judge. `[public: same post]`
- Phrasing sensitivity you published: four one-sentence phrasings of one behaviour over 2,000 traces matched 0.9–4.6 %, and 67 % of traces matched by one phrasing were rejected by another. `[public: same post]`
- Issue Detection v2 (2026-06-03): stumbles (one failed run, tagged, scan cadence) → issues (recurrence across runs and users, severity, overrepresented tools/models/signals, timeline) → signals. The two catalogs are disjoint. `[public: blog "Introducing Raindrop 2", MCP tool reference]`
- Experiments compare cohorts of already-logged events and never run the agent; "likely better/worse" needs a 95 % CI excluding zero and a sample check. `[public: docs/platform/experiments]`
- Change Reports scope traffic by `raindrop.app.commit_sha` and report ~24 h after a merge; Simulations (early access) replay production traffic against a harness change on every PR. `[public: docs/platform/change-reports; Series A post]`
- Storage: Tinybird and ClickHouse appear in your subprocessor list, RQL is a ClickHouse dialect, and the Rust SDK comments mention Tinybird's strict typing. `[public]` That events and traces are served from ClickHouse-family storage behind Tinybird is `[inferred]`.
- Our own integration: real events and feedback signals from our prototype arrived and read back in the UI on 2026-10-12 (project `rrflow-hydradb`). `[observed]`

## 2. Where we may have you wrong — please correct

1. `signals[].score` on an event: we read it as classifier probability; range and threshold are not public. `[inferred]`
2. The `event` object API inside generated signal code (`event.trace`, `findToolCalls`, `haveIdenticalInputs`, `formatForReview`, `classify(..., {spanIds})`): we have one blog example only. `[public, one example]`
3. How stumbles cluster into issues and what "Similar" uses. `[not public]`
4. How `topics[]` are generated and whether the taxonomy is per project and editable. `[not public]`
5. Whether the Signals 2.0 API / ZDR Signals / Signal Builder exposes a callable classifier outside the ingest path. `[public mention only; docs page 404]`
6. `track_partial` server-side merge semantics for conflicting fields. `[not public]`
7. Whether Rust/Go SDKs will carry `feature_flags` and `ai_data.tool_calls` on the wire (TypeScript and HTTP do today). `[public: absent from both repos]`

## 3. What RRFlow is, stated plainly

RRFlow is a local-first Rust engine (`github.com/rrflow/rrflow-development`, public, MIT OR Apache-2.0) that keeps one project's memory, graph, vectors and reasoning evidence in one bi-temporal store (rrflowDB on rrflowKV), with a System One controller (SWAG) deciding which model endpoint handles a prompt, and a learning configuration (RROFLOW) that is specified but unimplemented. Status today, in our own repo's words: storage gates A–C are the only checked gates; prompt flights and task records exist; no production generations are recorded yet; no benchmark against anything exists; our acceptance standard calls the product "0.0" until the UI drives a frontier model through rrflowDB memory end to end. We are not claiming to be faster, more accurate or cheaper than Raindrop; we have no numbers.

**Different, not better.** Raindrop observes agents running anywhere, at scale, as a hosted service. RRFlow is the engine an agent's memory and reasoning run *inside*, on the developer's own machine or in a shippin.cloud devspace, on the developer's own model subscriptions. The learning loop we want sits on evidence the engine already owns (every flight is a graph with edges to the records it used, at a read stamp). What we borrow from you is the mechanism discipline; what we add is that the classifier's evidence step is a declarative query over a temporal graph, replayable at any historical stamp.

## 4. What we intend to borrow, with credit

| Raindrop mechanism | RRFlow record | What changes in our design |
|---|---|---|
| Compile once, run deterministically, model only on ambiguous cases | behaviour signal = rrflowQL evidence program + LFG tier-2 judge escalating through SWAG | the evidence program is a query at a read stamp, not generated JS |
| Label a sample, not a checklist; ≤25 seeds, ≤500 refinement examples | Governance-family verdict records with those bounds as configuration | labels never inferred from aggregates |
| Daily frontier audit, retune as a revision | scheduled routine proposing a new definition revision | never auto-promotes |
| Stumbles ≠ issues; scan cadence with "as of" | two disjoint finding kinds; routine checkpoint stamp | cluster = hypothesis until a person acts |
| Ignore reasons as data | finding status with reason feeding refinement | |
| Cohort experiments, CI-excluding-zero verdict | cohort experiment + verdict record naming its method | method is declared configuration |
| Change Reports by commit | flight carries project-snapshot digest/commit; before/after routine | |
| Replay-server contract (`/health`, `/replay`, stitched ids) | outward adapter for agents running outside the engine | |
| Datasets as labelled snapshots | labelled evidence set pinned to a read stamp with consent and split manifest | |
| Per-project model isolation | per-estate definitions and judges | |

## 5. The proposition

Raindrop as a **class-7 endpoint binding** in RRFlow's profile: an `rrflow-raindrop` adapter exports flights as `events/track_partial` plus OTLP spans and reads your labels and findings back through the Query API or MCP. Developers on shippin.cloud bring their own Raindrop subscription the way they bring their own model subscriptions; your classifiers label our flights before ours exist, and when ours exist they are measured against yours on the same flights. We want that comparison to be possible, public and fair.

## 6. Three questions for the meeting

1. Is the `run(event)` helper API something you would publish or stabilise, or is it intentionally managed?
2. For an engine that already stores every turn as a graph, would you rather receive OTLP spans or a richer event with `tool_calls[]` and attachments?
3. Would a documented export path (events, traces, signals, verdicts) for a customer's own project be acceptable to you — it is what makes the two-way comparison honest.

## Sources

Public: https://raindrop.ai/docs (68 pages via `/docs/llms.txt`), https://raindrop.ai/blog/signals-2-frontier-classification/, https://raindrop.ai/blog/introducing-raindrop-2/, https://raindrop.ai/blog/series-a/, https://query.raindrop.ai/v1/openapi.json, https://github.com/raindrop-ai (workshop, raindrop-rust, go, raindrop-swift, skills, pi-agent, claude-code-plugin), https://www.ycombinator.com/companies/raindrop. Observed: our workspace, project `rrflow-hydradb`, 2026-10-12 read-back. Full sourced ledger available on request.
