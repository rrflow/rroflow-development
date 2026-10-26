# rroflow-development

**RROFLOW (rroflow.ai): Sentry for your AI.** It captures what your coding agents actually do, finds where they fail, stall or waste your money, and learns from your corrections. It runs on Shippin.cloud / shippin.ai. The reasoning engine, RRFlow (rrflow.ai), lives in [rrflow/rrflow-development](https://github.com/rrflow/rrflow-development). This repo is the RROFLOW lab and site; it moves to `rrflow/rroflow` at alpha.

| Path | What it is | State |
|---|---|---|
| [`crates/rroflow-core`](crates/rroflow-core) | `rroflow.turn/v1` record and behaviour signals v0 (deterministic, with evidence) | Working; signals measured on real Claude Code and Codex sessions |
| [`crates/rroflow-cli`](crates/rroflow-cli) | `rroflow`, the collector (first SDK): reads Claude Code and Codex sessions, evaluates signals locally, sends to rroflow.ai or Raindrop; `doctor` | Working; real sessions sent end to end |
| [`crates/rroflow-ingest`](crates/rroflow-ingest) | rroflow.ai ingest: files turns into the sender's devspace (key decides devspace) | Working locally; not yet deployed to Shippin.cloud |
| [`docs/plan`](docs/plan) | Plan v1 and the Raindrop + Sentry capability map | Proposal; decisions D-1 to D-3 answered |

```bash
cargo build --release
# See what your agents did, locally, sending nothing:
target/release/rroflow signals --agent claude ~/.claude/projects
target/release/rroflow signals --agent codex  ~/.codex/sessions
```

CI runs format, clippy, tests and a fixture parse and signal run on every push and pull request.
