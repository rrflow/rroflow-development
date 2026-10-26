# rroflow-development

**RROFLOW (rroflow.ai): Sentry for your AI.** It captures what your coding agents actually do, finds where they fail, stall or waste your money, and learns from your corrections. It runs on Shippin.cloud / shippin.ai. The reasoning engine, RRFlow (rrflow.ai), lives in [rrflow/rrflow-development](https://github.com/rrflow/rrflow-development). This repo is the RROFLOW lab and site; it moves to `rrflow/rroflow` at alpha.

| Path | What it is | State |
|---|---|---|
| [`crates/rroflow-core`](crates/rroflow-core) | `rroflow.turn/v1` record and behaviour signals v0 (deterministic, with evidence) | Working; signals measured on real Claude Code and Codex sessions |
| [`crates/rroflow-cli`](crates/rroflow-cli) | `rroflow`, the SDK: reads Claude Code and Codex sessions, evaluates signals locally, sends to your rroflow.ai devspace (or Raindrop); `login` with your Shippin account; `doctor` | Working; released for Windows and Linux, x86_64 and ARM64 |
| [`crates/rroflow-ingest`](crates/rroflow-ingest) | rroflow.ai ingest: Postgres store, devspaces owned by Nomen accounts, signal catalogue and per-devspace settings, dashboard | Deployed on tx-infra-prod; public TLS pending DNS |
| [`docs`](docs) | Every plan, spec and research record ([index](docs/README.md)) | Plan decisions D-1 to D-6 answered |

## Install

Windows (PowerShell):
```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rrflow/rroflow-development/releases/latest/download/rroflow-installer.ps1 | iex"
```
Linux:
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rrflow/rroflow-development/releases/latest/download/rroflow-installer.sh | sh
```
Each release carries the archives for Windows and Linux on x86_64 and ARM64, their SHA-256 sums and signed build provenance (GitHub attestations). Then:
```bash
rroflow login            # sign in with your Shippin account
rroflow ingest-claude ~/.claude/projects
rroflow ingest-codex  ~/.codex/sessions
rroflow whoami           # which devspace your turns land in
```

## Build from source

```bash
cargo build --release
# See what your agents did, locally, sending nothing:
target/release/rroflow signals --agent claude ~/.claude/projects
target/release/rroflow signals --agent codex  ~/.codex/sessions
```

CI runs format, clippy, tests and a fixture parse and signal run on every push and pull request.
