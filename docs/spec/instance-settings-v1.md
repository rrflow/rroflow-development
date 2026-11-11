# Instance settings v1

Status: implemented in development source for the first setting, not deployed.

Owner direction, 2026-11-11: every setting and configuration is driven by the UI, with nothing
hardcoded. Sign-in stays simple, because people building with AI will not adopt anything else.

## How it works

`GROUPS` in `crates/rroflow-ingest/src/settings.rs` is the one list of settings. Each has a label,
help text, range, default and presets, some marked as recommended. `GET /v1/instance/settings`
returns that list with current values, and the screen at `/app/settings` is built entirely from the
response. A setting added to the list appears in the console with no console change.

Changes need the `instance.settings` permission, which instance admins hold. Saving uses a revision
check, so two admins cannot overwrite each other, and each change is written to the audit log with
its old and new values. Secrets do not belong in this store.

## Sign-in

| Setting | Range | Default | Recommended |
|---|---|---|---|
| Minimum password length | 8 to 18 | 8 | 15 to 18 |

The minimum applies when someone sets a new password. Signing in never applies it, so raising it
cannot lock anyone out. Passwords stop at 128 characters and 512 bytes in every case, which bounds
the hashing work and is not a preference.

The first-run flow:

1. The install prints a one-time setup password. An AI may paste it, because it is exposed by design
   and stops working once used.
2. The person chooses their own password on the same screen, or asks for one to be generated.
3. A chosen password goes straight to the workspace. A generated one is shown once, to be saved.

## What moves here next

| Group | Settings still in the file today |
|---|---|
| Sign-in | Public signup on or off, session length, sessions per person, link lifetime, attempt limits |
| Email | SMTP server and sender, with the password held encrypted |
| Federation | Nomen, Google sign-in |
| Collectors | Device approval lifetimes and limits |
| Limits | Organizations per person, projects per organization, batch and body size, allowance policy |
| Datasets | Collection limits and retention |
| SDK activity | Activity limits |
| Branding | Name, mark, colors, links |

These stay in the file because the service needs them before it can read this store: listen
address, database address and connection count, data directory, public origin, encryption key file,
and operator keys.
