# rroflow: the rroflow.ai collector

Reads the session files your coding agents already write (Claude Code and Codex CLI), turns each prompt into one `rroflow.turn/v1` record, evaluates behaviour signals on your machine, and sends the records to **rroflow.ai** (your devspace) or to **Raindrop**. Previously named `rroflow-raindrop`.

## Quick start

```bash
cargo install --git https://github.com/rrflow/rroflow-development rroflow

# 1. See what your agents did. Runs locally and sends nothing.
rroflow signals --agent claude ~/.claude/projects
rroflow signals --agent codex  ~/.codex/sessions --details

# 2. Send to your devspace on rroflow.ai. The key decides which devspace receives the data.
export RROFLOW_INGEST_URL=https://<your devspace ingest>  RROFLOW_INGEST_KEY=<devspace key>
rroflow ingest-claude ~/.claude/projects --project shippinai
rroflow ingest-codex  ~/.codex/sessions  --project shippinai
```

**Content stays on your machine by default.** Signals run locally; only signal results, metrics (tokens, tool names and statuses, timings) and record ids are sent. Add `--content` to also send redacted prompt, response and tool text.

## Behaviour signals v0

| Signal | Fires when |
|---|---|
| `blocked_handoff` | a tool result shows a blocker (denied, 401/403, not signed in) and the agent hands the step back to you |
| `tool_error_streak` | 3+ tool calls fail in a row |
| `usage_limit_hit` (v2) | a usage, rate or credit limit appears in a failed tool call or the agent's answer |
| `false_done` | the agent claims done while its last tool call failed or calls are still running |
| `aborted_turn` | you interrupted the turn |
| `context_loss` | the agent compacted its context |
| `mood` | your prompt reads frustrated (-2) to pleased (+2); prompts from other agents are skipped |
| `correction_spiral` | 3+ prompts in a row correct the agent |

Measured 2026-10-26 on this repo's real sessions:
- 10 Claude Code turns gave 5 hits, all correct, after two v1 false positives were fixed. `usage_limit_hit` had matched rate-limit text in successful code output (now v2: failed calls and answers only). `mood` had fired on a subagent prompt (Claude sidechain prompts are now marked as agent-authored).
- 8 Codex subagent turns gave 0 hits. Their prompts come from agents, and their 5 failures are not consecutive.
- That is a smoke test, not an evaluation (n is small).

## Raindrop sink

`--sink raindrop` sends Raindrop events plus OTLP spans, as before.

`ingest-claude` reads Claude Code session transcripts (`~/.claude/projects/**/*.jsonl`) and sends:

- one Raindrop event per user turn (`claude_code.turn`), with input, output, model and conversation id = session id;
- one LLM span per turn, with token counts;
- one tool span per tool call, with name, duration and status. A failed call is an ERROR span and shows up as the event's tool error.

Ids are derived from the transcript (session, agent and prompt ids), so running it twice does not duplicate events.

## Codex CLI: `ingest-codex`

`ingest-codex` reads Codex CLI session rollouts from `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (on Windows, `%USERPROFILE%\.codex\sessions`). Older rollouts that Codex has compressed to `.jsonl.zst` are decompressed in-process. It sends the same shapes as Claude Code: event `codex.turn`, one LLM span, and one span per finished tool call. It takes the same flags and environment variables.

```bash
infisical run --env dev -- rroflow ingest-codex --sink raindrop ~/.codex/sessions --project shippinai --workspace shippinai --payloads
```

Both agents stamp `properties.agent` (`claude-code` or `codex`) and `properties.agent_version`, so one Raindrop project can hold both and a signal can be filtered by agent. Claude Code events also keep `claude_code_version`.

How the rollout is read is pinned to the format in `openai/codex` at `970111f` (2026-10-23):
- Turns are bounded by `task_started` / `task_complete`. Older rollouts have no task events, so a new prompt after an answer opens a new turn.
- The prompt comes from `user_message` (legacy history) or `item_completed` → `UserMessage` (paginated history). Injected `<environment_context>`, `<user_instructions>` and AGENTS.md blocks are never taken as the prompt.
- Tools are `function_call`, `custom_tool_call`, `local_shell_call` and `web_search_call`, paired with their outputs by `call_id`.
- Codex stores no success flag. A tool call counts as failed when its output reports a non-zero `Exit code:` or `metadata.exit_code`.
- The model comes from `turn_context`. Tokens are summed from `token_count.last_token_usage`; OpenAI's `input_tokens` already include cached input.
- Unknown record types are skipped, so a newer Codex loses fields instead of failing.

Not yet validated on a real rollout: it is tested on synthetic lines built from Codex's source and its own test fixtures. Before relying on it, run `--dry-run --dump out.jsonl` on your sessions and check the output.

## Deploy in any project (shippinAI, rrflow, rroflow, clyffy)

| Variable | Meaning | Default |
|---|---|---|
| `RAINDROP_WRITE_KEY` | Raindrop write key. Inject it from Infisical or Vaultix; never commit it. | required |
| `RAINDROP_PROJECT` | Raindrop project slug for this workspace, e.g. `shippinai`, `rrflow-hydradb`, `claude-code`. | `default` |
| `RAINDROP_USER_ID` | Who ran the session | `operator` |
| `RROFLOW_WORKSPACE` | Tag stored as `properties.workspace` | none |
| `RROFLOW_REDACT` | Extra literal secrets to redact, comma-separated | none |

```bash
cargo install --git https://github.com/rrflow/rroflow-development rroflow
infisical run --env dev -- rroflow ingest-claude --sink raindrop ~/.claude/projects --project shippinai --workspace shippinai
```

Flags:
- `--since 2026-10-24T00:00:00Z` sends only new turns.
- `--payloads` adds redacted, truncated tool inputs and outputs. It is off by default.
- `--dry-run --dump out.jsonl` writes the exact payloads to a file and sends nothing. Use it to inspect them first.

A new slug's data appears under Raindrop → Settings → Projects → "Projects with data, not yet created" until you click **Create project**.

## Check what Raindrop actually stored: `doctor`

```bash
infisical run --env dev -- rroflow doctor --project claude-code   # newest event
rroflow doctor --project claude-code --event <id> --json           # one event, machine-readable
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
| Fresh data (`data.fresh`) | The checked event is younger than `--stale-hours`. |

By default `doctor` checks the newest **AI interaction** among the last `--scan-events` (50) events. Plain telemetry events (no prompt, response or tool calls) are valid Raindrop data; checked explicitly with `--event`, their AI checks show `n/a` instead of failing. Errors print as one line.

Validated on 2026-10-25 against events written by three different producers:
- `claude-code` (this tool): 6 ok, 1 warn. The warn was the duplicate-span finding.
- `rrflow-hydradb` (HydraDB brain harness): 6 ok, 1 warn. No model name on the event, true for every event in that project.
- `default` (vibeguard harness): 5 ok, 2 warn. No token counts; data 12 days old.

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
