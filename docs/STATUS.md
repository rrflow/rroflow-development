# rroflow.ai status

Updated 2026-10-27. This page says where everything stands. The plan is [plan/rroflow-ai-plan.md](plan/rroflow-ai-plan.md), and every document is indexed in [README.md](README.md). This repository is the record: the claude.ai project copies are refreshed from here.

## Built

| Part | State | Where |
|---|---|---|
| `rroflow` CLI (the SDK) | `ingest-claude`, `ingest-codex`, `signals`, `doctor`, `login` / `logout` / `whoami` (Nomen device flow) | `crates/rroflow-cli` |
| Shared records | `rroflow.turn/v1` and the signal catalogue `rroflow.signals/v1` (Failure, Friction, Cost, Context) | `crates/rroflow-core` |
| Ingest API | Postgres store with migrations, upsert by `event_id`, summary, per-devspace signal settings, `whoami`, Nomen token introspection, `claim` | `crates/rroflow-ingest`; API table in [plan/identity-and-devspaces.md](plan/identity-and-devspaces.md) |
| Dashboard | one page served by the ingest, with Claude Code and Codex side by side; you sign in by pasting an ingest key (account sign-in is in #14) | `crates/rroflow-ingest/src/dashboard.html` |

## Released

- **v0.2.0** (2026-10-26): <https://github.com/rrflow/rroflow-development/releases/tag/v0.2.0>
  - `rroflow` for Windows x64 and ARM64, and Linux x64 and ARM64.
  - Shell and PowerShell installers, SHA-256 sums and GitHub build attestations.
- **v0.3.0** (on merge of the ingest release PR): the same CLI, plus `rroflow-ingest` for Linux x64 and ARM64 with checksums and attestations, for servers to pull. From here the CLI and the ingest share one version, checked in CI.
- **Publishing** is automatic: change the version in `crates/rroflow-cli/Cargo.toml` and merge. `release-on-version` then starts the release, which creates the tag.
- **CI** runs on every pull request, all on GitHub Actions at no cost because the repository is public:
  - Linux x64: fmt, clippy and tests on Postgres.
  - Linux ARM64, Windows x64 and Windows ARM64: tests on native runners.
  - Dry-run builds of all four release targets.

## Deployed

- **Texas host**, since 2026-10-26.
  - `rroflow-ingest` runs on `127.0.0.1:8787` with the host's Postgres, installed by `deploy/tx-infra-prod/deploy.sh`.
  - The self-test passed.
  - It is not reachable from outside: there are no `api`/`app` DNS records and no edge route yet.
- **Nomen sign-in** is built, and it switches on when `/etc/rroflow/nomen-key.json` is present. Whether the key has been copied there is not confirmed.
- **Next:** the service moves into its own VM behind the Texas edge, and the server pulls verified releases ([deploy/README.md](deploy/README.md)).

## Decisions (answered; not canon until saved as ADRs)

| # | Decision | Where |
|---|---|---|
| D-1 | RRFlow (rrflow.ai) is the engine; rroflow.ai is "Sentry for AI" and holds the learning vision | plan §9 |
| D-2 | Local-first collector and hosted accounts built in parallel on one event model | plan §9 |
| D-3 | Public site in `site/`: Astro static with islands, Rust gateway, Remotion explainers | plan §9 |
| D-4 | Hosted store: Postgres on Texas | plan §9 |
| D-5 | "Behaviour signal" is the term | plan §9 |
| D-6 | A devspace is owned by the Nomen account, and can be attached to a Shippin tenant | plan §9 |
| D-7 | Dashboard: Rust API + React 19/Vite/TypeScript; no Go | plan §9, [deploy/README.md](deploy/README.md) |
| D-8 | Deploys: Texas VM behind the edge; the server pulls verified GitHub releases | plan §9, [deploy/README.md](deploy/README.md) |

Also decided:
- Installers ship for Windows, Linux and ARM first.
- Devagotchi offers the rroflow.ai free tier and installs the SDK. From there it leads users to devscif and Vaultix, and tells them about its add-ons.
- Training datasets need a training consent that is separate from telemetry ([plan/sdk-install-and-linking.md](plan/sdk-install-and-linking.md)).

## In review (open pull requests)

| PR | What | State |
|---|---|---|
| #13 | `SECURITY.md`: private vulnerability reporting, data handling in scope | ready; needs private reporting turned on in the repo's Security tab |
| #14 | Browser sign-in with Nomen (PKCE through the Rust service, HttpOnly session), Texas read-only preflight, `PUBLIC_HOSTS` off by default; docs on Texas readiness, subscription support, automatic datasets v0 | draft; live Texas and Nomen not verified |
| #15 (on #14) | Durable SDK capture (local outbox, replay), capture events API, signal reviews (MATCH / NOT MATCH saved as immutable reviews), JavaScript SDK wrapper | draft; not production-ready by its own account |

## Next (one pull request each)

1. `deploy/texas/`: the bootstrap paste and `rroflow-host` with pull updates (increment 3).
2. A `web/` React dashboard at parity with the page as #14 and #15 leave it, embedded in the ingest, with types generated from Rust (increment 4).
3. `rroflow setup`: detect Claude Code and Codex, then offer to link them, backfill and capture every turn ([plan/sdk-install-and-linking.md](plan/sdk-install-and-linking.md), increment 2), building on #15's outbox.

## Waiting on the owner

1. Cloudflare: `api.rroflow.ai` and `app.rroflow.ai` as `A 23.156.136.173`, DNS only.
2. Nomen: `rroflow-dashboard` redirect `https://app.rroflow.ai/auth/callback`.
3. The Nomen key from Gold's sops, `rroflow_ingest_jwt_key_b64`, decoded to `/etc/rroflow/nomen-key.json`. Today that path is on the host; after the VM bootstrap it is in the VM.
4. After increment 3 of the deploy plan: the Texas bootstrap paste, as root on the host.

## Known leftovers

- `deploy/tx-infra-prod/deploy.sh` builds on the host. It is superseded by the VM layout, and it stays until the move is done.
