# Receipt provenance v1

Status: in development. This is the first part of platform slice 3; it is not
provider verification, a public-DSN permission, or completed hosted acceptance.

The hosted server stores `rroflow.provenance/v1` receipts separately from submitted
turns, capture events and source reports, in the same transaction. A receipt names
the receiving organization/project, authenticated credential kind and opaque member
or project-key ID, server receipt time and embedded build revision. Operator keys
have no public credential ID; their secret or digest is never exposed. Organization
is read from storage, not from a client's identity-provider organization claim.
These are service assertions, not cryptographic signatures or protection against a
database administrator changing records. A development build reports `development`.

Collector fields are explicitly **claimed**. Missing capture method, machine,
repository or SDK metadata remains unknown; the endpoint is not evidence of which
provider really performed an action. Content is not copied into receipts. A digest
binds a receipt to the stored record, including server-derived signal results.

Existing records have no receipt. An identical replay preserves that unknown state
or the existing receipt; it never fabricates a sender, refreshes receipt time, or
transfers ownership. A changed revision receives a new receipt. The current revision's
credential owns replacement: a project key can change only a revision that it wrote.
Members and operator keys retain their authorized write access. A replacement key
can replay identical data but cannot change the old key's records. No implicit
ownership transfer is provided in this increment. An uncredentialed internal write
invalidates the previous receipt when it changes the record.

Record writes and ownership decisions hold the existing project lock. A mixed batch
that includes an unauthorized replacement rolls back entirely, including allowance
reservations and receipts. Immutable capture-event conflicts remain immutable.

Member reads use `/v1/provenance?kind=turn|event|source&record_id=...`; missing and
cross-project records both return 404. Write-only keys cannot read receipts or coverage.
`/v1/provenance/coverage` counts recorded and unknown receipts for each record kind.
Receipt reads require PostgreSQL in this increment; local JSONL receipt persistence,
the console coverage view, explicit additional collector claims and per-key rate
limits remain required work for the full slice. They must not be described as built.

Clients cannot submit `attested` or `provenance` on a record or batch. Those reserved
fields are rejected, not treated as proof. Existing content payloads remain untrusted
content even if they contain similarly named text.

Acceptance: cross-key replacement refusal, same-key updates, member replacement,
identical replay without ownership/time changes, legacy unknown, batch rollback,
forged attestation refusal, record/receipt transaction rollback, tenant isolation,
and coverage on a real PostgreSQL database. Existing collector and browser tests
remain required. Migration 0014 requires the same backup/restore gate as 0013.
