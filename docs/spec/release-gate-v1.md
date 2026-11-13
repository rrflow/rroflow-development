# Release gate v1

Status: implemented. No release carries the self-host gates yet. They apply from 0.5.0, the first
self-host release.

A release is published only when everything on one list has passed. That list is
`.github/release-policy.json`. `scripts/release-gate.mjs` reads it before it creates a tag and again
before it publishes. When anything is missing, the gate stops before it writes to GitHub.

## What the list holds

- **`required_jobs`:** CI jobs every release needs. They must pass on the exact main-push CI run
  for the release commit.
- **`gates`:** checks that apply from a given version. A release candidate such as `0.5.0-rc.1`
  needs the same checks as `0.5.0`. Each gate has:
  - an `id`;
  - the version it applies `from`;
  - a plain sentence saying what it `check`s;
  - a `kind`, which is either:
    - `ci_job`: the named job must pass in that same CI run;
    - `record`: a person did the check, and the release commit records it, as described below.

## Self-host release gates (from 0.5.0)

| Gate | Kind | Check |
|---|---|---|
| `structure` | CI job | Features follow the feature-module convention, and new files stay within the size budget. |
| `install-compose` | CI job | A fresh install from the container image and Compose file reaches first-run, sign-in and a delivered collector record. |
| `install-binary` | CI job | The same, from the binary bundle under systemd. |
| `upgrade-restore` | CI job | Upgrading from each `upgrades_from` version keeps the data. A forced migration failure restores automatically. A backup restores to a working instance. |
| `security-review` | Record | An adversarial review of upload, parsing, the updater and permissions, with each finding fixed behind a failing test. |
| `clean-vm-install` | Record | A person installed the release on a clean VM using only the published install guide. |
| `minimum-specs` | Record | Minimum VM specs are published from a load test of this release. |

The CI jobs these gates name are added by the tasks that build them. Until they exist, a 0.5.0
release cannot be published. That is intended.

## Recorded checks

A check done by a person is recorded in `release/checks/v<version>.json`, committed before the
release commit is tagged. It is reviewed like code, and it cannot change after the tag.

```json
{
  "schema": "rroflow.release-checks/1",
  "version": "0.5.0",
  "upgrades_from": ["0.4.0"],
  "checks": [
    {"id": "security-review", "evidence": "https://github.com/rrflow/rroflow-development/issues/123", "recorded_on": "2026-12-01"}
  ]
}
```

- **One entry per gate:** every `record` gate that applies has exactly one entry, with an https
  evidence link and a date. An unknown, repeated or missing check stops the release.
- **Upgrade paths:** `upgrades_from` lists the versions this release upgrades from. The
  `upgrade-restore` job tests each one, and the release manifest publishes the list. An empty list
  means only a fresh install is supported.

## What the gate does not prove

It proves that the listed checks ran and passed or were recorded for this exact commit. It does not
prove that the list is complete. Adding a check means adding a gate here, in the same change as the
job or procedure that performs it.
