# Release manifest v1

Status: implemented. The first release to carry a manifest is the next one published.

Every release publishes `rroflow-release.json`, schema `rroflow.release/1`, with a `.sha256` file
beside it and a GitHub artifact attestation. Installers, the host updater and people read this one
file to learn what a release is. They don't have to guess from asset names.

## Fields

| Field | Meaning |
|---|---|
| `schema` | `rroflow.release/1` |
| `product`, `repository` | `rroflow` and the GitHub repository that built it |
| `version`, `tag`, `commit` | The package version, its tag, and the exact commit the tag identifies |
| `prerelease` | True when the version has a suffix such as `-rc.1` |
| `server` | The CI-qualified server archive: `target`, `archive` name, `sha256`, and `migrations`, the fingerprint of the complete ordered database migrations |
| `upgrades_from` | Versions this release is tested to upgrade from. Empty means fresh install only. |
| `artifacts` | Every other asset in the release, sorted by name, each with `name`, `kind`, `size` and `sha256` |
| `qualification` | The CI run that qualified the commit, the jobs it had to pass, and the checks people recorded |
| `verification` | How to verify an asset's attestation with the GitHub CLI |

### Artifact kinds

| Kind | What it is |
|---|---|
| `server` | The server archive tested in CI. It is the only server that installers and the updater use. |
| `server-build` | Server builds made by the packaging tool for other platforms. These are not installed. |
| `collector`, `collector-installer` | The collector for each platform, and its shell and PowerShell installers |
| `checksum`, `provenance`, `source`, `metadata` | Checksum files, the CI source receipt, the source archive, and the packaging tool's own manifest |

An asset with no recognized kind stops the release. So does a release with no server or no
collector.

## Guarantees

- **Deterministic:** the same assets always produce the same bytes. There are no timestamps.
- **Written once:** it is written after the qualified server archive is promoted and before the
  GitHub release is created. It is never overwritten.
- **Read directly:** the migration fingerprint is read from the server archive itself.
- **Attested:** the manifest has its own GitHub attestation, so a copy can be checked against the
  workflow that built it.

## How the updater uses it

1. Fetch the manifest, check its `.sha256`, and verify its attestation.
2. Download `server.archive` and check it against `server.sha256`.
3. Compare `server.migrations` with the running instance. A change means a database migration, so
   the updater backs up first and restores automatically if anything fails.
4. Refuse when the running version is newer than the release, or when a migration change comes from
   a version not in `upgrades_from`.

## Later additions

Container image references and the binary-with-systemd bundle are added when that packaging exists.
New fields are optional: readers ignore fields they don't know, and fields defined here never change
meaning within v1.
