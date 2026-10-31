# A signal dashboard that leads to evidence

Status: implementation in development, 2026-10-28. Hosted activation and complete
Raindrop/Sentry/PostHog parity are not established by this increment.

Open a signal from the library or choose a signal in Findings. The dashboard
shows matching turns, received turns in the same selection, and the share with
a recorded match. Agent, project, session, text and time filters apply to the
counts and evidence. A turn counts once even when it has repeated hits for the
same signal. Match share is neither classifier accuracy nor evaluation coverage:
the service cannot infer that a missing hit means a rule checked the turn.

The timeline shows up to the latest 30 UTC dates with received activity. Missing
calendar days do not establish inactivity. Earlier-date matches and undated
matches are counted separately and remain in the overall total. Selecting a day
sets inclusive start and exclusive end boundaries, clipped to the current range.
Undated records cannot satisfy a dated filter. Agent and project lists show the
top 12 groups, with omitted match counts. These limits bound presentation, not
the population used for totals. The paginated evidence list is a separate read;
ongoing ingestion can change records between the aggregate and evidence requests.

Choosing an agent, project or day preserves the other filters and resets pagination.
The URL holds the selection, so browser history and copied links keep context.
Advanced raw data remains in the turn inspector. A failed aggregate request shows
an unavailable state while retaining the evidence list; it never substitutes
first-page counts. Logout and navigation discard late responses and private totals.

The library includes captured signals without a current catalogue/definition under
Other recorded signals. Their evidence remains accessible, but they have no pause
or edit control because the service does not know their policy. Built-in and custom
signals keep their existing settings and immutable-definition behavior.

## API and storage

`GET /v1/signals/stats?signal=...` is authenticated. Optional filters are `agent`,
`project`, `session`, `search`, `since` and `until`. Unknown parameters, malformed
dates and reversed intervals are rejected. It returns `rroflow.signal-stats/v1`,
the owner, normalized scope, observation time, bounded breakdowns and complete
cohort counts. No prompt, response or tool payload is returned. `GET /v1/turns`
also accepts the exclusive `until` boundary for exact drill-down selection.

JSONL reads the owner's current in-memory index under its existing lock. Postgres
uses one statement snapshot with tenant predicates, signal existence checks and
grouping sets; ranking and truncation happen in SQL. A five-second statement
timeout bounds database work. No new schema or arbitrary query endpoint is added.
The API uses the current upserted turn revision, so replacement can change counts.
Saved review snapshots remain immutable in the separate review store.

Acceptance includes more-than-one-page histories, duplicate hits, missing dates,
bounded breakdown reconciliation, literal search, date boundaries, two owners,
replacement/restart, unavailable aggregates and late-response cleanup. Desktop
and phone tests follow signal → project → day → exact turn using the real Rust
fixture server. These tests are not a production load benchmark.

The [capability baseline](../plan/capability-map.md) tracks the remaining monitoring,
enterprise dataset and learning requirements. This screen does not publish a
dataset, measure held-out performance or activate a learned change.
