# Permissions v1

Status: implemented in development source, not deployed. Shippin ADR 0016 sets the direction.

Access is decided by named permissions. A person's role in an organization is a fixed bundle of
them, and a key holds what it was issued for. Every route declares what it needs, and a route that
declares nothing is refused.

## Permissions

| Permission | Scope | Allows |
|---|---|---|
| `project.read` | Project | Read monitoring data: turns, events, summaries, signals, SDK activity, provenance, source health, allowances and dataset status |
| `project.send` | Project | Deliver records: turns, events and source health |
| `project.write` | Project | Change signal settings and reviews |
| `keys.manage` | Project | List, create and revoke project keys |
| `datasets.manage` | Project | Change dataset consent |
| `datasets.export` | Project | Download a published dataset |
| `organization.read` | Organization | See members |
| `projects.create` | Organization | Create projects |
| `members.invite` | Organization | Invite people, and see and revoke invitations |
| `members.manage` | Organization | Change roles and remove members |
| `audit.read` | Organization | Read the audit log |
| `instance.settings` | Instance | Change this instance's settings |

A project's scope comes from the request's key, its `{project}` path, or the `x-rroflow-project`
header checked against membership. An organization's scope comes from its `{organization}` path.

## Who holds what

| Holder | Permissions |
|---|---|
| Viewer | `project.read`, `organization.read` |
| Member | Viewer's, plus `project.send` and `project.write` |
| Admin | All of the above |
| Owner | All of the above |
| Operator key (`[[devspace]]` in the config) | `project.read`, `project.send`, `project.write` |
| Project key (`rrop_`) | `project.send` |

Owners and admins hold the same permissions. Rank still applies on top of them: nobody manages
someone above them or grants a role above their own, an invitation never grants owner, and the last
owner cannot leave. Anyone may leave an organization; removing someone else needs `members.manage`.

Dataset consent and export also need a signed-in person, never a key, because they speak for
everyone whose work is in the project.

`GET /v1/me` lists each organization's `permissions` for the signed-in person, so a console offers
only what will work.

## Declared routes

`ROUTES` in `crates/rroflow-ingest/src/permissions.rs` lists every method and path the service
answers, with one of:

- `Public`: pages, assets, health, build identity and the public signal catalogue;
- `SignIn`: a sign-in or device-approval step that checks the credential it is given;
- `Own`: any valid credential acting on itself, such as `GET /v1/me` or creating an organization;
- `Requires(permission)`, or `AnyOf` when a send-only collector key may read the built-in signal
  settings it applies.

A request to a path the table does not declare is refused with 403 and logged; a method the table
does not declare for a known path gets 405. A test reads every `.route(` in `router()` and fails if a
route is served without a declaration or declared without being served. Another test checks, over
HTTP against PostgreSQL, that each role is allowed or refused exactly as its permissions say.

Handlers make each decision with one function, `permissions::require`. Organization checks run
inside the change's transaction, after locking the organization, so a role change that commits first
is always seen.

## Audit

Role changes, removals, invitations, keys, projects and organizations are written to `audit_events`
in the transaction of the change. From migration 0019 the database refuses to update, delete or
truncate those rows, whoever asks.

## Not built yet

- Instance-wide permissions for staff, updates from the admin panel and impersonation. They arrive
  with those features. `instance.settings`, held by instance admins, is the first.
- Per-project role assignments and custom roles.
- A console view of what each role can do.
