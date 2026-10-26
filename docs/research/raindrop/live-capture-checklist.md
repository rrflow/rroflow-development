# Raindrop live-capture checklist (logged-in workspace)

Purpose: the public material in `raindrop-source-ledger.md` covers wire formats, MCP tool surfaces, and the published architecture of rd-signal-2. What follows is NOT visible publicly and has to be captured from inside a logged-in app.raindrop.ai workspace (Pro trial is 14 days; Hobby exposes Signals but locks Issues/Stumbles/Experiments). Priority: P0 = blocks the learning-layer spec; P1 = shapes the spec materially; P2 = nice to have. Each item says why it matters for a layer that mints classifiers from traces.

Capture method per item: screenshot the UI state, copy any JSON/code shown, and where an MCP/Query API call exists, save the raw response (`get_signal`, `get_event`, `get_trace`, `list_signals`, `get_signal_creation_status`, `GET /v1/signals/{id}`). Record the project slug and timestamp with every capture.

## P0 — Signal definition & classifier minting

- [ ] **Signal creation card, every step** (New Signal → agent drafting → review → rules → create). Capture the full text of the numbered policy rules the agent writes, the chat transcript, and the step list the card tracks. *Why:* this is the human-readable "definition" artifact a learning layer must produce before compiling code; its structure (how many rules, scope statements, turn-vs-conversation framing) is the target schema for our definition object.
- [ ] **"Preview classifier code" contents for ≥3 signals** (one tool-pattern signal, one pure-language signal, one preset turned on). Copy the JS verbatim. *Why:* the only public example (`run(event)` → `findToolCalls` → `classify(...)`) is from a blog post; we need the real helper API (`event.trace`, `event.finalResponse`, `findToolCalls`, `haveIdenticalInputs`, `formatForReview`, `classify` signature incl. `spanIds`) to design an equivalent evidence-gathering DSL.
- [ ] **Review panel mechanics**: how many matched events are surfaced per scan, the "why it matched" explanation text, whether labels are per-event or per-span, and what "Run Again" changes in the rules. Count how many Match/Not-Match labels you give before "Create signal" enables. *Why:* fixes the few-shot budget and the label granularity our active-learning loop should mirror.
- [ ] **`get_signal_creation_status` responses over time** for one creation (`phase`, `scan.scanned_total`, `scan.matched_total`, `progress.*`). Save each poll. *Why:* reveals batch sizes, sample size scanned per revision, and the preparing→scanning→reviewing→installing state machine we would reproduce.
- [ ] **Signal detail page for a live `code` signal**: definition, description, sentiment, group, occurrence count, user count, trend chart buckets, events table columns, and the "Refine Signal" dialog (what is pre-seeded: current definition + all prior verdicts). *Why:* the refinement loop is where human labels accumulate; we need to know what state is carried between revisions.
- [ ] **Signal object from Query API `GET /v1/signals/{id}` and MCP `get_signal`** for one signal of each `type` (`topic`, `regex`, `instrumented`, `metric`, `code`). *Why:* confirms field set per type and whether classifier code/prompt/version is exposed via API (public schema shows only id/type/name/description/created_at/group/sentiment).
- [ ] **`signals[].score` on `GET /v1/events/{id}`** for several events matched by one signal; note range (0–1? logits?), whether deterministic-branch matches have a fixed score, and whether non-matches ever appear. *Why:* determines whether we can threshold/rank matches or only treat them as boolean labels.
- [ ] **Backfill behaviour**: after creating a signal, record how many of the last 90 days got labelled and the sampling ratio visible in the trend (events labelled vs total). *Why:* sets expectations for cold-start rate estimation in our layer.
- [ ] **Edit-code / edit-prompt affordance** mentioned in the Signals 2.0 post ("customers can inspect and edit the code and prompt"). Find it in the UI; capture what is editable and whether edits trigger re-scan. *Why:* tells us whether the classifier is a user-owned artifact or a managed one.

## P0 — Issue grouping & intent taxonomy

- [ ] **Issue detail page** (Pro): severity, status, affected event/user counts, affected dimensions (overrepresented tools/models/signals), timeline, report text, related events, and the Slack prompt block. Save `get_issue`, `get_issue_events`, `get_issue_timeseries` JSON. *Why:* the issue report is the "distribution-shift" grouping output; its dimensions tell us which features (tool, model, signal, flag) the grouper keys on.
- [ ] **Stumble detail + Similar panel**: tag vocabulary (collect every tag seen: Logic Error, Hallucination, …), timeline-of-key-moments format, and `search_stumbles` raw JSON incl. `cadence_minutes`/`last_run_at`. *Why:* the tag set is the implicit failure taxonomy; the cadence tells us the detection latency budget to match.
- [ ] **Topics**: find where `topics[]` surface in the UI (event sidebar? filters?), list the full topic vocabulary for the project, and check whether topics are editable/mergeable. *Why:* this is the closest thing to an intent taxonomy; we must know whether it is open-vocabulary or a fixed per-project set.
- [ ] **Conversation summaries** (Pro): format, length, and whether they are queryable. *Why:* a summary is a candidate compact context for cheap classification.
- [ ] **"Turn issue into a Signal"** path (blog claims any detected issue can become a Signal). Capture the pre-filled intent and seed examples. *Why:* the issue→signal handoff is exactly "mint a classifier from a cluster of traces".

