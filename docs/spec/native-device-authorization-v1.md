# Native collector authorization v1

Status: implementation under local qualification. Not deployed. This extends
[native accounts](native-accounts-v1.md) and the existing protected
[collector account store](collector-account-storage-v1.md).

## Member journey

An explicitly configured `rroflow login --auth-method native` displays a short
code and a member-page URL. The person opens `/app/device`, signs in with their
RROFLOW email account, enters the code, reviews the account email, registered
client, collector-supplied device name and permissions, and selects a writable
project. The collector receives a grant for that one project after approval.
Neither a password nor a browser-session credential reaches the CLI.

The page lists this native account's active collector approvals and allows
disconnecting one. An approval is not an online heartbeat. Disconnect refuses
subsequent credential use; previously stored captures remain. New signup mail
currently returns to the account page rather than preserving an in-progress
device approval. Existing-member sign-in returns to the device page.

## Configuration and authority

Device authorization is disabled unless `[accounts.device]` is configured:

```toml
[accounts.device]
resource = "https://your-ingest.example"
authorization_ttl_seconds = 600
access_ttl_seconds = 900
grant_ttl_seconds = 2592000
max_pending = 5000
max_grants_per_user = 20
global_attempts = 600
user_attempts = 20
[[accounts.device.clients]]
id = "your-collector"
name = "Your collector"
```

The resource is a canonical HTTPS origin; exact loopback HTTP origins are allowed
for development. Public client IDs and names, URLs, limits and lifetimes are
configuration. Browser origin remains the native-account HTTPS origin. API and
browser origins can differ. Limits are validated at startup; grants have a fixed
expiry and do not extend indefinitely through refresh.

The collector profile explicitly sets `auth_method: "native"`, issuer, client ID,
ingest URL and optionally `device_name`. Omitted authentication method preserves
the existing Nomen adapter. A native profile must omit the Nomen `project_id`
audience; its destination project comes from the approved grant. Account files
keep that project under `native.project_id`, separately from a Nomen audience.
Changing a profile never changes an existing queue's destination.

The only granted scope is `capture`. Access permits GET `/v1/whoami` and
`/v1/signals/settings`, and POST `/v1/turns`, `/v1/events` and `/v1/sources/health`.
All other paths are refused for these credentials, including record reads,
project/account management and dataset consent. Each use checks grant revocation,
credential epoch, registered client, expiry and current writable membership. A
project override cannot widen access. Plan allowances and content consent remain
independent authorities. Password recovery invalidates grants through the epoch.

## Persistence and retry protocol

Migration 0017 stores hashed device/user codes and access/refresh credentials,
account/project bindings, expiry and approval state. The user code has 50 random
bits. Approval requires a live native browser cookie, exact Origin and the
dashboard request header; bearer credentials cannot approve another device.
Pending capacity and per-user grant capacity are enforced under database locks.
Expiry is rechecked using the database clock after lock waits.

Form endpoints under `/auth/native/device/` are `start`, `token` and `revoke`.
Browser JSON endpoints are `inspect`, `decide`, `grants` and `disconnect`.
All responses are no-store/no-referrer. Polls start at five seconds; early polls
increase the interval. The CLI bounds approval waiting to 15 minutes and backs
off after transport failures. Token requests carry a 32-random-byte hex
`request_id`; it is an extension of the device/refresh grants, not a claim of
being a general OAuth authorization server.

Device exchange and refresh atomically store their result with the new grant or
rotation. A repeated credential plus the same request ID recovers that exact
result; it does not mint another rotation or extend the returned access expiry.
The latest retry result is AES-GCM encrypted using the existing configured
account keyring and context-bound associated data. Keep old encryption keys
until grants whose retry receipts use them have expired or been revoked.

Before refresh, the CLI synchronizes the request ID into the locked private
account file. Transport failure or process termination leaves the old token and
that intent together. A later process repeats the same request. It saves a
recovered result before attempting another rotation if the recovered access
token expired during the outage. At most two exchanges occur per refresh pass.
No expiry grace shortcut prevents a still-valid native grant from recovering
after more than seven days offline.

Reusing a spent refresh credential with a different request ID revokes the grant.
Old credential/request relationships remain until grant expiry, but only the
latest rotation retains encrypted response credentials. Superseded same-ID
receipts are refused without creating another rotation. Explicit revoke accepts
current or spent refresh credentials so logout can revoke after response loss.
The browser's own logout does not disconnect collectors.

## Qualification and limits

PostgreSQL tests exercise concurrent refresh retries, response recovery after
server reconstruction, request tampering, consent, scope, capacity, membership,
password reset, revocation and expiry while waiting for locks. Browser tests use
the packaged service for approval and disconnect on desktop and phone, including
accessibility and literal rendering of untrusted device names. A real CLI/service
test exercises lost responses, process termination, SDK/transcript delivery,
queue retention after disconnect and deployment mismatch before refresh.

These use disposable accounts, databases and provider-shaped records. They do
not prove real vendor subscription capture, production SMTP, public abuse
resistance, cross-platform power-loss behavior, account linking or Texas
activation. Nomen refresh recovery remains governed by that issuer; this protocol
does not change its refresh semantics.

Protocol inputs: [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html) for device
approval and polling, and [RFC 9700 section 4.14](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14)
for rotation, grant binding and replay detection. The selected durable retry
receipt adds recovery after an ambiguous network result; merely retrying a spent
rotating token with a fresh request identity was rejected because it would
trigger revocation after an ordinary lost response.
