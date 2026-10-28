# Keep your collector running

Implementation: development branch, pending native CI verification and release.
This does not confirm installation on your computer or deployment to Texas.

RROFLOW reads the Claude Code and Codex session folders linked on this computer.
It saves captured turns locally, then retries delivery to your signed-in account.
The startup option keeps this work running after you close the terminal.
It does not collect conversations from provider websites or phone apps.

## Set up once

Use a collector build containing `sources service`. The published older binary
may not include it. Sign in with `rroflow login`, then link your session folders
with `rroflow sources link`. Stop an existing foreground `sources run` first.

Run `rroflow sources service install` as your normal user. This starts collection
now and registers it for future logins. If you linked a custom store, use
`rroflow sources service --store "ABSOLUTE STORE PATH" install` instead, and pass
the same store for each command below.

Installation requires the saved login. Temporary `RROFLOW_INGEST_KEY`,
`RROFLOW_INGEST_URL`, and `RROFLOW_REDACT` overrides are refused because they would
not reliably survive a new login. Persist supported settings or use foreground
collection when those overrides are needed. Credentials are read from the saved
account file, never copied into task definitions or command lines.

## Check, pause, or remove

| Command | What it does |
| --- | --- |
| `rroflow sources service status` | Shows whether startup is registered and its operating-system state. |
| `rroflow sources service stop` | Stops capture now; future login startup remains enabled. |
| `rroflow sources service start` | Starts the installed collector again. |
| `rroflow sources service remove` | Stops capture and removes automatic startup. Keeps captured data and your account. |

Open **Sources** in the dashboard to check the last report, delivery queue and
capture errors. A running task is not proof of successful delivery. Refresh the
dashboard to retrieve new reports. A missing or expired login pauses delivery;
local capture continues while the collector is running. Use `stop` to pause
capture too. Signing into a different account does not transfer the old queue.

## What survives closing a session

Windows uses a Task Scheduler task under your current user, with limited
privileges and no saved password. Closing the terminal or disconnecting RDP
leaves a logged-in user's collector running. Logging out stops this interactive
task; the next login starts it. This is not a machine-wide Windows service.

Linux uses a systemd user service. Keeping the user manager running after logout
or starting it at boot requires the host administrator's deliberate linger
configuration. Installation never enables linger or changes system services.
See [systemd user service behavior](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)
and [loginctl linger](https://www.freedesktop.org/software/systemd/man/latest/loginctl.html).
Windows settings follow [Task Scheduler settings](https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtasksettingsset?view=windowsserver2025-ps).

## Upgrade and recovery

Installation pins a private copy of the current collector by SHA-256. Replacing
the original download does not silently replace the running collector. To
upgrade, remove startup, run `install` with the newer collector, then check
Sources. Retained journal data is replayed with its existing identities.

Each worker launch checks the pinned binary and linked source binding. Changed
manager definitions are refused rather than overwritten. Restore an edited
definition before removing it deliberately. A fatal worker failure has bounded
automatic retries; normal network failures stay in the delivery retry loop.
macOS startup registration is not implemented.
