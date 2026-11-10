# Tenancy v1: organizations, projects and project keys

Status: implemented in development source and tested against real Postgres. Not deployed. Native
accounts, the member console and turnkey onboarding are later slices of
[platform v1](../plan/platform-v1.md); until they land, this is reached through its HTTP API with a
Nomen sign-in. Accepted direction: Shippin ADR 0013.

## Model

```text
Organization ── members (user + role)
     └── Project ── project keys (write-only)
              └── turns, events, signals, reviews, datasets, source health
```

A **project is a devspace**: the partition every monitoring table already keys on, under its product
name. No monitoring record moves, and existing devspace ids do not change, because collectors bind
their queues to them. Do not confuse this with the `project` field on a turn record, which is the
source project an agent was working in. One RROFLOW project can hold turns from several of those.

A **personal organization** is created at first sign-in, with a default project whose id is still the
account ID. It always has exactly one member, its creator, and refuses invitations and membership
changes. So the project a sign-in opens by default can never be taken away from that person. To work
with others, create a team organization. Moving a project between organizations is not implemented.

Devspaces registered by an operator's `[[devspace]]` key stay outside any organization until claimed
with `rroflow-ingest claim`, which attaches them to the account's personal organization. A project
created inside an organization cannot be claimed.

## Choosing a project

A signed-in request applies to the account's default project unless it names another:

```http
X-RROFLOW-Project: p_0123...
```

Membership is checked on every request, so existing clients that send no header behave as before. A
project that does not exist, and one the caller cannot see, get the same `404`. A malformed or
repeated header gets that answer too. A key opens only its own project: naming another is refused,
not ignored. A well-formed request with no credential is always `401`.

## Roles

Each role includes the ones before it.

| Role | Can |
|---|---|
| Viewer | Read the organization's projects and see its members |
| Member | Also send records, change signal settings and save reviews |
| Admin | Also create projects, manage keys and invitations, manage members below owner, read the audit trail, and manage dataset consent and exports |
| Owner | Also grant and remove owners |

Nobody manages someone above them or grants a role above their own. Anyone may leave. An organization
always keeps an owner, including when two owners step down at once. Changes apply on the person's next
request, whether or not their sign-in is still valid. Dataset consent needs an admin because it speaks
for everyone whose work is in the project.

Removing someone, or taking them below admin, revokes the invitations they created, so nobody can keep
a token and use it to come back. Their keys are not revoked: a key belongs to the project, and someone
leaving should not stop a running delivery. Removal reports `active_keys_created` so whoever removed
them can decide what to rotate. Someone who still holds a key can keep delivering until it is revoked.

## Project keys

`rrop_` followed by 64 hex characters, shown once, stored only as a SHA-256 digest, revoked on the next
request with no cache to wait out. A key is **write-only**. It can:

- deliver: `POST /v1/turns`, `POST /v1/events`, `POST /v1/sources/health`;
- identify itself: `GET /v1/whoami`;
- read built-in signal settings: `GET /v1/signals/settings`, without custom rules, which run on the
  server and can describe private work.

Every other route refuses it. A project key holds only `project.send`, and every route declares the
permission it needs ([permissions v1](permissions-v1.md)), so a new route refuses write-only keys
unless it declares `project.send`. An unrecognized `rrop_` key is refused without being sent to the
identity provider.

**Treat a key as a secret.** Do not commit one. A key held in CI or a server environment is the intended
use. A leaked key cannot read records back, but until it is revoked it can:

- replace turns or source reports whose current revision that key wrote;
- learn whether an `event_id` exists, from delivery responses, and see the organization's usage in a
  quota refusal;
- spend the organization's pooled allowance, which stops delivery for **all** its projects until the
  next period or an operator's change.

