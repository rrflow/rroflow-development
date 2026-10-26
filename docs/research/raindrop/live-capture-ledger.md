# Raindrop live capture — Jesse's workspace, read through the Query API

Date 2026-10-24. Key type: Query API key (read). Write probe → 401; `GET /v1/projects` → 200. Projects: `default` (Production), `rrflow-hydradb`. Raw JSON in this folder. Nothing was written to the workspace.

## Workspace contents
- **default**: 12 signals, 26 events (2026-10-11 → 10-13). Signal types `instrumented` (7) and `code` (5). The code signals are Attempted Prompt Injection, User Frustration, Churn Intent, Capability Gaps and Repeated Tool Call Failures, each with a one-line description and Negative sentiment. No event in the window matched a code signal. [observed: default-signals.json]
- **rrflow-hydradb**: 2 instrumented signals (Brain ui helpful, Answer helpful), 207 events (10-12 → 10-24). Event names: workspace_sync 96, brain_ui_generated 53, brain_recall 28, github_repository_sync 14, verification_evidence_captured 12. `model` is null on every event, `topics[]` is empty on every event, and `tool_calls` lists `hydradb.query`. [observed]
- Signal groups are per project and seeded as Negative, Other, Positive. [observed]

## Facts answered by observation (closing ledger open questions)
1. `signals[].score` is **absent/null on instrumented signals**. A score exists only where a classifier produced the match. Closes part of open question 3. [observed: events]
2. `GET /v1/signals/{id}` for a `code` signal returns only id, type, name, description, created_at, group and sentiment. **Classifier code is not exposed through the Query API.** [observed: sig-code-prompt-injection.json]
3. Trace rows from `/v1/traces` carry: attributes, duration_ns, start/end_time_ns, input, output, input/output_tokens, model, provider, parent_span_id, span_id, span_name, span_type, status, trace_id. A brain_recall event shows 2 spans: an `LLM_GENERATION` root with no model, and a `TOOL_CALL` child. [observed: trace-brain_recall.json]
4. Events sent without OTLP spans (harness_run, vibeguard chat) return 0 trace rows; their `tool_calls` come from the SDK interaction instead. [observed]
5. **Evals are gated per organization**: `/v1/evals*` and `/v1/eval-datasets/*` return 403 "Evals is not enabled for this organization." `/v1/replays/manifest?dataset=` returns 400 "No dataset named …" (the route exists). [observed]

## The SDK eval surface (read from `raindrop-ai@1.0.2` type definitions and dist; not executed)
- Exports include: `publishEvalDataset`, `readEvalDataset`, `defineDataset`, `defineLocalEvaluator` (output boolean | score | number | preference), `defineEvaluatorProgram` (`executionMode: deterministic | judge`, `intent`, `rules[]`, `source`), `defineEvalSuite` (dataset + evaluators with thresholds + `run` or `agent`, concurrency), `runEvalSuite`, `createEvalSuiteRun`, `readEvalRun`, `evaluateEvalRun` ("regrade captured traces with hosted evaluators; does not call the agent again"), `compareEvalRuns` (returns `sameEvaluatorVersion`), `pullEval` and `loadEvalSnapshot` (pin dataset + evaluator artifacts to JSON, reload offline), `importPortableEvaluator`, `evaluatePortableEvaluator`, `verifyPortableEvalArtifact`, `replay` / `runReplay` / `claimReplay` / `createEvalHandler` / `defineAgent`, `traceTools.getOutput/getToolCalls` ("never execute tools").
- **Portable evaluator artifact**: `{format: "raindrop.eval-program", formatVersion: 1, runtimeAbi: "raindrop-eval-v3", evaluator: {id, slug, programVersion, executionMode, outputType, scope: "trace"}, traceProjectionVersion: 1, traceProjectionSha256, source, sha256}`. The SDK pins its trace projection (`TRACE_PROJECTION_SHA256 = 243e3954…721c`) and refuses an artifact built against another projection.
- **Guest ABI**: the source runs in QuickJS (quickjs-emscripten 0.32.0). It must define global `__raindrop_eval_run(inputJson)` and return `{type:"result", result}`. It can call host `__raindrop_eval_judge(requestJson)` with `{type:"judge_request", requestId, traceId, rubric (≤40,000 chars), text (≤4 MiB), outputType: boolean|score|preference}`. **The judge is a host callback the caller binds**: `PortableJudge(request) → {pass} | {score 1–5} | {value} | {preference A|B|tie}`.
- Sandbox defaults: memory 32 MiB, stack 64 KiB, program timeout 1 s, judge timeout 60 s, judge request timeout 30 s, judge concurrency 4, max input and output 4 MiB each.
- Eval run results carry `tracesExpireAt` / `tracesExpired`, so captured traces for eval runs expire.
- Endpoints called by the SDK: `/v1/evals/{slug}`, `/v1/evals/{slug}/runs`, `/v1/evals/{slug}/local`, `/v1/evals/{slug}/runs/local`, `/v1/eval-datasets/{slug}[/versions/{id}]`, `/v1/eval-runs/{id}?include=traces`, `/v1/eval-runs/{a}/compare/{b}`, `/v1/replays[/{id}][/claim|/report|/trace]`, `/v1/replays/manifest?dataset=`, `/v1/traces`.

