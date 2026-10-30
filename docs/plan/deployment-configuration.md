# Deployment configuration and customization

Status: implemented in the deployment-configuration increment, 2026-10-30.
This describes source behavior, not a deployed Texas release.

## Configure a collector once

The collector has no compiled hosted issuer, client ID, project audience or ingest
destination. Get these public settings from the operator of your deployment.
Copy [the example](../../deploy/collector-deployment.example.json), replace its
example values, then run:

```sh
rroflow setup --deployment /absolute/path/to/deployment.json
```

Setup signs in, asks before linking local agent sessions, and registers per-user
startup. `--no-service` links without startup. Text capture remains opt-in. Setup
checks an existing source queue against the signed-in account before reusing it.
Startup configuration is not proof of delivery: check Sources in the dashboard
and `rroflow sources status` for received reports and pending revisions.

To make the settings the default, save the file as `deployment.json` under
`RROFLOW_CONFIG_DIR`, or set `RROFLOW_DEPLOYMENT` to its path. Without
`RROFLOW_CONFIG_DIR`, Windows uses `%APPDATA%/rroflow`; other systems use
`$XDG_CONFIG_HOME/rroflow` or `~/.config/rroflow`.

| Public setting | Command option | Environment |
|---|---|---|
| Nomen issuer origin | `--issuer` | `RROFLOW_ISSUER` |
| CLI public client | `--client-id` | `RROFLOW_CLIENT_ID` |
| Project audience | `--project-id` | `RROFLOW_NOMEN_PROJECT` |
| Ingest origin | `--ingest-url` | `RROFLOW_INGEST_URL` |
| Deployment file | `--deployment` | `RROFLOW_DEPLOYMENT` |

For `login` and `setup`, precedence per setting is command option, environment,
selected file. Setup can fall back to the saved account when a field is absent;
login requires a complete configuration. An explicitly selected missing file,
unknown JSON field, empty setting or unsafe origin is an error. The default file
may be absent. URLs must be origins without credentials, paths, query strings or
fragments; HTTPS is required except for loopback development. Identity uses the
existing Nomen/Zitadel protocol adapter, not arbitrary OIDC provider discovery.

The public JSON must never contain tokens, private keys or provider credentials.
Sign-in saves the resolved deployment and tokens in the separate protected
`account.json`. The background collector uses its saved account reference, so it
does not depend on the interactive shell retaining environment variables.

## Multiple deployments and account changes

Use a separate absolute `RROFLOW_CONFIG_DIR` for each deployment. Source journals
also bind to the authenticated devspace. Changing an environment variable or
profile never migrates a queue or sends a saved token to a new ingest server.
Interactive sending checks the saved issuer, client, audience and destination
before even refreshing a token. Old accounts without an explicit project field
continue to send when no changed audience is requested.

To switch accounts within one deployment, use another config directory for the
new source collection. Existing queues remain bound to their original devspace;
automatic transfer of captured history is deliberately not an account switch.

For headless operation, provide `RROFLOW_INGEST_KEY` and an explicit
`RROFLOW_INGEST_URL` (or `ingest_url` in the selected file), then use
`rroflow sources link`. Keys do not inherit the destination of an unrelated saved
member account. Member-oriented `setup` refuses ambient headless keys.

## SDK and server

JavaScript `Capture` requires `endpoint` explicitly, alongside its spool and
devspace. The spool retains its destination during retries. SDK wrappers capture
only calls made through them; this does not claim visibility into subscription
mobile/browser history.

The ingest server's TOML already owns database/storage, listen address, request
limits, allowed Nomen audiences/clients and browser sign-in configuration.
`RROFLOW_DATABASE_URL` can supply the database connection separately. See
[browser sign-in](browser-sign-in.md) and [identity](identity-and-devspaces.md).
No hosted identity IDs are defaults in the collector or the legacy installer.
The legacy source-build installer now requires `BOOTSTRAP_DEVSPACE` and explicit
Nomen settings when an ingest key is installed. It reserves `selftest` for its
deployment probe and is not the production release mechanism. Its system paths,
service account and database layout remain fixed deployment conventions.

## Remaining customization work

Public/member identity, introductory copy, links and accent colors now have a
[validated brand configuration](../ux/brand-configuration.md). The same local
vector artwork follows the configured highlight, without a remote image load.
Full layout theming and tenant-managed branding are still separate work.
Installer layout and upstream release provenance remain deployment-specific. Hosted plans/quotas must be
server-enforced; editable UI text cannot supply subscription entitlements.

The product targets in [the capability map](capability-map.md), consented dataset
jobs and held-out learning/promotion remain required work. Changing deployment
configuration does not implement those features or prove live tenant isolation.

## Licensing status

Rust workspace and JavaScript package metadata currently declare Apache-2.0.
This increment changes neither those declarations nor existing grants. A root
license text and contribution/third-party notice review still need reconciliation.

The owner requested public customizable source with BSL terms. Business Source
License 1.1 is source-available until its specified change date, rather than an
OSI open-source license ([the license text](https://mariadb.com/bsl11/)). The
pending choice is an Apache-2.0 core with separately scoped BSL additions, or
BSL for future app versions. A BSL release needs its exact licensed work, licensor,
additional use grant, change date and change license recorded. No values or
restrictions are invented here. Historical Apache-licensed versions retain their
existing grants.
