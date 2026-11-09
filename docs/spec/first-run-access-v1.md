# First-run access v1

Status: the password stages are implemented in development source and not deployed.
The passkey and Google stages below are agreed direction and are not built.

Owner direction, 2026-11-09: someone building with AI must be able to install RROFLOW,
sign in and secure the account without knowing how any of it works. Access starts with a
password an AI may paste into chat, becomes a password only the person has, and then
becomes a passkey on each device. Google sign-in is an optional shortcut with a clear
agreement.

## The journey

| Stage | What happens | Who sees the secret | Status |
|---|---|---|---|
| Install | `rroflow-ingest first-run` prints a one-time setup password | The person and any AI running the install | Built |
| First sign-in | The setup password is consumed and a new password is generated | Only the browser that signed in | Built |
| Save | The person confirms they saved it; a page closed first can make another | Only that browser | Built |
| Passkey | Offered for this computer or phone, never required | Nobody; the private key stays on the device | Not built |
| Google | Optional sign-in after an explicit agreement | Google identifies the person to RROFLOW | Not built |

## Why the setup password may appear in chat

It is treated as already exposed, so exposure gives little. It is stored only as a digest,
works once, expires after 60 minutes, and only one can be live at a time. Using it creates
the built-in `admin` account with a personal organization and default project, and returns
a generated password to that browser alone, uncached. The command tells an AI agent that it
may show the setup password, must not sign in for the person, and must never ask for the
new password.

If someone else uses it first, the person's own attempt fails with "already used".
`first-run --reset` then issues a new setup password; using it replaces the admin password
and signs that account out everywhere, keeping its data.

## Operating it

```sh
rroflow-ingest first-run --config /etc/rroflow/ingest.toml          # first install
rroflow-ingest first-run --config /etc/rroflow/ingest.toml --reset  # lost admin password
```

Running it again before the password is used replaces the earlier one. After setup it
refuses without `--reset`. It needs database access, so only the operator can run it.

The minimum configuration needs no mail server:

```toml
[accounts]
origin = "http://localhost:8787"           # or your https:// address
encryption_keys_file = "/run/secrets/account-mail-keys.json"
```

The origin must be HTTPS or `http://localhost` with an optional port. Browsers treat
localhost as secure for cookies and passkeys. A bare IP address needs HTTPS, and cannot
use passkeys at all.

## Service contract

All are same-origin browser requests, carrying the `X-Rroflow-Request: dashboard` header.

| Request | Purpose |
|---|---|
| `GET /auth/native/config` | Adds `first_run` (a setup password is live) and `builtin_admin` |
| `POST /auth/native/first-run` `{password}` | Consumes the setup password; returns `{signin,password,revision,active_keys}` and a session |
| `POST /auth/native/password/status` | `{signin,unsaved,revision}` for this session |
| `POST /auth/native/password/saved` `{revision}` | Records that the person saved that password; refuses one since replaced |
| `POST /auth/native/password/replace` | Makes another password and returns `{signin,password,revision}` |

Only the browser session that received a generated password may replace it, and signing in
with that password ends the right, so a password someone relies on cannot be changed from
here. Replacing it also signs out every other session and ends device connections.

Responses containing a password use `Cache-Control: no-store`. The built-in account signs
in with the name `admin` on the normal sign-in form. It is recorded in `instance_admins`,
which grants nothing further yet. Audit events are `instance.first_run_claimed`,
`instance.admin_password_replaced`, `account.password_saved` and
`account.password_regenerated`, and never contain a secret.

## Known limits

- The service cannot tell a person from an agent. If an agent uses the setup password
  itself, it receives the new password. The command's instruction is the only control.
- `admin` has no email, so mail recovery does not apply. `--reset` is its recovery.
- `admin` is a known name, so repeated wrong passwords can block its password sign-in for
  ten minutes at a time, and repeating them keeps it blocked. `--reset` is not subject to
  that limit.
- `--reset` ends sessions and device connections but leaves project keys working, so
  collectors survive a lost password. The person is told how many remain and must revoke
  any they did not make. Invitations and other changes are not undone either.
- Plain `http://localhost` is proven in Chromium. Safari is not verified. Cookies there are
  not separated by port, so other local servers on the same machine receive them.
- SMTP, identity providers and other settings are still configuration files. Editing them
  in the app, after first sign-in, is the next piece of owner direction.

## Next: passkeys

After the password is saved, and again on later sign-ins, offer "sign in faster on this
device". It stays optional and the password remains the fallback. Require user verification
(fingerprint, face or device PIN) and store only the public key.

A passkey belongs to one domain. One made on `rroflow.ai` works only there, one made on
`localhost` works only on that machine, and changing a deployment's domain means enrolling
again. `first-run --reset` must also remove the admin's passkeys.

## Next: Google sign-in

Off by default. Each deployment supplies its own Google OAuth client, so no client secret
belongs in this repository. Request only `openid`, `email` and `profile`, which are
non-sensitive and avoid full Google app verification. Showing a name and logo needs Google
brand verification, and a privacy policy on the deployment's own domain.

Before leaving for Google, show one plain screen listing what is received: Google account
ID, email, name, picture, and the connection details any site sees (IP address, browser and
sign-in time). Owner decision, 2026-11-09: this data is used for sign-in and account security
only. Record the person's agreement with the version of the wording they accepted.

Google's user data policy requires disclosing all data accessed, used, stored or shared. It
forbids selling that data or using it for advertising. Using it beyond the disclosed purpose
needs fresh consent. A Google sign-in never links automatically to an existing account.
