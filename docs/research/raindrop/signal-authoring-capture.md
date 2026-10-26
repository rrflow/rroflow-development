# Captured 2026-10-25: Raindrop custom signal authoring, end to end (project claude-code)

Flow: New Signal (one description box) → Triage agent asks the project → draft card with 3 plain-language rules → "1 matched events ready to review" → review panel (Why Triage thinks this is a match + Confidence 62% + NOT MATCH / MATCH) → after labelling, a **Run Again** button appears (labels feed a refinement pass) → Create signal opens a dialog: Project, Name, Description (rewritten by Triage), Sentiment NEGATIVE, **Preview classifier code**.

Triage's match reason (62%): usage-credit barrier handed back to the user. Labelled MATCH.

Dialog description: "Flags turns where the assistant encounters a tool, access, or permission blocker and hands a required manual step back to the user instead of completing it itself."

## Generated classifier (verbatim from Preview classifier code)
```js
const CLASSIFY_POLICIES = {
  positive: [
    "A tool denial, permission restriction, or authentication barrier blocks the assistant's progress.",
    "The assistant hands a required manual step to the user because of that blocker.",
    "Manual steps include signing in, creating a repository, supplying a key, or approving access.",
    "Saying it is waiting for the user or needs one user action counts as a handoff."
  ],
  negative: []
};

export async function run(event) {
  const tools = (event.trace ?? []).filter(e => e.type === "tool_call");
  const output = event.data.aiData.output ?? "";
  const hasToolContent = tools.some(e => (e.input.raw ?? "").trim().length > 0 || (e.output?.raw ?? "").trim().length > 0 || (e.error?.raw ?? "").trim().length > 0);
  if (!output.trim() && !hasToolContent) return { matched: false };
  return classify(
    buildClassifierContext(event, [], "tool-resolution"),
    "The assistant encounters a tool, permission, or authentication blocker and hands a required manual step back to the user instead of completing that step itself.",
    { spanIds: tools.filter(e => e.status === "error" || (e.error?.raw ?? "").length > 0).flatMap(e => e.source.spanIds) }
  );
}
```

## What this reveals (observed, not inferred beyond the code)
1. Classifier = deterministic gate + model judge. The gate returns `{matched:false}` with no model call when there is no output and no tool content (cost control).
2. Event shape the classifier sees: `event.data.aiData.output`; `event.trace[]` entries with `type:"tool_call"`, `input.raw`, `output.raw`, `error.raw`, `status`, `source.spanIds`.
3. Runtime helpers: `classify(context, question, {spanIds})` and `buildClassifierContext(event, [], "tool-resolution")` — a named context projection ("tool-resolution").
4. Evidence is attached as span ids (error spans) — the match points at the exact spans.
5. Policies are plain-language positive/negative lists. The 4 positive rules differ from the 3 shown on the draft card: rule 4 ("waiting for the user… counts") was added after the MATCH label → labels rewrite policy text, not weights.
6. Consequence for our ingest: without `--payloads`, `input.raw/output.raw` are empty, so tool-content-only turns are skipped by the gate. Payloads matter for signal quality, not just the onboarding check.

## RROFLOW mapping
- R3 behaviour-signal revision = {policies[], evidence program (rrflowQL), judge binding, context projection id, digest}. Raindrop's shape confirms the split; ours makes the projection and judge explicit, versioned inputs.
- Label → policy rewrite is R5/R6 territory: a label is evidence that produces a candidate revision, which must pass held-out before promotion (Raindrop applies it directly).

## After Create signal (observed 2026-10-25)
- Redirects to `/signals/<id>?type=code`; the signal id equals the refinement artifact id (`signal-refinement:<id>`).
- Backfill over the last 7 days: banner "Events are being classified… 0% complete", finished within ~10 min.
- Signal page: Overview / Configure, Filter, Display, Add to Dataset, Ask Raindrop, Add alert, Archive, View signal code; side panel auto-correlates **Failing tools** (Bash 9 failed · 8 turns; device_request_folder_access 2 · 4; add_repo 2 · 2; get_page_text 2 · 2) and **tag distribution** (model, cwd with %).
- Query API: `GET /v1/events?signal=<id>` filters to matched events (`signal_id=` is ignored and returns all). Project selected by `X-Raindrop-Project-Id` header.

## Precision, scored by hand against this session's ground truth
14 matched of 22 events = 8 distinct turns (6 are duplicate pairs from the earlier id-collision bug).
| Turn | Verdict | Why |
|---|---|---|
| "We need to maximize…" | TP | browser sign-in and instrumentation handed to user |
| "you need to truly be using the tools…" | TP | folder access + Workshop runs denied |
| "you have full approval…" | TP | Infisical login unreachable from VM |
| "done" (01:05) | TP | repo creation blocked → asked user |
| "so lets setup rroflow.ai…" | TP | "Waiting on you to create the repo" |
| verifier subagent | borderline | usage-credit API error, not an agent handoff |
| task-notification | borderline | same API error relayed |
| "get the apikey" | FP by rule 1 | refusal by choice; no tool or permission blocker |
Not matched: post-compaction turn (agent finished itself), harvest subagents, "done" (01:09), local-command — all correct negatives.
Precision 5/8 strict, 7/8 lenient; no observed misses (n is tiny — this is a smoke test, not an eval).

Lesson for RROFLOW: the one human label (on a borderline case) widened the policy ("waiting for the user… counts") with no held-out check. In RROFLOW that label would produce a candidate revision evaluated on a labelled set before promotion (R5/R6), and the harness-error vs agent-handoff distinction would be a negative policy line.
