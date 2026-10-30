# Account allowances v1

Status: implemented enforcement seam; Shippin subscription synchronization and
live billing are not connected. This does not establish Texas deployment.

The service owns account allowances in Postgres. A member can read only their
credential-bound status at `GET /v1/allowances`. There is no member write endpoint,
no client-selected tenant or plan, and no provider subscription inference.

## Operator configuration

Choose numerical allowances as a product decision. There are no built-in prices
or free/paid allocations. A default policy applies to every account without an
unexpired grant. Configure the default as the desired free offering, and supply
expiring grants for paid accounts after authorization by the subscription owner.
The current operator CLI is an internal control; it is not a billing webhook.

Install a JSON file with `schema: rroflow.allowance-policy/v1`, `revision: 1` and
`default: {plan, label, turns_per_month, events_per_month}`. `plan` is a bounded
identifier, `label` is plain text, and both limits are explicit nonnegative
integers. Zero stops new identities of that kind, while keeping accepted replay
and existing data readable. Subsequent edits use the next revision. Identical
retries of the current revision succeed; stale, conflicting or skipped revisions
fail.

```sh
rroflow-ingest allowance --config /etc/rroflow/ingest.toml --file policy.json
```

For one existing devspace, use a file with `schema: rroflow.allowance-grant/v1`,
`revision: 1`, `allowance: {plan, label, turns_per_month, events_per_month}` and
`expires_at` as an RFC3339 timestamp:

```sh
rroflow-ingest allowance --config /etc/rroflow/ingest.toml --devspace ACCOUNT --file grant.json
```

The command requires operator database access; it does not create unknown
accounts, change ownership, bill the user or grant dataset/training consent.
An expired grant falls back to the current default. Revoke early by saving the
next grant revision with an expiry at or before the current time. Upgrade,
downgrade, expiry and default-policy changes never reset the month's usage.
Grant revision and expiry remain visible in the authenticated status.

Hosted configuration should set `require_allowance_policy = true` at TOML root.
Install the policy before starting the service. Missing Postgres or policy fails
startup; later policy disappearance fails readiness and new ingest with 503.
Existing evidence remains readable. If this flag is false, an installed policy
is still enforced; absence is explicitly `unconfigured`, not a free plan.
JSONL reports `local_unmetered` and cannot enable required policy enforcement.
Nomen-only service startup no longer needs a dummy static devspace/key.

## Counting and delivery

Turn identities and SDK event identities have separate monthly allowances.
Counts are not tokens, provider cost, storage bytes or subscription charges.
The period is a UTC calendar month, based on the database receipt clock rather
than an uploader's timestamp. A devspace row lock serializes concurrent writers;
the quota ledger, records and signal projections commit in the same transaction.
A default-policy update cannot change an in-flight policy decision.

An accepted identity consumes one unit, including when multiple copies occur in
a batch. Replacement turns and identical event replay consume no further units,
including after a downgrade or in a later month. Event conflicts and failed
transactions roll back their quota reservations. The ledger is independent of
mutable turns. Pre-migration history is recorded as unmetered; new identities are
counted from this migration onward even before a policy is installed.

A batch that would exceed either allowance is rejected atomically with 429,
`error: allowance_exceeded`, resource/used/limit/requested/reset fields and
`Retry-After: 60`. The short retry permits an operator upgrade to take effect
without waiting until month end. Both durable collectors preserve pending work
and obey the existing retry cooldown across process restarts. There is no partial
acknowledgement or automatic batch splitting: a batch larger than the entire
configured allowance requires an allowance change or a smaller delivery batch.
Local disk capacity remains finite; this is not a lossless disk-full guarantee.

Settings shows allowance usage, reset and grant expiry, with distinct local,
unconfigured and unavailable states. Failure to load status never means zero
usage or an assumed paid plan. Sign-out clears displayed account counts.

## Rollout and remaining boundaries

Stop all older writers before migrating and enabling policy. Older binaries do
not maintain this ledger; mixed-version ingest and rollback to a pre-allowance
writer are unsupported after enforcement is enabled. Back up and rehearse restore
before production migration. Current updates retain the current document and
revision; a subscription-event audit journal and signed synchronization contract
still need implementation before automatic billing changes.

This increment does not implement prices, checkout, billing webhooks, rate/burst
limits, storage/retention quotas, invitations/organization roles, dataset jobs or
training permissions. Shippin membership remains the intended subscription
authority. The existing identity-resolution port is not an entitlement feed.
Purpose-specific dataset and training consent must remain separate.

## Evidence

`src/allowances/tests.rs` exercises PostgreSQL races, isolation, rollback, replay,
expiry, default changes, calendar boundaries and policy disappearance.
`scripts/test-account-allowances.mjs` runs the actual operator, server and collector
binaries against a disposable database, waits out real 429 cooldowns, and verifies
SDK/session readback after a policy upgrade. Browser tests use explicitly marked
allowance response fixtures plus real local-unmetered status; they do not prove
live member billing or deployment.
