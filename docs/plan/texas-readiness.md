# Texas application VM readiness

Status: in progress, 2026-10-26. This is the operational continuation of
[rroflow-ai-plan.md](rroflow-ai-plan.md), not a replacement product authority.

## Intended outcome

RROFLOW develops Reason Ready Objects and flow from captured activity: events,
evidence, corrections, dataset revisions, experiments and reversible improvements.
Users should connect the subscriptions they already have. Capture support,
subscription-powered execution, chat-history import and model training are
distinct capabilities and each gets a verified status.

The immediate deployment is the collector-facing API and dashboard in an
identified application VM. Postgres is the answered hosted-store choice (D-4);
Nomen account ownership is the answered identity choice (D-6). The local store
remains open. The older reconciliation's JSONL-only observations are historical.

## Placement and operational gates

1. Establish the trusted SSH route and confirm VM identity against the management
   inventory. A public address and a hostname in a script do not prove placement.
   Keep addresses, SSH credentials and runtime configuration out of this record.
2. Inventory OS, architecture, vCPU, available memory, disk/inodes, clock sync,
   existing listeners, service ownership and firewall. Observe load before sizing.
   Do not run the application installer on a Proxmox hypervisor.
3. Run the read-only guard from the same pinned source checkout as deploy.sh:
   `EXPECTED_HOST=<verified-host> REV=<full-commit> bash deploy/tx-infra-prod/preflight.sh`.
   The guard is a necessary check, not a production-readiness certificate.
4. Qualify the release with fmt, clippy, Rust tests and real Postgres tests.
   Build outside the serving workload, retain the previous executable and config,
   and pin an artifact checksum. The existing source-build installer is a bootstrap
   tool; upgrades still need a transactional release/rollback implementation.
5. Install loopback-only first. Inspect role ownership, migrations and filesystem
   permissions. Keep Postgres private. Test bad-token rejection and two different
   devspaces: reads, writes, settings and generated datasets must remain isolated.
6. Exercise Nomen login/refresh/logout and account-to-devspace mapping. Complete
   dashboard PKCE and collector onboarding before claiming user-ready sign-in.
   Never use provider account credentials as RROFLOW credentials.
7. Take a database backup and restore it into an isolated target; verify data and
   owner mapping. Document retention, deletion, encryption and recovery ownership.
   Rehearse rollback across the actual migration set before the next release.
8. Configure DNS/TLS only after service checks pass. PUBLIC_HOSTS is deliberately
   empty by default. Integrate with an existing proxy instead of overwriting it.
   Prove public health, authentication and rate limiting; liveness alone is not
   database readiness. Protect abuse-sensitive endpoints before opening intake.
9. Add health/readiness and backup-failure alerts, request/worker limits and disk
   alerts. Keep dataset and training workers out of the request process. No GPU
   purchase or model placement is implied; use measured work and bound workers.
10. Prove one real, consented Claude Code session and one Codex session through
    capture, replay, dashboard and deletion. Extend adapters against the same
    contract. Record a support claim only after its complete flow passes.

## Known work before public release

- Original installer could mutate a host before checking proxy conflicts, allowed
  public exposure by default, and had no application-VM identity guard. The new
  preflight runs before package/service/config changes and defaults to loopback.
- Source extraction replaces /opt/rroflow/src, builds run on the serving host,
  and binary/config replacement is not transactional. Replace this path with
  versioned releases and verified rollback before using it for production upgrades.
- /healthz is liveness; database readiness and public request rate limiting need
  verification/implementation.
- Browser PKCE, release installers, setup/capture hooks, per-provider linking and
  automatic dataset jobs are not established by the existing ingest deployment.
- Postgres storage and Nomen code are present; their successful operation on the
  Texas VM remains unverified.

## Evidence from this session

The independent source baseline is
`ea66dbadb077111b3ca49f35be20db3a4179ee24`.
Work is isolated on branch `codex/texas-readiness`.

The configured management route timed out. Direct SSH to the documented target
stopped at missing trusted host identity; it was not bypassed. Public checks
observed apex DNS, no HTTPS response and missing API/app DNS. Those observations
are point-in-time probes, not a complete host inventory. No remote mutation has
been performed. The verified VM SSH route is the current deployment dependency.

See [subscription support](subscription-support.md) and
[automatic dataset contract](../spec/automatic-datasets-v0.md).
