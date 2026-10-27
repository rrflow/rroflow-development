# Browser sign-in and your Claude Code / Codex activity

Implementation in the Texas readiness worktree. Live Texas deployment and real
Nomen account sign-in remain unverified.

The dashboard uses the existing Nomen dashboard public client with authorization
code + S256 PKCE. The Rust gateway exchanges the code and introspects the access
token before opening the same account-owned devspace used by the collector.
Neither the browser nor RROFLOW needs a Claude or Codex provider credential.

## Configuration on Texas

Keep the existing `[nomen]` configuration and provision its API key through the
operator's secrets mechanism. Add this block to the configuration retained by
the installer (`/etc/rroflow/nomen.toml`):

```toml
[browser]
origin = "https://app.rroflow.ai"
client_id = "396821568908492804"
```

Register the exact callback `https://app.rroflow.ai/auth/callback` on that public
client, with token endpoint authentication `none` and PKCE enabled. It must also
appear in `nomen.clients`. The service refuses insecure origins, and browser
sign-in requires one configured Nomen project audience. Without `[browser]`,
the UI says account sign-in is unavailable and retains the key entry.

Use the canonical app hostname: cookies are host-only, so signing in there does
not sign a browser into the API hostname. Both can reach the same Rust service.
Do not log query strings on `/auth/callback` in the reverse proxy: they contain
short-lived authorization codes. Tokens and callback responses are never logged
by this implementation.

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

## Populate the owner's dashboard

On the computer running Claude Code and Codex, install the collector release,
run `rroflow login`, then `rroflow whoami`. Use the **same Shippin account** as
the browser. An ingest-key environment override takes precedence over login;
inspect `whoami` before sending, and remove that override if it names a different
devspace. Run the existing ingest commands with explicit session-directory paths:

```powershell
rroflow ingest-claude "$HOME/.claude/projects"
rroflow ingest-codex "$HOME/.codex/sessions"
```

Use the actual configured session folders when different. These commands upload
metrics and signal results; content is omitted unless `--content` is explicitly
chosen. They are one-shot imports; ongoing capture hooks and automatic discovery
are separate work. Refresh the dashboard to see both agents' received records.
No record means **no captured activity**, not proof of a provider connection.

Existing key-owned data needs the operator's explicit `rroflow-ingest claim`
operation after checking the account subject. Never assign the `jesse` devspace
automatically to the first user who signs in. Existing ownership/claim safeguards
remain in force.

## Proof required before calling it live

Run Rust tests (including Postgres/Nomen tests), dashboard state tests, formatting,
clippy and deployment preflight. Then verify on Texas: trusted VM identity,
app DNS/TLS, configured Nomen key/client/callback, real account sign-in,
same-owner Claude and Codex captures, second-account isolation, and sign-out.
The local OAuth transport test uses a test credential and mock token endpoint;
it does not claim real Nomen or Texas authentication.

Protocol references: [Zitadel endpoints](https://zitadel.com/docs/apis/openidoauth/endpoints)
and [frontend/backend audience scopes](https://zitadel.com/docs/guides/solution-scenarios/frontend-calling-backend-API).