## P1 — Experiments, replay, verification

- [ ] **Experiment builder**: cohort condition types (model, flag, property, tool, date), outcome signal picker, Events %/Users % toggle, verdict, 95 % CI / p-value tooltip text, Event Stats rows (cost per event median, tools, duration, errors). Save `get_experiment` JSON. *Why:* defines the minimum statistical verification our layer must offer to prove a minted classifier's rate moved after a fix.
- [ ] **Change Report** sample (connect a GitHub repo to a test project): early-check and full-report text, which agent-harness changes were detected. *Why:* shows how they attribute behaviour change to code change via `raindrop.app.commit_sha`.
- [ ] **Workshop replay UI**: the replay form fields (user message, model, system prompt, context prefill), the stitched original-vs-replay view, and `/api/replay` SSE events. *Why:* replay is how a minted classifier gets validated against a candidate fix locally.
- [ ] **Dataset workspace**: label set, custom-row upload format (CSV columns), CSV export columns, "find more like these" via Triage. *Why:* datasets are the labelled-example store; export columns tell us what a training row contains.

## P1 — Settings & plan limits

- [ ] **Settings → API Keys**: key types (write key, personal query key, org-wide key), scopes shown, "Limit to owners and admins" toggle. *Why:* defines the auth model we need to interoperate with (ingest vs query vs MCP).
- [ ] **Settings → Billing**: event counters, signal-slot usage, Triage message usage, overage display. *Why:* confirms metering units (one event = one turn incl. tool calls and sub-agents).
- [ ] **Settings → PII Playground**: redaction categories and toggles. *Why:* redaction categories constrain what text a classifier can see.
- [ ] **Settings → Filters → User Exclusions** and internal-traffic flag (`is_internal`, `is_excluded_user` columns). *Why:* our layer needs the same exclusion semantics so rates match.
- [ ] **Settings → Notifications**: Daily Digest, Signal Alerts thresholds (users/events/every match), Change Reports max-per-day. *Why:* alert thresholds are the operational contract for a signal.
- [ ] **Settings → Triage Agent → Use all projects**, and project switcher behaviour for All projects. *Why:* scoping rules for cross-project learning.
- [ ] **Projects page**: "Projects with data, not yet created" list and per-org slug limit (number). *Why:* we rely on ingest-first project creation; the limit is undocumented.

## P1 — Event & trace views

- [ ] **Event sidebar**: every field shown (input/output, attachments tabs, matched signals with scores, properties, feature flags, user traits, system prompt snapshot, tools map, cost). *Why:* the per-event feature vector available to classifiers.
- [ ] **Trace view**: span tree, span types present (incl. synthetic SYSTEM_PROMPT), payload truncation behaviour, sub-agent nesting. Save `get_trace` with and without `include_context`. *Why:* defines the evidence a code classifier can walk.
- [ ] **Events filter chips**: full list of filterable fields (user, conversation, date, property, model, tool, signal, flag, trait). *Why:* candidate cohort keys.

## P2 — Export, retention, misc

- [ ] **Data export** (Pro feature named in plans): locate it, capture format (CSV/JSON/Parquet?) and scope (events? traces? signals?). *Why:* bulk export is how we would bootstrap a learning layer from existing Raindrop data.
- [ ] **Users CSV export columns**. *Why:* user-level features (plan, traits, signals).
- [ ] **Retention enforcement**: on Hobby, confirm events older than 14 days disappear from Events/Query API. *Why:* training-window constraint.
- [ ] **Memory UI** (Pro): what memories look like, how Triage cites them. *Why:* customer-specific context injected into classification/triage.
- [ ] **Tool Errors tab** on Signals: how auto-derived tool-error signals are named and grouped. *Why:* a free, label-free signal family worth mirroring.
- [ ] **Self Diagnostics tab**: how `agent` signals render, category breakdown. *Why:* agent self-reports are high-precision seeds for classifiers.
- [ ] **Dashboards**: built-in Usage board panels and a saved dashboard's `get_dashboard` JSON (panel kinds, visualizations, filter kinds). *Why:* reveals the RQL queries Raindrop itself uses for rates.
- [ ] **Signals 2.0 API / ZDR Signals**: ask support or sales for docs; capture any endpoint for calling a trained classifier on arbitrary input. *Why:* would show whether classifiers are callable outside the ingest path.
