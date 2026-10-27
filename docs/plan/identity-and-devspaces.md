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
| Server check | key hash lookup, then Nomen (built) | opaque token introspection (RFC 7662) with private-key JWT client auth, answers cached 60 s; Nomen unreachable → 503 `issuer_unavailable`. Same pattern as the Devagotchi hub (`crates/hub/src/nomen.rs`). Required: issuer, `aud` = the rroflow project, `client_id` in the allowed list, `exp`, `sub` |

Keys stay supported after sign-in exists, for headless machines and CI, but are minted by a signed-in user for their own devspace.

## API (ingest)

| Method and path | Auth | What it does |
|---|---|---|
| `POST /v1/turns` | devspace | upsert `rroflow.turn/v1` records by `event_id` |
| `GET /v1/turns?agent=&signal=&limit=` | devspace | newest first; filter by agent (`claude-code`, `codex`) and signal |
| `GET /v1/summary` | devspace | totals, per-agent signal counts, the catalogue, the devspace's settings |
| `GET /v1/signals` | public | catalogue `rroflow.signals/v1`: categories (Failure, Friction, Cost, Context), signals, options with defaults and ranges |
| `GET`/`PUT /v1/signals/settings` | devspace | which signals run and their thresholds; collectors read this before evaluating |
| `GET /v1/whoami` | devspace | which devspace the credential opens, and how (`key` or `nomen`, account ID, org) |
| `GET /healthz`, `GET /` | public | health; dashboard page |

## Build steps

1. **Built:** Postgres store with migrations, signal catalogue grouped with options, per-devspace settings, dashboard with Claude Code and Codex side by side, deploy script with Postgres for tx-infra-prod.
2. **Built:** Nomen token introspection on the ingest (ported from the Devagotchi hub, same checks and caching); first sign-in creates the devspace for that `sub`; `rroflow-ingest claim` gives an existing key devspace to an account; Nomen down → 503 `issuer_unavailable`, keys keep working.
3. **Built:** `rroflow login` / `logout` / `whoami` (device flow ported from the Devagotchi CLI, refresh within 5 min of expiry, 7-day offline grace, tokens in `account.json` 0600). The collector sends as the signed-in account; `RROFLOW_INGEST_KEY` still wins for headless machines.
4. Dashboard sign-in (authorization code + PKCE); key management in the page.
5. Public: `api.rroflow.ai` (API) and `app.rroflow.ai` (dashboard), both A records to 23.156.136.173. TLS ends at the Texas edge container (CT 100), which proxies both names to the `rroflow` VM; both reach the same service, so the dashboard's API calls are same-origin. Layout: [../deploy/README.md](../deploy/README.md).
6. Attach a devspace to a Shippin tenant (`shippin_tenant_id`), started from the Shippin console.

## Nomen setup (created 2026-10-26, project `rroflow` in the Shippin Platform org)

| What | Value |
|---|---|
| Issuer | `https://id.shippin.cloud` (endpoints under `/oauth/v2/`) |
| Project id (the `aud`) | `396821566727389188` |
| `rroflow-ingest` API app, private-key JWT (introspection) | client id `396821567230771204`; key in Gold's `platform-core.sops.yaml` as `rroflow_ingest_jwt_key_b64` |
| `rroflow-cli` native app, device flow + refresh, no secret | client id `396821568421953540` |
| `rroflow-dashboard` web app, authorization code + PKCE + refresh, no secret | client id `396821568908492804`; redirect `https://app.rroflow.ai/auth/callback` |

DNS: `rroflow.ai` → 23.156.136.173 (Cloudflare proxy off). `api.rroflow.ai` and `app.rroflow.ai` to be added the same way.

## Turning on sign-in (tx-infra-prod)

These steps are for the first install on the Texas host. In the VM layout the bootstrap carries `nomen.toml` and the key file across ([../deploy/README.md](../deploy/README.md)).

1. Put the API application's key file at `/etc/rroflow/nomen-key.json`, owner `rroflow`, mode 0600.
2. Write `/etc/rroflow/nomen.toml` (kept across deploys):

   ```toml
   [nomen]
   issuer = "https://id.shippin.cloud"
   audiences = ["396821566727389188"]
   clients = ["396821568421953540", "396821568908492804"]
   key_file = "/etc/rroflow/nomen-key.json"
   ```
3. Re-run the deploy; it prints `nomen sign-in: on`.
4. Sign in once, read your account ID from `GET /v1/whoami`, then give your existing devspace to it:
   `rroflow-ingest claim --config /etc/rroflow/ingest.toml --devspace jesse --subject <sub>` (run as `rroflow`). An empty devspace made by that first sign-in gives way; one with turns does not.
