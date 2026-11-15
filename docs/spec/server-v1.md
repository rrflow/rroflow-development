# Server v1

Status: implemented. Moved out of `lib.rs` in refactor R1 (self-host release 1, TASK-004) with no
change in behavior.

The plumbing every feature shares: the service configuration, the state each handler receives, the
caller a credential proves, and the answer a refused request gets. It lives in
`crates/rroflow-ingest/src/server/`, and the crate root re-exports it, so `crate::AppState`,
`crate::Caller`, `crate::ApiError` and `crate::Config` stay the paths every feature uses.

| File | Holds |
|---|---|
| `server/config.rs` | `Config` (the TOML file), `NomenConfig`, `Devspace`, `hash_key`, `valid_devspace_id` |
| `server/state.rs` | `AppState` and `AppState::new`, which checks the configuration before serving |
| `server/caller.rs` | `Caller`, `AppState::authenticate` and `AppState::allowed` |
| `server/error.rs` | `ApiError` and the response each variant gets |
| `server/routes.rs` | `GET /v1/whoami` |

## Configuration

The file holds only what the service needs before it can read its database: the listen address,
the database URL, limits, operator keys, sign-in providers and branding. Everything a person changes
while the service runs is a setting in the console (`instance-settings-v1.md`).
`RROFLOW_DATABASE_URL` overrides `database_url`, so a password never has to sit in the file.

## Who is calling

`authenticate` reads `Authorization: Bearer <credential>` and tries, in order:

1. An operator key from `[[devspace]]`, matched by its SHA-256. It opens its one devspace.
2. A project key (`rrop_`). It opens its one project and can only send.
3. A native session or device token. Membership is re-read on every request.
4. A Nomen access token, when Nomen sign-in is configured.

A request may name a project with `x-rroflow-project`. A key that opens one project refuses any other
name. A member's name is checked against membership. A malformed or repeated header gets the same
answer as a project that does not exist.

`allowed` is `authenticate` followed by the permission check its route declares in
`permissions::ROUTES` (`permissions-v1.md`).

## Routes

| Method | Path | Access | Returns |
|---|---|---|---|
| GET | `/v1/whoami` | The caller's own | The `Caller` the credential proves |

## Errors

| Variant | Status | Meaning |
|---|---|---|
| `Unauthorized` | 401 | No credential, or one nobody issued |
| `MemberRequired`, `Forbidden` | 403 | Signed in, but not allowed this |
| `Missing`, `NotFound` | 404 | No such thing, or one this caller cannot see |
| `Conflict`, `SettingsConflict`, `SourceConflict`, `DatasetPolicyConflict`, `Refused` | 409 | Allowed, but stale or stopped by a rule |
| `BadRequest` | 400 | The request is malformed. The message says how |
| `Quota` | 429 | The workspace's allowance is used up. `Retry-After: 60` |
| `IssuerUnavailable`, `AccountUnavailable`, `AllowanceUnavailable` | 503 | A dependency could not be reached. Retry |
| `Internal` | 500 | A store failure, logged without record content |
