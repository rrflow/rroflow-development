# Automatic datasets v0

Status: proposed implementation contract, 2026-10-26.
Owner: RROFLOW dataset product; engine promotion continues to follow RRFlow's
learning contract. Extends R8 and R3-consent in
[mechanism-spec-v0.md](mechanism-spec-v0.md).

## Product behavior

A user enables a dataset policy once for a devspace and selected sources.
Subsequent capture can schedule dataset builds automatically within that grant.
Dataset creation does not silently authorize training or activation. A separate
bounded training policy may authorize recurring experiments. Successful experiments
produce candidates; promotion follows held-out evaluation and reversible admission.

Objects carry stable identity and immutable revision: source event, evidence,
human correction, label, dataset manifest, experiment and candidate. Repeated
capture must not mutate the meaning of an already published dataset revision.

## Build input and output

A build accepts an authenticated devspace, policy revision, consent revision,
source cutoff/read stamp, projection/redaction version and split policy.
Its idempotency key derives from those inputs. Client-supplied devspace claims
cannot override the authenticated owner. No cross-tenant pooling by default.

Every output manifest contains:

- schema and dataset ID/revision; owner and allowed sharing scope;
- exact source event IDs plus immutable source digests/revisions;
- content eligibility, purpose-specific consent and redaction/projection version;
- immutable labels with author/evaluator provenance; corrections reference both
  the prior and corrected artifact;
- included/excluded counts and machine-readable exclusion reasons;
- immutable train/validation/test assignments and their digests;
- row format, artifact digests, creation time and retention/deletion policy;
- builder version, code revision and input cutoff so the snapshot is reproducible.

Existing turns are upserted. A source reference alone is therefore insufficient:
the builder must materialize an immutable redacted snapshot or retain the exact
source revision before publishing a manifest.

## Job behavior

`queued -> running -> validating -> published`, with explicit failed/cancelled
states. A durable lease and idempotency key prevent duplicated jobs after restart.
Write artifacts into a private staging area; publish the manifest only after every
artifact checksum, scope, count and consent check passes. Recheck consent before
publication. A failure leaves no partly published dataset.

Keep builds off the HTTP request path. Apply per-devspace CPU/memory/storage,
concurrency and retry budgets. Persist progress and exclusion counts; record
failures without raw prompt content or credentials. Queue selection, scheduler
and artifact storage remain implementation decisions with owned interfaces.

## Dataset families

- **Supervised:** explicit MATCH/NOT MATCH and outcome labels; edited artifacts
  preserve the actual correction. Ambiguous/unlabelled rows stay out of the
  labelled training set.
- **Unsupervised:** eligible unlabelled examples for clustering and novelty
  detection. Cluster IDs are candidate groupings, never correctness labels.
- **Evaluation:** frozen, separate held-out revisions. Group related sessions,
  forks, near-duplicates and project artifacts before splitting to prevent leakage.
  Once consumed for selection, rotate the holdout according to policy.
- **Preference/correction:** pairs require a supported provenance relationship
  and human or declared evaluator preference; an aborted turn is not automatically
  a rejected example.

Provider-neutral canonical rows can have explicit exporters for a training target.
A consumer subscription does not itself establish a model fine-tuning interface.
Training a RROFLOW classifier, evaluating a prompt/rule, and fine-tuning an external
model are separate job types with distinct capability and budget checks.

## Revocation and deletion

Stopping capture, withdrawing dataset use and deleting records are separate
commands. Revocation blocks new builds and affected pending training. Deletion
invalidates affected manifests and propagates to retained artifacts/exports under
the accepted retention policy. Record downstream model provenance; do not promise
that deleting a row reverses an already trained model. Requalification, retirement
or retraining is an explicit downstream action.

## Acceptance evidence before activation

1. Two devspaces cannot read, build or export each other's records.
2. Replay/crash/retry yields one published revision with identical contents.
3. Missing consent, withdrawn consent, unredacted content and malformed sources
   are excluded or rejected with observable reasons.
4. Related sessions/forks and duplicate artifacts never cross train/test splits.
5. Unlabelled and cluster-generated rows cannot become supervised truth.
6. Row counts, labels, source revisions and artifact digests reproduce exactly.
7. Delete/revoke blocks future use and invalidates affected revisions.
8. A bounded training run references the manifest, code, parameters, budget and
   evaluation method; failed or inconclusive results cannot promote.
9. A passing candidate can be shadowed, admitted and reverted with journaled
   evidence. The existing ingest alone satisfies none of these acceptance gates.

This contract alone does not establish a built worker or training engine. The first executable collection increment is [metadata datasets v1](metadata-datasets-v1.md): scoped consent, immutable metrics/review artifacts, durable jobs, expiry and revocation. Training, text corpora, leakage qualification and promotion remain open under the full acceptance gates above.