## vibeguard: Jesse's own recursive-improvement experiment on Raindrop (`EonsofStupid/vibeguard`, harness in `web/src/server/harness/`)
- Loop: run 16 frozen tasks (10 train / 6 held-out) → a guard blocks candidate code that matches approved rules → a deterministic judge grades the result → failures on **train only** propose a rule → a human approves or reverts (gate: agents may only tighten; only humans approve, loosen or revert; every transition writes the rule plus an audit receipt). Raindrop receives `harness.run` interactions, `rule_proposed` events, feedback signals `rule_approved` / `rule_reverted`, an edit signal `rule_narrowed`, self-diagnostics per training failure, and agent self-grades `heldout_improved / flat / regressed`. [source + observed]
- Observed history:

| Runs | Rules active | Held-out | Train |
|---|---|---|---|
| run_001–003 | none | 1/6 (CI 0.03–0.56) | 1/10 |
| run_004–006 | VG-0001, VG-0002, VG-0003 | 6/6 (CI 0.61–1) | 9/10 |
| run_007 | VG-0002, VG-0003 (VG-0001 reverted by a human) | 4/6 (CI 0.30–0.90), signal heldout_regressed | 6/10 |
| run_008–018 | VG-0002, VG-0003, VG-0004 (narrowed from VG-0001, `warn`) | 4/6 for 11 straight runs | 7/10 |

- **Why it plateaued (from `loop.ts`)**: proposals are made only for categories with no non-reverted rule. VG-0004 covers `cost.unbounded_api_calls` with a literal pattern of the one failing snippet and action `warn`, so the category counts as covered, the proposer goes silent, and held-out never recovers. Two reverts in a category silence it permanently.
- Other defects visible in the trace:
  - The same 6 held-out tasks were evaluated across 18 runs, so the holdout is contaminated.
  - n = 6 gives a 95 % interval of 0.30–0.90, too wide to tell 4/6 from 6/6.
  - "heldout_flat" is filed in the Positive group, so a stalled loop reads as good news.
  - The eval-replay path (`raindrop-evals.ts`) could never have run, because Evals is disabled for the org.

## Lessons for RROFLOW (each added to the spec)
1. Coverage is not effectiveness. A category with an active rule must reopen for proposals when held-out stays below its best observed level for N runs. Effectiveness is measured, not assumed from coverage.
2. A human revert is a negative verdict on that rule's revision, not a permanent block on the category. Silencing needs its own explicit, revocable decision.
3. A held-out set used to accept a revision is spent. Rotate or version it (the learning contract already requires this; vibeguard shows the cost of skipping it).
4. Minimum sample per decision: state the interval and refuse "improved/regressed" verdicts the interval cannot support.
5. Sentiment of a self-grade must follow its meaning: flat is neutral, not positive.
6. Raindrop's portable evaluator (program + bound judge + pinned trace projection + content hash + sandbox limits) is the closest existing artifact to the spec's R3. RRFlow's equivalent is a behaviour-signal revision whose evidence program is rrflowQL, whose judge is an endpoint binding, and whose identity is a digest.
