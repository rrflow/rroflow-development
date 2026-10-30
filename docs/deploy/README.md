# Deploying rroflow.ai

Date: 2026-10-27. Status: plan. This covers where rroflow.ai runs, how GitHub gets builds onto the server, and what has to move from the first install. Decisions D-4, D-7 and D-8 are in [../plan/rroflow-ai-plan.md](../plan/rroflow-ai-plan.md).

## Where it runs: Texas

rroflow.ai runs on **Texas** (`tx-infra-prod.shippin.cloud`, 23.156.136.173, a Proxmox host).

- **D-4 (2026-10-26):** the hosted store is Postgres on tx-infra-prod.
- **The 2026-10-27 site split:** Texas is Shippin's site (Nomen at `id.shippin.cloud`, Vaultix, the edge). Wisconsin is Devagotchi's (devagotchi.com, CI runners).
- rroflow.ai is a Shippin service, and it calls Nomen on every sign-in.

The scripts are written so the same release could run behind another edge. Wisconsin would need an edge adapter, because its front door is nginx, not the Caddy edge container that Texas has. That adapter is not built yet.

## How Texas serves a service

Nomen established this layout ([shippinAI/nomen `deploy/texas/README.md`](https://github.com/shippinAI/nomen), PR #44):

```
client ──443──▶ Texas host DNAT ──▶ edge container (CT 100, Caddy) ──▶ service VM on vmbr1
                                     TLS ends here                       (10.40.0.0/24, gateway 10.40.0.1)
```

- Each service gets its own VM on the private bridge `vmbr1`.
  - CT 100 `edge`: Caddy, holds every certificate.
  - CT 110 `hub`: Devagotchi's hub, moving to Wisconsin.
  - VM 120 `nomen`: 10.40.0.30.
- A one-time root paste on the host creates the VM and appends the service's site block to the edge's `/etc/caddy/Caddyfile`. It keeps a backup, runs `caddy validate`, and puts the backup back if anything fails.
- The host's DNAT only takes traffic that arrives from outside. A guest that needs another Texas service by its public name gets an `/etc/hosts` line pointing at the edge. rroflow needs this for `id.shippin.cloud`, because the ingest introspects every Nomen token.

## Target layout for rroflow.ai

```
api.rroflow.ai ─┐
                ├─▶ edge CT 100 (Caddy, TLS) ──▶ VM `rroflow` :8787 ── rroflow-ingest (API + dashboard files)
app.rroflow.ai ─┘                                    │
                                                     └─ Postgres 17, localhost only, peer auth
```

| Item | Value |
|---|---|
| VM | `rroflow`, proposed id 130 at 10.40.0.40. The bootstrap preflight refuses an id or address already in use. |
| Size | 2 cores, 4 GB, 40 GB, Debian 13 cloud image pinned by SHA-512 (same image as Nomen's) |
| Process | `rroflow-ingest serve` under systemd, as user `rroflow`, with the same hardened unit as today |
| Store | Postgres inside the VM, database `rroflow`, Unix socket, peer auth (no password). Moving to a shared Postgres stays open, as it does for Nomen. |
| Port | 8787 answers the edge only, through an nftables table of its own (same pattern as `inet nomen`) |
| Edge routes | `api.rroflow.ai` and `app.rroflow.ai`, both to the VM. The dashboard and its API share an origin, so there is no CORS. |
| Nomen | `/etc/rroflow/nomen-key.json` (from Gold's sops, never in git), plus a hosts line `id.shippin.cloud → edge` |

## How releases reach the server (D-8: the server pulls)

The repository is public. A self-hosted runner in a production VM can be reached by workflows anyone proposes, and GitHub advises against self-hosted runners on public repositories. So no runner and no GitHub credential live on the server. The VM pulls:

1. **CI builds** `rroflow-ingest` for Linux x86_64 and aarch64. The dashboard bundle is built into the binary, so a single file carries both. Each build is published to a GitHub Release with SHA-256 sums and a GitHub build attestation, the same way the CLI ships.
2. **`rroflow-host update`** runs in the VM on a systemd timer, every 10 minutes:
   - finds the newest release for its channel;
   - downloads the tarball and checksum;
   - verifies the checksum and the attestation (signed by this repository's release workflow);
   - installs the new binary and keeps the previous one;
   - restarts the service and waits for `/healthz`.

   If the health check fails, it rolls back to the previous binary. The verbs (`provision`, `update`, `install`, `restart`, `rollback`, `status`) follow Nomen's `nomen-host`.
3. **Pinning:** `/etc/rroflow/channel` holds `stable` (newest release) or an exact tag. Writing a tag there holds or rolls back a deploy.

This keeps the earlier rule "deploy from GitHub only, no hand-built copies". Builds are made only by GitHub Actions, and the server checks where each build came from before running it.

## What runs today and what moves

- **On 2026-10-26, `deploy/tx-infra-prod/deploy.sh` installed the ingest directly on the Texas Proxmox host:**
  - it built from source with a Rust toolchain on the host;
  - it installed the host's own Postgres with database `rroflow`;
  - it runs on `127.0.0.1:8787`;
  - its self-test passed.

  That was the approved first step. It does not match the layout above.
- **Do not run its TLS step on Texas.** It installs Caddy on the host, but 443 is forwarded to the edge container, so the certificate could never be issued there. PR #14 changes the default to `PUBLIC_HOSTS` empty and adds a read-only preflight that refuses to install on the hypervisor ([../plan/texas-readiness.md](../plan/texas-readiness.md) once merged).
- **The move happens once, in the bootstrap:**
  1. `pg_dump` the host's `rroflow` database into the VM.
  2. Copy `/etc/rroflow/keys/*` and `nomen.toml` into the VM.
  3. Start the VM's service.
  4. Check the turn counts match.
  5. Stop and disable the host's `rroflow-ingest`.

  Removing the host's Postgres package is a separate step, and the owner decides it, because other things on the host may use it.

## Dashboard (D-7: Rust API + React)

- **`web/`:** React 19, Vite and TypeScript, the same stack as Devagotchi's apps (`apps/desktop`, `apps/website`).
- **Types:**
  - TypeScript types are generated from the Rust records (`rroflow.turn/v1`, `rroflow.signals/v1`, the API responses) through JSON Schema.
  - Devagotchi already does this with `json-schema-to-typescript`.
  - CI fails if the generated types drift from the Rust ones.
- **Serving:** CI builds the bundle, and `rroflow-ingest` embeds it and serves it at `/`. The API stays under `/v1`. There is one release and one process.
- **Sign-in:** through the Rust service, as PR #14 builds it: Nomen authorization code + S256 PKCE with the `rroflow-dashboard` app (`397185155288072195`, redirect `https://app.rroflow.ai/auth/callback`). The access token stays in server memory, and the browser holds only an opaque `__Host-` HttpOnly session cookie, so the React app never handles a token.
- **First screen:** parity with today's `dashboard.html`, with Claude Code and Codex side by side, signals by category, and settings. After that come evidence cards, spend and limits, and linked agents.
- **The public site stays as decided (D-3):** Astro static with islands. Its islands can reuse the dashboard's React components.

## Increments (one PR each)

1. **This record:** deploy plan, D-7 and D-8, status page, docs index. (In review alongside it: #14 browser sign-in + Texas preflight, #15 durable capture + signal reviews.)
2. **Release `rroflow-ingest`** for Linux x86_64 and aarch64 from GitHub Actions, with checksums and attestations, published automatically on a version bump. Done: from v0.3.0 the ingest and the CLI share one version and one release (`rroflow-ingest-<target>.tar.xz`), and CI fails if their versions differ.
3. **`deploy/texas/`:** the bootstrap paste (preflight, VM, edge routes, hosts line, data move) and `rroflow-host` with pull updates. It will be tested against stand-ins for the Proxmox commands, the way Nomen's was.
4. **`web/` dashboard** at parity, embedded in the ingest. Types are generated from Rust, with a CI drift check.
5. **Key management in the page** (sign-in itself is #14).

## Owner steps (each needs root, DNS or Nomen admin)

1. In Cloudflare, add `api.rroflow.ai` and `app.rroflow.ai` as `A 23.156.136.173`, DNS only.
2. In Nomen, make sure `rroflow-dashboard` has the redirect `https://app.rroflow.ai/auth/callback`. The first guess was `https://rroflow.ai/auth/callback`.
3. After increment 3: paste the bootstrap into a root shell on the Texas host, with the Nomen key file ready from Gold's sops.
