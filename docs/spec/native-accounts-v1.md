# Native accounts v1

Status: implemented on the development branch, under verification. Not deployed.
The owner accepted native accounts alongside Nomen in Shippin ADR 0013. This slice
implements browser signup, sign-in and recovery. Native CLI device authorization,
explicit account linking, MFA/passkeys and public-launch security review remain open.

## A person's journey

`/auth/login` opens `/app/account` when native accounts are configured. The React
page offers email sign-in, registration when enabled, and password recovery when
mail is configured. "Continue with Shippin" goes through `/auth/federation` when
browser federation is configured. Both routes use the same existing organization
and project APIs; signing in never grants a paid entitlement or dataset consent.

Registration first sends a verification link to the email address. It does not
reserve an account or accept a password before mailbox ownership is demonstrated.
The one-use link lets the recipient set their password and creates their canonical
user, native identity, personal organization and default project in one transaction.
They then sign in explicitly. An existing Nomen identity with a matching display
name or email is never silently linked.

Recovery uses the same one-use mechanism, bound to the credential revision at the
time of request. Completing recovery replaces the password, increments that
revision, deletes all native sessions and invalidates all outstanding links for
that email in one transaction. A concurrent login rechecks the password revision
after taking its row lock. A concurrent completion rechecks expiry and token use
after hashing and after taking the email lock. No reset automatically signs in.

## Stored identity and sessions

Migration `0016_native_accounts.sql` adds credentials, actions, an encrypted mail
outbox, session digests and durable attempt windows. Emails are trimmed ASCII
addresses, case-insensitive by application policy. Plus tags and dots are preserved;
internationalized addresses are not supported in this version.

Passwords accept 15–128 Unicode characters (at most 512 UTF-8 bytes), without
normalization or composition rules. Argon2id v19 uses 19 MiB, two iterations and
one lane, with a random salt. Password work runs on bounded blocking workers;
saturation refuses more work. Verification checks the encoded profile before
allocating its memory. An unknown account performs a dummy password verification.
This is not a claim of constant-time HTTP responses or a completed abuse review.

Native sessions are random 256-bit credentials stored as SHA-256 digests, bound to
the configured origin and credential revision. Every request rechecks the session
and current project membership. Native tokens are never sent to Nomen. Browser
cookies are Secure, HttpOnly, SameSite=Lax and host-only. Cookies never enter browser
storage or JSON responses. Explicit bearer credentials keep precedence for API
clients. Switching sign-in methods revokes and clears the previous browser session.

Cookie writes and all native account mutations require the configured Origin and
`X-Rroflow-Request: dashboard`; no cross-origin permission is granted. Native JSON
bodies are limited to 4 KiB. Authentication responses use no-store and no-referrer.
Storage outages return an unavailable response, not a false password refusal.
Verification, password resets, successful logins and logouts write account audit
events into the personal organization's existing audit trail, without credentials.

## Configurable delivery and bounds

Example values below are placeholders; endpoints and secret paths are operator
configuration. No SMTP or AI provider credential belongs in Git.

```toml
[accounts]
origin = "https://your-member-host.example"
encryption_keys_file = "/run/secrets/account-mail-keys.json"
signup_enabled = false
action_ttl_seconds = 1800
session_ttl_seconds = 86400
password_concurrency = 4
sessions_per_user = 10
max_pending_emails = 5000
global_attempts = 300
email_attempts = 5
mail_poll_seconds = 5

[accounts.smtp]
host = "your-smtp-host.example"
port = 587
from = "accounts@your-member-host.example"
credentials_file = "/run/secrets/account-smtp.json"
```

PostgreSQL is required. If browser federation is also enabled, its origin must
match. Omitting `[accounts]` disables native accounts. `signup_enabled` defaults to
false; enabling it without an SMTP adapter is refused. The existing native accounts
can still sign in when registration is disabled. Removing SMTP disables recovery.

Limits are shared in PostgreSQL across serving instances. The global and per-email
attempt windows are ten minutes, separately for email requests, password login and
link completion (token digest for the latter). Email requests use the same response
for eligible and ineligible addresses. Full rate windows return 429 with Retry-After.
Expired windows are removed; denied attempts cannot grow the rate table. Queue
capacity is checked transactionally. Sessions have a per-user limit; a successful
new sign-in removes the oldest sessions above the limit. These are abuse bounds,
not subscription allocations. Distributed/IP-level abuse controls and public
capacity tuning still require launch review.

Mail keys use the same private key-ring format as [browser sessions](browser-sessions-v1.md):
an active key id and up to three 32-byte hex keys. Unix permissions must exclude
group/other access; operators must protect Windows files with private ACLs. Rotation
retains older keys until their queued links expire. AES-256-GCM with library-generated
nonces binds each envelope to origin, token digest, recipient, purpose, expiry and
key id. Changing any of these makes it unreadable. The SMTP file contains only
`username` and `password`; it is private and bounded to 16 KiB.

Run the separately supervised worker with the same configuration:

```sh
rroflow-ingest account-mail-worker --config /path/to/service.toml
# One bounded delivery attempt, with counts suitable for a smoke test:
rroflow-ingest account-mail-worker --config /path/to/service.toml --once
```

SMTP requires verified STARTTLS, with no plaintext or certificate-bypass switch.
The worker locks one queued row, sends with a bounded timeout, and records delivery
or schedules retry with backoff. Competing workers skip locked rows. A crash after
SMTP accepts can redeliver the same link; link consumption remains one-use. Expired
actions and envelopes are removed. Failed envelope authentication stays queued
until corrected or expired, and is never sent. Logs report counts, not recipients,
links, passwords or SMTP errors that may contain addresses.

Links use URL fragments, keeping the secret out of server access logs. The account
page removes the fragment and handles links opened into an existing page. Production
mail delivery and mailbox receipt must be qualified with the selected provider before
public signup is enabled. No provider has been selected or configured by this change.

## Verification boundary

Rust tests use actual PostgreSQL schemas and the real HTTP router. They cover
ownership-before-creation, concurrent single consumption, recovery invalidation,
origin and credential-revision binding, fresh membership, CSRF, durable attempt
limits, queue capacity, mail retry/tamper/expiry and fail-closed storage behavior.
Lock-wait tests exercise expiry and password changes during pending operations.

Playwright runs desktop and phone registration, login and recovery through the real
Rust server into real project records. Its isolated fixture reads and authenticates
the test mail envelope instead of delivering SMTP, and routes HTTPS browser requests
to a loopback server. It therefore does not qualify real TLS termination, real SMTP
delivery, a production member or Texas. There is no production token-reading test API.

The previous 44 browser scenarios and existing Rust/federation tests remain required.
Keep public signup disabled until delivery, abuse controls and the security review
are qualified. Existing release activation must also handle the new one-way migration
and prove backup/restore before Texas deployment.
