# Identity and devspaces on rroflow.ai

Date: 2026-10-26. Decisions D-4 (hosted store: Postgres on tx-infra-prod) and D-6 (devspace owned by the Nomen account) in [rroflow-ai-plan.md](rroflow-ai-plan.md).

## Model

```
Nomen account (issuer, sub)  ──owns──▶  devspace  ──holds──▶  turns, signal hits, signal settings
                                            │
                                            └──attached to (optional)──▶  Shippin.cloud tenant
```

- **Nomen** is the identity provider shared by every Shippin service (a Zitadel fork). rroflow.ai does not keep its own accounts.
- **Devspace** is one person's space on rroflow.ai. Table `devspaces`: `id`, `owner_issuer`, `owner_subject`, `owner_org` (Nomen claim `urn:zitadel:iam:user:resourceowner:id`), `shippin_tenant_id`. One devspace per Nomen account (unique index on issuer + subject).
- **Growing into Shippin.cloud** sets `shippin_tenant_id`. Turns stay where they are, so nothing is copied and nothing can be lost in a move.
- **The credential decides the devspace.** A client never names a devspace; the server derives it from the key or token. Requests can only read and write their own devspace.

## Credentials

| Who | Today (built) | Next (Nomen) |
|---|---|---|
| Collector (`rroflow ingest-claude`, `ingest-codex`) | ingest key; server stores only its SHA-256 | `rroflow login` via the OAuth device flow, like `devagotchi-cli@shippin`; refresh token kept in the OS keychain |
| Dashboard | ingest key pasted into the tab (session storage only) | sign in with Nomen: authorization code + PKCE |
| Server check | key hash lookup | opaque token introspection (RFC 7662) with private-key JWT client auth, answers cached 60 s; Nomen unreachable → 503 `issuer_unavailable`. Same pattern as the Devagotchi hub (`crates/hub/src/nomen.rs`). Required: issuer, `aud` = the rroflow project, `client_id` in the allowed list, `exp`, `sub` |

Keys stay supported after sign-in exists, for headless machines and CI, but are minted by a signed-in user for their own devspace.

## API (ingest)

| Method and path | Auth | What it does |
|---|---|---|
| `POST /v1/turns` | devspace | upsert `rroflow.turn/v1` records by `event_id` |
| `GET /v1/turns?agent=&signal=&limit=` | devspace | newest first; filter by agent (`claude-code`, `codex`) and signal |
| `GET /v1/summary` | devspace | totals, per-agent signal counts, the catalogue, the devspace's settings |
| `GET /v1/signals` | public | catalogue `rroflow.signals/v1`: categories (Failure, Friction, Cost, Context), signals, options with defaults and ranges |
| `GET`/`PUT /v1/signals/settings` | devspace | which signals run and their thresholds; collectors read this before evaluating |
| `GET /healthz`, `GET /` | public | health; dashboard page |

## Build steps

1. **Built:** Postgres store with migrations, signal catalogue grouped with options, per-devspace settings, dashboard with Claude Code and Codex side by side, deploy script with Postgres for tx-infra-prod.
2. Nomen token introspection on the ingest; first sign-in creates the devspace for that `sub`.
3. `rroflow login` (device flow) and token refresh in the CLI.
4. Dashboard sign-in (authorization code + PKCE); key management in the page.
5. `api.rroflow.ai` public: DNS A record to 23.156.136.173, TLS at a reverse proxy in front of `127.0.0.1:8787`.
6. Attach a devspace to a Shippin tenant (`shippin_tenant_id`), started from the Shippin console.

## Inputs needed for steps 2–5

- A Nomen project for rroflow and its project id (the `aud`).
- An API application in that project with private-key JWT, for introspection (key file installed on the host, never in the repo).
- A native/device client for the CLI and a web client (PKCE) for the dashboard; their client ids.
- DNS for `api.rroflow.ai` (and `rroflow.ai`).
