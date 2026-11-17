# Signals v1

Status: implemented. Moved out of `lib.rs` and `analytics.rs` in refactor R1 (self-host release 1,
TASK-004) with no change in behavior. The versioned signal contract comes in M3 of the self-host
release plan.

A signal is a named finding about a turn, such as a blocked handoff. The built-in signals are
evaluated by the collector. A project's custom rules are evaluated on the server when turns arrive
(`turns-v1.md`). Code is in `crates/rroflow-ingest/src/signals.rs` and `signals/`; the member
console's screen is described in `member-signal-library-v1.md`.

| File | Holds |
|---|---|
| `signals/model.rs` | The catalogue document, `rroflow.signals/v1` |
| `signals/routes.rs` | The catalogue, settings and preview handlers |
| `signals/stats.rs` | Signal statistics |

## Routes

| Method | Path | Permission | Does |
|---|---|---|---|
| GET | `/v1/signals` | Public | The catalogue: categories with their signals, and every signal's definition |
| GET | `/v1/signals/settings` | `project.read` or `project.send` | The project's choices. A send-only key does not receive custom rules |
| PUT | `/v1/signals/settings` | `project.write` | Save the project's choices |
| POST | `/v1/signals/preview` | `project.read` | Try a custom rule against recent turns without saving it |
| GET | `/v1/signals/stats` | `project.read` | Counts for one signal over the project's turns |

## Settings

Settings carry a `revision`. A save must send the revision it read; if another tab saved first, the
answer is `409` and nothing changes. A saved revision goes up by one.

## Preview

The rule is checked, then evaluated against the 100 most recent turns that match its agent and
project. The answer reports how many turns were scanned and evaluated, how many lacked the data the
rule needs, whether older history exists, and each match.

## Statistics

`signal` is required, at most 128 characters. `agent`, `project`, `session`, `search`, `since` and
`until` filter as they do for turns. The answer counts received and matched turns, with matches by
day (30 at most), by agent and by project (12 each at most), and the remainder of each. No captured
content is returned.
