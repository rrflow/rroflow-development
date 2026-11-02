# Browser sessions v1

Status: implementation under verification. Part of platform-v1 slice 2; this does
not complete native accounts or qualify a public signup service.

## Storage and identity

The browser holds a random 256-bit opaque `__Host-rroflow-session` cookie with
Secure, HttpOnly, SameSite=Lax and Path=/ attributes. Explicit Authorization bearer
credentials take precedence over cookies. Cookie-authorized writes require the
configured origin and the existing non-simple request header.

With `browser.session_keys_file` configured, established sessions use PostgreSQL.
The database stores a cookie digest, RROFLOW user ID, key ID, expiry, nonce and
AES-256-GCM ciphertext. Federation bearer credentials are encrypted by aws-lc-rs
`RandomizedNonceKey`; the application does not generate or reuse AEAD nonces.
Authenticated associated data binds the payload to the configured origin, issuer,
client, audience, cookie digest, user, key ID and expiry. Copying ciphertext between
deployment contexts or extending its expiry fails authentication. This is credential
confidentiality/integrity, not an audit signature or protection against a compromised
running server or restoration of an earlier database snapshot.

Every API request still passes through the existing federation verifier (whose
introspection cache lasts at most 60 seconds) and fresh membership checks. A stored
session never grants a role, paid plan, dataset consent or training permission.
Logout deletes the row, so other instances stop accepting it; already-authorized
in-flight requests may finish. Lookup/revocation failures return 503. Logout never
reports success or clears its cookie while durable revocation has failed. Invalid
or expired credentials cannot fall back to a cached process session.

## Configuration and operation

`[browser]` retains its HTTPS `origin` and `client_id`, and adds:

- `session_keys_file`: private JSON key-ring file supplied at launch through a
  mounted file or Vaultix. Selecting it requires PostgreSQL; unreadable/invalid
  configuration prevents startup. No secret is compiled in or stored in TOML.
- `session_capacity`: maximum established sessions per configured deployment,
  default 4096; valid range 1–1,000,000. Capacity and replacement are serialized
  transactionally across instances. Issuing a session purges expired rows.
- `session_ttl_seconds`: absolute maximum lifetime, default 3600; valid range
  1–86,400. The provider's reported token lifetime is a further upper bound.
  Requests do not renew the lifetime and no refresh token is retained.

The private key file has `active` (a key ID) and `keys` (a map of IDs to 32-byte
hexadecimal keys). IDs contain only ASCII letters, digits, `_` and `-`. At most
three keys and 16 KiB are accepted. Use mode 0600 on Unix and a private service
account ACL on Windows; the Unix mode check is enforced. Do not commit the file
or include its contents in diagnostics or database backups.

For rotation, first distribute a ring containing both keys to all instances, then
select the new active key. Keep the previous key for at least the longest issued
session lifetime after its last use. Removing a key invalidates its remaining
sessions. Keep the ring in the secrets recovery procedure; a database restore alone
must not recover authenticating credentials. Changed deployment identity creates a
separate session context.

Without `session_keys_file`, legacy process-memory sessions remain available and
a restart signs users out. Hosted acceptance must explicitly configure and verify
durable mode. This compatibility mode is not the native-account target.
`GET /auth/config` reports `session_storage` as `postgres`, `memory`, or null when
browser login is disabled. It exposes no key IDs, file paths or credentials.

## Current limits and proof obligations

OAuth state/PKCE attempts remain process-local and expire after ten minutes; a
restart during sign-in requires starting sign-in again, and callback routing must
reach the instance that began that attempt. Established sessions can be shared.
Durable login attempts, native passwords, email verification/reset, native device
flow, explicit account linking and public-signup security review remain slice-2 work.

Tests use isolated PostgreSQL schemas and fake federation identities. They must
prove reconstruction with independent instances, cross-instance logout, expiry,
capacity races/rollback, rotation, deployment/ciphertext/expiry tamper rejection,
CSRF, bearer precedence and storage-outage behavior. These tests are not evidence
of Texas activation or two real production accounts.

Migration 0015 adds one table and indexes without rewriting monitoring records.
The managed-release migration/backup gate still applies before activation. Do not
modify checksums of applied migrations to make a release pass.

Primary references observed 2026-11-02: [OWASP session management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
and the locked aws-lc-rs 1.18.2 source (`aead/rand_nonce.rs`).
