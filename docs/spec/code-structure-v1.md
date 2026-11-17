# Code structure v1

Status: enforced by the `structure` CI job from this change on. Existing code is grandfathered by
a baseline that may only shrink.

Every feature is built the same way. Each one keeps its code together in one place, small enough to
review, and is described by its own spec. This is what lets the product grow without each change
spreading across large shared files.

## A feature

A feature is something a person can name: issues, alerts, settings, updates, signal schema. It
owns everything below.

### Server, `crates/rroflow-ingest/src/`

```text
<feature>.rs            the feature's public surface and `pub fn routes()`
<feature>/routes.rs     HTTP handlers and route registrations, each declared in the permission table
<feature>/model.rs      request, response and stored types
<feature>/store.rs      database reads and writes
<feature>/settings.rs   its entries in the settings registry, when it has settings
<feature>/tests.rs      its tests; split further by concern as they grow
```

The module root is `<feature>.rs` beside `<feature>/`, the style the crate already uses for
`accounts`, `tenancy` and `datasets`. `lib.rs` only assembles the features' routers.

### Console, `console/src/features/<feature>/`

The feature's screens, its API client, its styles and its tests all live together.

Code used by several features goes in `console/src/shared/`.

### Everything else

- **Spec:** each feature has `docs/spec/<feature>-v1.md`, with underscores in the module name
  written as hyphens. Its contract version goes in the file name.
- **Migrations:** these stay in `crates/rroflow-ingest/migrations/`, because the migration tool
  needs one ordered folder. Each one is named for the feature it serves, for example
  `0021_issues.sql`.
- **Settings:** a feature never hardcodes a preference. It declares the setting in the settings
  registry, which builds both the API and the screen (see `instance-settings-v1.md`).
- **Permissions:** a feature's routes are declared in the permission table, with what each one
  requires (see `permissions-v1.md`).

## What the check enforces

`scripts/check-structure.mjs` reads every tracked source file. It skips generated console and site
output.

| Rule | Applies to |
|---|---|
| A source file is at most 600 lines. Split by concern, not by line count. | All new files |
| A file over budget may not grow, and leaves the baseline once it is split under budget. | Grandfathered files |
| Route registrations live only in a feature's `routes.rs`. `lib.rs` may not gain routes. | Server, excluding tests |
| A new server feature module has its spec. | New top-level modules in the server |
| New console code lives in `features/<feature>/`, which has its spec, or in `shared/`. | Console |

## The baseline

`.github/structure-baseline.json` records the code that predates this convention:
- the files over budget, with their line counts as ceilings;
- route registrations outside a `routes.rs`;
- the existing server modules;
- the existing top-level console files.

In CI the check compares the baseline with the one on the base commit. Any entry that is added or
raised fails. The baseline can only shrink as old code is moved into features.

## Emptying the baseline

The refactor plan in self-host release 1 empties the baseline in six pull requests, each a move
with no change in behavior:

| Step | Moves | State |
|---|---|---|
| R1 | `lib.rs` and `tests.rs` into `server`, `health`, `turns`, `signals` and `site`, and every module's routes into its `routes.rs` | Done |
| R2 | The console: `main.tsx`, `signal-library.tsx`, `account.tsx` and `styles.css` | Next |
| R3 | `tenancy.rs` and its tests | Planned |
| R4 | `accounts/device.rs` and the accounts and datasets tests | Planned |
| R5 | The collector's `sources.rs`, `raw.rs`, `account.rs` and `doctor.rs` | Planned |
| R6 | `rroflow-core`'s `signals.rs` | Planned |

The original dashboard's `dashboard.js` is not split: it is retired when the member console
replaces it.

Shared test helpers live in `crates/rroflow-ingest/src/tests.rs`. A feature's tests start with
`use crate::tests::*;`.
