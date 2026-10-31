# Browser sign-in and your Claude Code / Codex activity

Updated 2026-10-31. Implemented in the application source. Live Texas deployment
and real Nomen account sign-in remain unverified.

The dashboard uses the existing Nomen dashboard public client with authorization
code + S256 PKCE. The Rust gateway exchanges the code and introspects the access
token before opening the same account-owned devspace used by the collector.
Neither the browser nor RROFLOW needs a Claude or Codex provider credential.

## Deployment configuration

Configure the service's retained external TOML as described in
[identity and devspaces](identity-and-devspaces.md#configure-the-deployment).
The `[browser]` origin and public client come from that deployment's verified
identity registration. No production client ID or hostname is a compiled default.
The ingest application's private key remains an operator-managed server file;
neither the browser nor the collector receives it.

Register the exact configured origin plus `/auth/callback` on the dashboard
public client, with token endpoint authentication `none` and PKCE enabled.
The client must also appear in `nomen.clients`. The service refuses insecure
origins and requires one configured project audience for browser sign-in.
Without `[browser]`, the UI reports that account sign-in is unavailable and
retains the advanced key entry.

Use the canonical app hostname: cookies are host-only, so signing in there does
not sign a browser into a different API hostname. Both may reach the same Rust
service. Do not log callback query strings in the reverse proxy; they contain
short-lived authorization codes. Tokens and callback responses are not logged
by this implementation. Use the [managed release procedure](hosted-releases.md)
after placement/recovery checks, not the old source-build installer.

## Session behavior

- Login transactions expire after ten minutes and are bound to a random browser
  cookie. A callback is single-use. State and PKCE are independently random.
- Access tokens stay in server memory. The browser receives an opaque
  `__Host-` cookie with Secure, HttpOnly, Path=/ and SameSite=Lax.
- API requests still pass through Nomen introspection and the existing
  account-to-devspace mapping. Cookie writes require the exact configured Origin
  and the dashboard's custom header. Collector bearer credentials remain supported.
- Sessions last at most the access token lifetime or one hour, whichever is
  shorter. They are bounded in number and cleared by server restart. This version
  does not request a refresh token; sign in again after expiry. Multiple service
  instances require sticky routing or a separately designed shared session store.
- Sign out invalidates the local session and pending login, and clears displayed
  data. It does not sign the user out of every Shippin service.
- Responses have `Cache-Control: no-store` and `Referrer-Policy: no-referrer`.

## Connect the activity you expect to see

On the execution machine, use the operator-provided public deployment file:

```sh
rroflow setup --deployment /absolute/path/to/deployment.json
rroflow whoami
rroflow sources status
```

Use the same Shippin account as the dashboard. Setup asks before linking Claude
Code/Codex session sources and registers per-user startup unless `--no-service`
is chosen. Check Sources for fresh received reports and Activity for the actual
turns. A registered startup service alone does not prove delivery. Text remains
opt-in. See [deployment configuration](deployment-configuration.md) and
[source collection](source-collection.md) for queue/account binding and recovery.

Member-oriented setup refuses an ambient headless ingest key. For deliberate
key-based collection, follow the separate headless path in the deployment guide;
inspect `whoami` before sending. An unrelated key must not silently redirect a
member's activity into another workspace.

One-shot historical imports remain available with explicit local session paths:

```powershell
rroflow ingest-claude "$HOME/.claude/projects"
rroflow ingest-codex "$HOME/.codex/sessions"
```

Use the actual folders when different. These commands do not establish ongoing
collection; managed sources provide that separate capability. SDK application
calls also require their [wrappers and delivery setup](../../sdk/javascript/README.md).
Neither path establishes coverage of provider browser/mobile history.

Existing key-owned data requires the operator's explicit ownership claim after
checking the intended account subject; see [identity and devspaces](identity-and-devspaces.md).
No existing owner space is automatically assigned to the first visitor.

## Proof required before calling it live

Run Rust tests (including Postgres/Nomen tests), dashboard state tests, formatting,
clippy and deployment preflight. Then verify on Texas: trusted VM identity,
app DNS/TLS, configured Nomen key/client/callback, real account sign-in,
same-owner Claude and Codex captures, second-account isolation, and sign-out.
The local OAuth transport test uses a test credential and mock token endpoint;
it does not claim real Nomen or Texas authentication.

Protocol references: [Zitadel endpoints](https://zitadel.com/docs/apis/openidoauth/endpoints)
and [frontend/backend audience scopes](https://zitadel.com/docs/guides/solution-scenarios/frontend-calling-backend-API).
