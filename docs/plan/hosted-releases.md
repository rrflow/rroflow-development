# Hosted server artifacts and managed activation

Status: implemented for review on `codex/hosted-release`, 2026-10-27. This is a
continuation of [Texas readiness](texas-readiness.md). It does not establish a
verified Texas VM, a public deployment, production recovery or account activation.

## Build and qualification

The `hosted-artifact` CI job depends on the Linux/Postgres validation job. It checks
out the exact PR head (or push) commit, builds `rroflow-ingest` in release mode on
Ubuntu 22.04, and packages an x86_64 GNU/Linux server. This targets glibc 2.35 or
newer; architecture and runtime compatibility must be checked on the actual VM.
The collector's four-platform release matrix does not establish server support
on those four platforms.

The server embeds its commit, Cargo target and SHA-256 fingerprint of the complete
ordered migration files at build time. `rroflow-ingest build-info` and the read-only
`GET /build-info` endpoint expose that identity without consulting the host's Git
checkout or runtime environment. Unstamped local builds say `development` and
cannot be packaged by the release script.

The archive contains only `rroflow-ingest`, `revision`, `target`, `migrations`,
`build-info.json` and their `SHA256SUMS`. The adjacent `.sha256` pins the archive.
Packaging rejects a binary whose embedded commit/target differs from the expected
release. The CI smoke test extracts that exact archive into a temporary directory,
starts it against disposable JSONL fixture data, and checks build identity,
readiness, unauthenticated rejection, account mapping, ingest/readback, dashboard
assets, authenticated research and graceful SIGTERM. Postgres behavior is tested
in the prerequisite validation job, not by this JSONL artifact smoke.

The CI artifact is retained for 30 days. The publication path below also promotes
its exact verified bytes to the permanent GitHub Release, with the inner archive
checksum and a source/CI receipt. The receipt is provenance metadata, not a signature
or a server attestation. Select the reviewed commit and obtain its expected digest
from the trusted workflow/release result; an adjacent untrusted checksum alone does
not establish provenance. Promotion requires all applicable checks on that revision.

## Readiness versus liveness

`/healthz` remains process liveness. `/readyz` has a two-second deadline and checks
Postgres connectivity/table readability or the presence of the JSONL directory.
Failures return 503 without connection details. It does not certify free space,
write permissions, backup health or identity-provider availability. Activation
also performs authenticated `/v1/whoami` and `/v1/summary` checks against an expected
devspace, so liveness alone cannot make a rollout successful.

## Provisioning boundary

`deploy/tx-infra-prod/activate-release.sh` upgrades an already-provisioned dedicated
application VM. It never installs packages, creates a database or account, changes
credentials, rewrites runtime configuration, edits DNS/TLS or touches Gold. The old
`deploy.sh` source-build bootstrap is not the production upgrade path.

Before first activation, the operator must independently verify the SSH host and
VM inventory, architecture/runtime, capacity, firewall, Postgres ownership and a
backup restored to an isolated target. Provision root-owned non-writable release
directories `/opt/rroflow` and `/opt/rroflow/releases`, the `rroflow` service user,
external configuration at `/etc/rroflow/ingest.toml`, and the reviewed
`deploy/tx-infra-prod/rroflow-ingest.service` unit. The unit executes
`/opt/rroflow/current/rroflow-ingest` as `rroflow`; configuration and state remain
outside release directories. Do not start it until a release is installed.

The script requires a root-owned backup/restore receipt and a separate protected
smoke-account ingest key file. The presence of a receipt is an operational gate,
not machine proof that a restore succeeded. Keep those files on the host, outside
Git. Configure the expected smoke devspace explicitly. The key is passed to curl
through stdin, never a command-line argument or deployment receipt. Runtime
configuration must grant the service only the access it needs.

After those prerequisites and commit/checksum verification, the operator invokes:

```sh
EXPECTED_HOST=<verified-vm-name> \
RROFLOW_BACKUP_RECEIPT=<protected-restore-receipt-path> \
RROFLOW_SMOKE_KEY_FILE=<protected-smoke-key-path> \
RROFLOW_SMOKE_DEVSPACE=<expected-devspace> \
bash deploy/tx-infra-prod/activate-release.sh <archive> <trusted-sha256> <full-commit>
```

`LISTEN` defaults to loopback `127.0.0.1:8787` and must match the external config.
`RROFLOW_RELEASE_ROOT` and `RROFLOW_CONFIG` support explicitly provisioned layouts;
the systemd unit must use those exact paths. Credentials and live endpoint values
do not belong in this document.

