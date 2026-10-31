# rroflow-development

**RROFLOW (rroflow.ai): AI monitoring, with the evidence.** It captures coding-agent activity and produces behaviour signals for review. Consented private metadata datasets are implemented as an operator-enabled member preview; qualification, supervised/unsupervised training and governed promotion remain planned. Hosted delivery targets Shippin.cloud / shippin.ai on Texas, whose deployment remains unverified. The reasoning engine, RRFlow (rrflow.ai), lives in [rrflow/rrflow-development](https://github.com/rrflow/rrflow-development). This repo is the RROFLOW lab and site; it moves to `rrflow/rroflow` at alpha.

| Path | What it is | State |
|---|---|---|
| [`crates/rroflow-core`](crates/rroflow-core) | `rroflow.turn/v1` record and behaviour signals v0 (deterministic, with evidence) | Working; signals measured on real Claude Code and Codex sessions |
| [`crates/rroflow-cli`](crates/rroflow-cli) | `rroflow`, the SDK: reads Claude Code and Codex sessions, evaluates signals locally, sends to your rroflow.ai devspace (or Raindrop); `login` with your Shippin account; `doctor` | Working; released for Windows and Linux, x86_64 and ARM64 |
| [`crates/rroflow-ingest`](crates/rroflow-ingest) | rroflow.ai ingest: Postgres store, devspaces owned by Nomen accounts, signal catalogue and per-devspace settings, dashboard | Implemented and locally tested; Texas identity, deployment and public account flow not verified |
| [`site`](site) | Public Astro page at `/`, member workspace at `/app`; embedded in the Rust service | Built and tested in development; public signup, entitlements and live Texas acceptance remain open |
| [`docs`](docs) | Every plan, spec and research record ([index](docs/README.md)) | Plan decisions D-1 to D-6 answered |

The [public and member entry](docs/ux/public-member-entry.md) now separates product discovery from the authenticated workspace, follows the configured account origin and shows onboarding progress from received evidence. It does not activate billing or provision a live hosted account.

The [AI capability baseline](docs/plan/capability-map.md) tracks Raindrop, Sentry and PostHog requirements with implementation gaps. [Signal drill-down](docs/ux/signal-drilldown.md) adds full-history counts, daily activity and agent/project navigation to exact evidence; enterprise datasets and learning remain separate required implementation stages.

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
rroflow login --deployment /path/to/deployment.json
rroflow ingest-claude ~/.claude/projects
rroflow ingest-codex  ~/.codex/sessions
rroflow whoami           # which devspace your turns land in
```

For builds containing deployment configuration, start with the
[deployment guide](docs/plan/deployment-configuration.md) and its generic JSON
example. No hosted client IDs or destination are compiled into the collector.
`rroflow setup --deployment /path/to/deployment.json` provides guided account
sign-in and session linking. The guide also records remaining customization work
and the pending license structure; this is not yet a fully white-label release.

## Build from source

The SDK capture branch adds [continuous session collection](docs/plan/source-collection.md)
and project/session dashboard navigation. This is not yet an installed background
service or a production hosted release.

The [dashboard experience increment](docs/ux/dashboard-experience.md) adds focused
navigation, searchable paginated activity, a turn inspector, signal reviews,
settings and the complete research library. It is tested source on
`codex/dashboard-experience`; local activation, visual acceptance and Texas hosted
delivery remain open. Released v0.3.0 predates the `sources` workflow; use a build
containing that feature before following its development setup commands.

```bash
cargo build --release
# See what your agents did, locally, sending nothing:
target/release/rroflow signals --agent claude ~/.claude/projects
target/release/rroflow signals --agent codex  ~/.codex/sessions
```

CI runs format, clippy, tests and a fixture parse and signal run on every push and pull request.
