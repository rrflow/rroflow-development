# Turns v1

Status: implemented. Moved out of `lib.rs` in refactor R1 (self-host release 1, TASK-004) with no
change in behavior.

A turn is one `rroflow.turn/v1` record: one exchange between a person and an AI, as a collector saw
it. This feature receives turns and reads them back. What the usage fields mean is in
`turn-usage-v1.md`. Code is in `crates/rroflow-ingest/src/turns.rs` and `turns/`.

## Routes

| Method | Path | Permission | Does |
|---|---|---|---|
| POST | `/v1/turns` | `project.send` | Store a batch of turns |
| GET | `/v1/turns` | `project.read` | Read turns, newest first and undated last, one page at a time |
| GET | `/v1/summary` | `project.read` | Totals, the signal catalogue and the project's signal settings |

## Sending turns

The body is `{"turns": [...]}`, with no other fields. A batch holds at most `max_batch` records (500
by default). Each record must:

- name the schema `rroflow.turn/v1`;
- have a non-empty `event_id`;
- give `started_at` and `ended_at` as RFC 3339 timestamps, or leave them empty.

The first record that fails is named in the answer, and nothing in the batch is stored.

Signals named `custom_*` belong to the receiving project, so any the client sent are dropped. The
project's enabled custom rules are then evaluated on the server. Records upsert by `event_id`: sending
one again replaces it. The answer counts `accepted`, `new` and `replaced`.

## Reading turns

| Query | Meaning |
|---|---|
| `limit` | Page size, 50 by default, at most 1000 |
| `agent`, `signal`, `project`, `session`, `event` | Filters. Empty values are ignored |
| `search` | Literal text search, at most 200 characters |
| `since`, `until` | RFC 3339 bounds. `since` must be before `until` |
| `cursor` | The `next_cursor` from the previous page |

Unknown query fields are refused rather than ignored. A page includes `next_cursor` when more
records follow, and `revisions`, the review digest of each turn on the page.
