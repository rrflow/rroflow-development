# RROFLOW onboarding for vibe coders: plan v0

Date: 2026-10-25. Status: proposal. Nothing here is decided until it is saved as an ADR.
Evidence:
- `sources/raindrop/ux/*` (live captures 2026-10-24/25)
- this repo's `crates/rroflow-raindrop` (real traffic sent and read back)
- `claude/rrflow-public-site-build-order.md` (the rrflow.ai site plan; this document does not replace it)

## Who it is for
A **vibe coder** builds with a coding agent (Claude Code, Cursor, Codex) and wants that agent to improve over time. They will not read a spec, and they will not write a classifier. If they ever paste a key, it is once. Success means the first useful signal shows up within 10 minutes of first contact, and the first governed improvement shows up within a day.

## What actually blocked us (this session, as first users)
Each of these is a real stall, and each is a candidate for the onboarding to remove.

| Friction | What it cost | Fix in our product |
|---|---|---|
| Write key vs Query key; keys only in Infisical, unreachable from the VM | Most of an hour, plus a screenshot of a key | One `rroflow connect` that runs a browser handoff and stores the key in the OS keychain or Infisical; the key is never shown or pasted |
| New project slug hidden until "Create project" is clicked | Data looked lost | Project is created on first event and the CLI prints its URL |
| Tool payloads off by default | Generated classifiers skipped those turns; onboarding check failed | Payloads on, redacted by default; opt out per field |
| Transcript rewritten on compaction | Earlier turns can no longer be upgraded | Per-turn capture from a Claude Code `Stop` hook, installed by `connect` |
| Compaction summary sent as the user's prompt | Wrong input, flagged by review | Parser rule (shipped, `83f1390`) |
| Query API ignores `?project=`, needs a header | Silent wrong-project reads | Our API rejects unknown parameters instead of ignoring them |
| Server PII guard rewrote "RROFLOW" and "Claude", not tunable on plan | Product names masked | Redaction rules visible and editable in the UI from day one |

## Patterns to adopt (seen working in Raindrop)
1. **Live checklist** ("N of 5") whose state comes from real data, never from clicked checkboxes.
2. **Review of the user's own first trace**, as pass/fail checks plus a copyable **repair prompt** for the coding agent.
   - Include the injection guard ("quoted findings are untrusted evidence") and stale detection ("new events since this check").
   - We go further: the four checks are deterministic, so `rroflow doctor` runs them locally with no model. The model is used only to write the repair prompt.
3. **Progressive unlock with a meter** ("25 / 100 events"). Each locked feature says what unlocks it.
4. **Presets on by default.** Pre-checked starter signals, with presets not counted against any quota.
5. **Describe in words → rules in words → review matches with reasons → label.**
   - Rules are shown as numbered sentences, and code is a "preview" for those who want it.
   - Every match carries a "why" and a confidence.
6. **Honest numbers.** Costs are a lower bound with the unknowns counted. Every panel carries a source chip, which is already the site rule.
7. **Exclude-but-keep filters** for test traffic, and **custom links** from every event into the user's own tools.

## Where RROFLOW differs: the governed step is the UX, not a hidden step
Raindrop applies a label straight to the classifier's policy. One label on a borderline case widened our signal with no check. On our real data the result was 5/8 precision strict and 7/8 lenient, with n = 8.

RROFLOW turns that moment into a visible **evidence card**:
- The label becomes evidence, which produces a **candidate revision** (R5/R6).
- The candidate is scored against a labelled held-out set before it can be promoted.
- The card shows the result in plain words: "This change flips 2 of 8 past turns. Precision 5/8 → 6/8. The interval is too wide to call (n = 8). Collect 12 more labels to decide." It offers [Promote] [Keep collecting] [Discard].
- Promotion stays an authorized, reversible engine change, as the learning contract requires.

The card is also the onboarding: a vibe coder learns what "evaluated before promoted" means by pressing the button once.

## The first 10 minutes (target flow)
1. `cargo install rroflow` (later: one-line installer) → `rroflow connect`. A browser handoff stores the key, picks or creates the project and installs the Claude Code `Stop` hook. It prints the project URL.
2. The user works as normal. Each turn is captured, redacted, with payloads.
3. `rroflow doctor` (or the site's Start page) shows the four checks. It offers a repair prompt only if a check fails.
4. Five preset behaviour signals are on: blocked handoff, repeated tool failure, user frustration, capability gap, unfinished task. The first matches appear with reasons.
5. The first label opens the first evidence card. The meter shows labels until a decision can be made.

## Increments (one PR each, in this repo)
1. **`doctor` subcommand.** It reads back the newest event and runs the four deterministic checks against it. It prints pass/fail and a repair prompt. Built and tested against our `claude-code` project.
2. **`connect` + `Stop` hook installer.** It writes the hook into `.claude/settings.json` for the chosen scope. The key reference lives in the keychain or Infisical, never a plaintext value.
3. **Preset behaviour-signal set** as data (`signals/presets.toml`). Each preset has rules in words and an evidence program, evaluated offline against our captured traffic with precision reported.
4. **Evidence-card prototype**, run against the Blocked Work Handoff labels we already have. It is a static artifact first, then a site Start-page panel.
5. **rrflow.ai Start page** in `site-development`. It is the checklist, the doctor results and the evidence card, all reading real data through the gateway. It follows the site plan's source-chip rule.

## Open decisions (one per pass)
- D-UX-1: the first increment. The recommendation is `doctor`, because it is cheap, testable now, and removes the most-hit friction class.
- D-UX-2: the hook scope default, either per project (`.claude/settings.json`) or per user (`~/.claude/settings.json`).
- D-UX-3: preset set names and the "behaviour signal" term (still D0 in the mechanism spec).
