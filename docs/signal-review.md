# Signal inbox and review journal

Status: implemented in the SDK capture work branch; local verification, not a Texas deployment.

The dashboard reads actual turn records and shows a signal inbox, evidence tool
references, rule revisions and capture identity. A rule match is a finding to
review, not a proven failure. Selecting a signal count filters the recent turns.
Historical activity does not establish that a collector is connected now.

`GET /v1/turns` includes a `revisions` map from event ID to the SHA-256 of the
server's serialized turn record. `POST /v1/reviews` accepts:

```json
{
  "review_id": "a9c3d761-299f-4677-b9fd-8b197823f660",
  "event_id": "the captured turn ID",
  "turn_digest": "the revision returned with that turn",
  "signal": "blocked_handoff",
  "rule_version": 1,
  "verdict": "not_match"
}
```

The authenticated devspace is authoritative. The server verifies the turn and
signal revision, then retains the exact snapshot with the verdict, receipt time
and reviewer identity. A devspace key is attributed as `devspace-key`, not falsely
as a named human. An identical request ID is replayable; a changed request under
the same ID conflicts. New reviews of stale evidence are rejected. A later
correction creates another review; it never overwrites the prior verdict.
The UI displays the latest verdict for the exact revision among the newest
1,000 reviews. The local store is single-process; hosted persistence is Postgres.

Reviews do not authorize dataset creation, training, rule activation or promotion.
Those remain governed by the existing learning plan and mechanism spec. Outcome
labels, review sampling and reviewer roles remain future increments.

## Delivery gaps that this increment does not close

- `setup` must discover and link existing agents, install capture safely and show
  real source health. SDK wrappers are a separate developer integration path.
- Native transcript ingest still requires network settings before sending.
  Its automatic durable offline replay is not implemented by the SDK outbox.
- Source identity, dispatch correlation, queue age/bytes, backpressure and
  automatic reconnect must be observable at both the collector and cloud.
- Shippin owns account and subscription truth. Free/paid allowances, retention
  and rate limits need a versioned entitlement seam and enforcement; no pricing
  or allowances have been invented here.
- Raw transcript token normalization needs validation. The dashboard withholds
  spend totals and labels raw usage unverified.
- Texas management identity, deployment, DNS/TLS and real account onboarding
  remain required hosted acceptance checks.

See `plan/sdk-install-and-linking.md`, `plan/rroflow-ai-plan.md` and
`spec/mechanism-spec-v0.md` for the product flow and governed learning roadmap.