## Activation, rollback and interruption

The read-only VM guard runs first and refuses a Proxmox host. The activator checks
root ownership, permissions and service execution identity, then takes a process
lock. It copies the archive into a protected staging directory and rechecks the
trusted digest, preventing a mutable download from changing between verification
and extraction. Archive membership, member digests and embedded binary identity
must agree. Existing release directories cannot be silently replaced.

Upgrades require the same migration fingerprint as the prior release. A migration
change is refused before activation and requires a separate reviewed migration
and restore procedure. Equal fingerprints are necessary for this rollback path;
they do not prove arbitrary future application/data changes backward compatible.
External config changes are a separate operation and are not rolled back here.

The activator records `activation.pending`, atomically swaps the `current` symlink,
restarts the managed service, and checks readiness, exact build identity and
authenticated reads. Success writes `last-deployment.receipt`. Failure restores
the prior symlink, restarts and verifies it, and writes `last-rollback.receipt`.
First-install failure stops the service and retains a pending receipt; it cannot
invent a prior release. Failed rollback also retains the pending receipt.

A kill/power failure can leave `activation.pending`. The next invocation refuses
to proceed. An operator must reconcile its recorded previous/candidate paths with
the actual symlink and running build, restore a verified prior release or stop the
service, and perform the same authenticated checks before archiving the pending
receipt. Automatic recovery after abrupt power loss is not implemented. Release
directories and database content are never garbage-collected by this script.

The Linux fixture suite exercises activation, upgrade, explicit rollback,
readiness/authentication-triggered rollback and refusal of checksum, migration,
locking, pending-receipt, unit, key-permission and immutable-release violations.
Systemd, HTTP and machine identity in that suite are fixtures. A real Texas
backup/restore and rollout rehearsal remains an acceptance requirement.


## Mainline publication gate

Implemented release pipeline. Version v0.4.0 was published from the qualified main
commit on 2026-10-31; its downloaded hosted assets were verified byte-for-byte against
the CI artifact. See the [release verification record](../releases/v0.4.0.md) for the
commit, workflow runs and digests. Texas activation remains unverified.

`release-on-version.yml` responds to a completed successful `ci` main-push run, or an
explicit retry on main. It checks out that exact commit. `scripts/release-gate.mjs`
requires the latest matching main-push CI run from this repository, successful jobs
listed in `.github/release-policy.json`, and its unexpired hosted artifact with a
GitHub-provided digest. A PR/fork run, skipped required job, failed/pending run, missing
artifact or API error cannot authorize publication. Existing released versions are a
no-op; an unreleased version reserves a lightweight tag at the qualified commit. Tags
are never moved. A conflicting tag fails before dispatch.

The release workflow is dispatched from that version tag, never the moving main branch.
Its publishing path rechecks tag/commit/package version and exact CI qualification before
installing dist or creating artifacts. Direct publishing dispatch from a branch is refused;
PR and dry-run builds remain available. Server builds receive the exact source stamp.
Concurrent runs of one tag serialize. A failed attempt can be retried against the unchanged
tag once its external failure is resolved; a published release is never overwritten.

Before creating the permanent release, the host job downloads the qualified CI artifact
by ID. It checks the metadata and zip digest, archive checksum, bounded zip/tar membership,
member checksums, embedded commit/target/migration identity, and refuses existing output
files. Signed storage redirects receive no GitHub token. It copies the original tar.gz
and checksum bytes, plus a public CI/source receipt, into the release assets alongside the
normal dist packages. There is no second server build in this promotion step. Validation
failure prevents the permanent release; publication is separate from Texas activation.

The generated dist workflow has intentional repository gates; `dist generate` changes
must preserve them. CI checks the wiring and exercises refusal/idempotency/identity cases,
artifact mutation and unsafe archive members. The promotion verifier first ran against
a PR36 CI archive without publishing it. The later v0.4.0 record proves mainline
publication and exact-asset promotion. Neither result proves deployment, production
backup or real member sign-in.

Primary references: [GitHub workflow-run triggers](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_run),
[workflow dispatch references](https://docs.github.com/en/rest/actions/workflows#create-a-workflow-dispatch-event),
and [dist dispatch configuration](https://axodotdev.github.io/cargo-dist/book/reference/config.html),
observed 2026-10-31.