[Receipt provenance](provenance-v1.md), in development, limits a key to revisions it wrote
in PostgreSQL. Identical replay does not transfer ownership. Replacing an old key therefore
does not grant permission to change that key's existing records. Per-key rate limits and
the console provenance view remain unimplemented parts of
[slice 3](../plan/platform-v1.md#3-provenance-contract). Do not publish project keys.

Operator-registered `[[devspace]]` keys are unchanged. They still read and write, and they are still
not people, so they cannot manage dataset consent or tenancy.

## HTTP

Everything returns JSON. Bodies reject unknown fields. All management routes need a signed-in person.

| Route | Who | Does |
|---|---|---|
| `GET /v1/me` | Anyone signed in | Account, organizations with roles, projects, default project |
| `POST /v1/organizations` `{name}` | Anyone signed in, up to the limit | Create a team organization, with the creator as owner |
| `POST /v1/organizations/{id}/projects` `{name}` | Admin | Create a project |
| `GET /v1/organizations/{id}/members` | Any member | List members |
| `PUT /v1/organizations/{id}/members/{user}` `{role}` | Admin | Change a role, within the rules above |
| `DELETE /v1/organizations/{id}/members/{user}` | Admin, or that person | Remove, or leave |
| `POST /v1/organizations/{id}/invitations` `{role}` | Admin | Create an invitation; returns its token once |
| `GET /v1/organizations/{id}/invitations` | Admin | List pending invitations |
| `DELETE /v1/organizations/{id}/invitations/{id}` | Admin | Revoke |
| `POST /v1/invitations/accept` `{token}` | Anyone signed in | Join |
| `GET /v1/organizations/{id}/audit?limit&before` | Admin | Newest first, paged by `next_before` |
| `GET /v1/projects/{id}/keys` | Admin | List keys, without the keys themselves |
| `POST /v1/projects/{id}/keys` `{label}` | Admin | Create a key; returns it once |
| `DELETE /v1/projects/{id}/keys/{id}` | Admin | Revoke; repeating it succeeds |

Outside an organization every one of these answers `404`, identical to an identifier that does not
exist. Refused for your role is `403`. A rule or limit is `409` with a plain message.

An invitation is a bearer link: whoever holds the token can join, for 7 days, once. It never grants
owner; an owner promotes a member after they join. Unknown, used, revoked and expired tokens look the
same. An existing member accepting one does not use it up. There is no email delivery yet, and no
binding to an address; the inviter passes the token on. So a removed person who was also handed someone
else's token could still use it: revoke pending invitations when that matters.

## Allowances

Allowances pool across an organization's projects, so creating a project does not buy more of the
default allowance ([allowances](account-allowances-v1.md)). Reservation holds the organization's lock,
so concurrent delivery to different projects cannot overspend. Replay stays free.

Operator grants are the exception until they move to the organization. A grant sets one project's limit
against pooled usage, and a new project gets the default. So a grant can raise a limit but cannot
reliably lower one: with a default of 1000 and a grant of 100 on an exhausted project, an admin can
create another project and keep delivering up to 1000.

Each person's personal organization has its own allowance, and they can create up to
`max_organizations_per_user` team organizations with their own. That is a real, bounded way to hold
several default allowances. Close it, if needed, when the Shippin entitlement contract lands.

## Limits

```toml
[tenancy]
max_organizations_per_user = 3       # team organizations; the personal one is not counted
max_projects_per_organization = 10   # the default project counts
```

These bound abuse; they are not plan allocations, and the defaults are not a pricing decision. Also
fixed: 50 active keys per project, 100 pending invitations per organization.

## Audit

Every organization, project, member, invitation and key change made through these routes is recorded
in the same transaction as the change: actor, action, target, time. A null actor is an operator action.
Records hold no keys, tokens or monitoring content. The service only ever inserts them, but nothing
stops someone with database access from changing them, so this is an accountability record and not
tamper evidence. Monitoring activity itself, including reads, is not audited.

`rroflow-ingest claim` can replace an account's empty default project. It refuses while that project
has active keys, and records what it replaced in `project.claimed`. The signal settings of a replaced
project go with it.

## Migration

`0013_organizations_and_projects.sql` gives every account-owned devspace a user, identity, personal
organization and owner membership, and moves no record. Applying it is one-way: older releases do not
know about organizations and would create accounts the new constraint rejects, so recover by restoring
a backup, not by starting an older release.

**Take and verify a backup before starting this release anywhere with real data.** `activate-release.sh`
refuses any release whose migration fingerprint differs from the running one, so on a host with a
previous release it needs the separately reviewed migration and restore procedure in
[hosted releases](../plan/hosted-releases.md). That check is the only guard. Every `rroflow-ingest`
command that opens the store applies pending migrations by itself, and the old `deploy.sh` installs and
restarts with no backup or fingerprint check.

## Not in this slice

- First-party accounts, durable browser sessions and email. Sign-in is still Nomen only.
- Any screen. The existing dashboard does not offer an organization or project switcher.
- Routing a collector's sessions to a chosen project; a collector uses its credential's project.
- Project, organization and account deletion, and data-retention controls.
- Renaming organizations or projects.
- SSO, SCIM and per-project roles. A role applies to the whole organization.
- Paid-plan entitlements, which remain operator-set.
- Hiding Nomen account IDs: saved reviews record them, and anyone who can read the project can see them.
  An account ID is also that person's default project id, though knowing it grants no access. Slice 2
  moves reviews to RROFLOW user ids, since native accounts have no Nomen ID.
