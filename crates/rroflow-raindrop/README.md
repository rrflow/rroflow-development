# rroflow-raindrop

Sends real agent runs to Raindrop, so its classifiers and RROFLOW's work on the same traffic.

`ingest-claude` reads Claude Code session transcripts (`~/.claude/projects/**/*.jsonl`) and sends:

- one Raindrop event per user turn (`claude_code.turn`), with input, output, model and conversation id = session id;
- one LLM span per turn, with token counts;
- one tool span per tool call, with name, duration and status. A failed call is an ERROR span and shows up as the event's tool error.

Ids are derived from the transcript (session, agent and prompt ids), so running it twice does not duplicate events.

## Deploy in any project (shippinAI, rrflow, rroflow, clyffy)

| Variable | Meaning | Default |
|---|---|---|
| `RAINDROP_WRITE_KEY` | Raindrop write key. Inject it from Infisical or Vaultix; never commit it. | required |
| `RAINDROP_PROJECT` | Raindrop project slug for this workspace, e.g. `shippinai`, `rrflow-hydradb`, `claude-code`. | `default` |
| `RAINDROP_USER_ID` | Who ran the session | `operator` |
| `RROFLOW_WORKSPACE` | Tag stored as `properties.workspace` | none |
| `RROFLOW_REDACT` | Extra literal secrets to redact, comma-separated | none |

```bash
cargo install --git https://github.com/rrflow/rroflow-development rroflow-raindrop
infisical run --env dev -- rroflow-raindrop ingest-claude ~/.claude/projects --project shippinai --workspace shippinai
```

Flags:
- `--since 2026-10-24T00:00:00Z` sends only new turns.
- `--payloads` adds redacted, truncated tool inputs and outputs. It is off by default.
- `--dry-run --dump out.jsonl` writes the exact payloads to a file and sends nothing. Use it to inspect them first.

A new slug's data appears under Raindrop → Settings → Projects → "Projects with data, not yet created" until you click **Create project**.

## Check what Raindrop actually stored: `doctor`

```bash
infisical run --env dev -- rroflow-raindrop doctor --project claude-code   # newest event
rroflow-raindrop doctor --project claude-code --event <id> --json           # one event, machine-readable
```

`doctor` reads one event and its trace back through the Query API (`RAINDROP_QUERY_KEY`) and runs seven checks. They are deterministic and make no model call:

| Check | Passes when |
|---|---|
| No secrets sent (`secrets.absent`) | Nothing in the stored input, output, error payloads or span payloads matches a redaction pattern or a configured key. Findings name the field, never the value. |
| Clean user message (`input.clean`) | The input is the person's prompt, not harness text, a compaction summary or a serialized transcript. |
| Assistant response (`output.present`) | The response text is present. |
| Tools in the trace (`tools.traced`) | Every tool call on the event has a named span, and none is listed twice. A gap within `--ingest-lag-secs` of receipt counts as *wait*, not failure. |
| Tool inputs and outputs (`tools.payloads`) | At least `--payload-pass-ratio` of finished tool calls carry input plus output, or an error. |
| Model and tokens (`model.tokens`) | Model name and token counts are present. |
| Fresh data (`data.fresh`) | The newest event is younger than `--stale-hours`. |

Any warning or failure prints a **repair prompt** to paste into your coding agent. It quotes the findings as untrusted data and names the exact flag when the event came from this tool. Exit code: `0` nothing failed, `1` a check failed, `2` a secret reached Raindrop (rotate it).

First live run (2026-10-25) found a Raindrop behaviour this tool now avoids. A span sent while its tool call was still running (`UNSET`) and sent again when finished is kept **twice** in the event's `tool_calls`, though only once in the trace. Ingest now sends only finished tool calls; `properties.tool_calls_unfinished` counts the rest, and the next run sends them.

## Redaction (always on)

Before anything is sent, these are replaced with `[REDACTED]`:

- the values of `RAINDROP_WRITE_KEY`, `RAINDROP_QUERY_KEY` and `RROFLOW_REDACT`;
- bearer tokens;
- `sk-`, `gh*_`, `xox*-` and `AKIA` keys;
- PEM private keys;
- hex strings of 48 or more characters.

Verified on 2026-10-24 against a transcript containing both live Raindrop keys: neither key appeared in the dumped payloads.

Raindrop's own server-side PII guard also runs, and it rewrote "RROFLOW" and "Claude" as `[PERSON_NAME]`. On our plan, Settings → PII redaction reports "This section isn't available for your organization", so these terms can't be allowlisted. Signals still work, but expect product names in inputs to be masked.

## Behaviour verified against the live API (2026-10-25)

- **Re-sending upserts.** Raindrop replaces an event with the same `event_id` and merges spans by span id. Because ids are deterministic, re-running with `--payloads` upgrades existing events in place without duplicating them. Observed: same event, `received_at` advanced, spans 21 → 39.
- **Ingestion lag is about 3–5 minutes** before the event can be read back.
- **`--payloads` matters for signals, not just onboarding.** Raindrop's generated classifiers skip any turn with no output and no tool input/output/error text before calling the model. Tool payloads arrive as span `input` / `output` (`traceloop.entity.input` / `traceloop.entity.output`).
- **Run it per turn, not as a batch.** Claude Code rewrites the session transcript when it compacts context, so turns before the compaction are gone from the file. Run it from a Claude Code `Stop` hook with `--since`, or on a short schedule.
- **Context-compaction summaries are not prompts.** A turn opened by a compaction summary takes the original prompt's text when it is still in the file. Otherwise it takes the latest `last-prompt` record, and only as a last resort is it marked `[continued after context compaction]`, and either way it is tagged `properties.compacted = true`.
- **Reading back through the Query API.** The project is selected by the `X-Raindrop-Project-Id` header. A `?project=` query parameter is silently ignored and falls back to the default project.

## Verified

On 2026-10-24, one real session (10 main turns and 4 subagent turns, 488 spans) was sent to project `claude-code`. It was read back through the Query API: 467 tool calls, 11 tool errors, plus model and token counts.
