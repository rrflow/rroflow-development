# Host updater v1

Status: design for review. Not built. Self-host release 1, TASK-008, TASK-009 and TASK-010.

An instance admin presses Update in the console. From then on the instance either runs the new
release, or is back on the old release with its data exactly as it was, and either way a receipt
says what happened. The same updater runs on every self-hosted VM and on rroflow.ai.

## Pieces and privileges

| Piece | Runs as | May do |
|---|---|---|
| `rroflow-ingest` (the service) | `rroflow` | Check for releases, show them, write an update or backup request, read status and receipts. It never runs privileged commands. |
| `rroflow-updater` | root, started by systemd | Act on a request: verify, back up, swap, migrate, check health, restore. |
| `rroflow-updater.path` | systemd | Starts the updater when a request file appears. |
| `rroflow-updater-recover.service` | root, at boot, before the service | Finish or undo an update that a crash or power loss interrupted. |

**What the service may send:** only a version and a request id. The updater fetches and verifies
everything else itself, so a compromised service can at most ask for a published, verified release.

**Where files live:**
- `/var/lib/rroflow/updates/`: the service writes requests here. The updater writes `status.json`
  and receipts here, readable by the service.
- `/etc/rroflow/updater.toml`: root-owned. It holds only what the updater needs before any database
  exists: the release repository, the install mode, paths, the database name and the service names.

**Where the updater itself comes from:** it ships inside the verified server archive. A new
updater is installed only after the service update it came with has succeeded.

## Settings in the console

The following come from the settings registry, under an Updates group:

| Setting | Default |
|---|---|
| Release channel | Stable (prereleases off) |
| How often to check | Daily |
| Apply updates | Notify only; a person presses Update |
| Backups kept | 10 |
| Days to keep a failed database for diagnosis | 7 |

The service performs the check itself, over HTTPS to the GitHub release API, so check frequency
needs no root access.

## Permissions

- `instance.update`: request an update.
- `instance.backup`: create, download or restore a backup.

Both are held by instance admins, and every request is written to the audit log.

## One update, step by step

Each step is written to `status.json` before it starts.

1. **Lock and recover:** take the update lock. If an earlier update left a pending receipt, recover
   it first (see Interruption).
2. **Manifest:** fetch `rroflow-release.json` for the requested tag, check its `.sha256`, and verify
   its attestation. Then refuse when:
   - the repository, version or tag disagree with the request;
   - the release is older than the running one;
   - the channel excludes a prerelease;
   - the migrations change and the running version is not in `upgrades_from`.
3. **Download:** fetch `server.archive` and check it against `server.sha256`. Archive membership,
   member checksums and the embedded commit and migration fingerprint must all agree, as in today's
   activation script.
4. **Preflight:**
   - free space covers the backup plus the release;
   - the database answers;
   - the service is the expected one.
5. **Back up:** take a database dump, an archive of the data directory and a copy of the
   configuration, written as one backup set with its own checksums. The dump must read back with
   `pg_restore --list` before the update continues.
6. **Stop:** stop the service and its workers.
7. **Swap:** point the current release at the new one. In binary mode that is a symlink. In Compose
   mode it is the image pinned by digest.
8. **Migrate:** run `rroflow-ingest migrate` as the service user.
9. **Start and check:** start the service and require:
   - `/readyz` answers;
   - `/build-info` reports the manifest's commit and migration fingerprint;
   - the console loads.
10. **Finish:** write the success receipt, apply the backup retention setting, then install the
    release's updater.

Any failure at steps 7 to 9 restores the instance:
1. Stop the service.
2. Restore the dump into a new database.
3. Rename the migrated database aside to `<name>_failed_<id>`.
4. Rename the restored database into place.
5. Swap back to the old release.
6. Start it and run the same checks against the old version.
7. Write a restored receipt naming the failed step.

Restoring into a fresh database means tables a migration created cannot survive the restore.

If the restore itself fails, the service stays stopped and the receipt says an operator is needed.
The backup set and both databases are left untouched.

## Database migrations

Today every command that opens the database applies pending migrations. That would let a plain
restart migrate with no backup. With the updater:

- `rroflow-ingest migrate` applies pending migrations and exits.
- `serve` and the workers refuse to start when migrations are pending, and name the updater.
- A fresh install runs `migrate` once as part of installation.

## Interruption

A pending receipt names the step reached. At boot, the recovery unit acts on it:
- **Stopped before the swap:** it clears the receipt.
- **Stopped at or after the swap:** it runs the restore path above.

The service does not start until recovery has finished.

## Install modes

| | Binary with systemd | Container with Compose |
|---|---|---|
| Release swap | `current` symlink under `/opt/rroflow` | Image digest pinned in the Compose `.env` |
| Migrate | `rroflow-ingest migrate` as `rroflow` | `docker compose run --rm rroflow migrate` |
| Database dump | `pg_dump` as `postgres` | `docker compose exec postgres pg_dump` |

The updater always runs on the host, never inside a container with the Docker socket.

## Backups page

- **List:** backup sets with their size, the version they were taken from, and the reason they were
  taken (before an update, or by request).
- **Back up now:** creates a backup set.
- **Download:** streams a backup set to holders of `instance.backup`. Each download is audited.
- **Restore:** asks the updater to restore a backup set through the same restore path. A backup
  from a newer version than the running release is refused.

## Attestation verification

The manifest's GitHub attestation is verified against these expectations:
- the Sigstore public-good trust root;
- the GitHub Actions OIDC issuer;
- the identity of this repository's `release.yml` running at the requested tag.

The first choice for doing this is the `sigstore` Rust crate. Implementation must prove it on a
real release. If it cannot verify GitHub's bundles, the alternative is recorded here before
anything else is used.

Until that proof exists, the updater still requires the manifest's own checksum and every archive
digest, and the receipt states that attestation was not verified.

## Tests required before release (gate `upgrade-restore`)

- **Install:** a fresh install in both modes.
- **Upgrade:** an upgrade with data from each `upgrades_from` version.
- **Forced failures:**
  - a migration failure, a readiness failure and a wrong build identity each restore the old
    release and its data byte-for-byte, compared by table checksums;
  - a power cut simulated after the swap is recovered at boot;
  - a refused manifest, digest or downgrade changes nothing.
- **Backups:** restoring a backup set to a working instance, and refusing a backup from a newer
  version.
- **Privilege:** the service user can neither run privileged commands nor make the updater fetch
  anything other than a published release.

## Replaces

`deploy/tx-infra-prod/rroflow-deploy` and `activate-release.sh` stop being the Texas path once
rroflow.ai is installed through this updater (TASK-011 and TASK-012). Their verification checks
move into the updater.
