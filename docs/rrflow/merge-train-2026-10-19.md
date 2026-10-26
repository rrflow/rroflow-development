# RRFlow merge train, 2026-10-19/20

Status record for the merges Jesse approved on 2026-10-19 ("i gave you my approval … done right no exceptions no shortcuts", then "merge it"). Supersedes the matching open-PR lines in `claude/rrflow-build-order.md` (Status section, 2026-10-18).

## Landed on `rrflow/rrflow-development` main

Every PR was brought up to date with main by a merge commit (no force push), passed candidate CI 17/17 on that exact head, and was merged pinned to it. Since 2026-10-19 a `main` ruleset enforces this. It requires a PR, the `pipeline / ci-gate` check, a branch that is up to date with main, and resolved threads.

| PR | Merge commit | What |
|---|---|---|
| #17 | `9bcc6154` | H-06a: prompt flights governed inside `RrdEngine`; POAM-032 |
| #18 | `13ce3386` | H-06b: every prompt flight a graph rrflowQL traverses. Its commit-time constraint gap is **POAM-034**: it was first numbered POAM-033, but #26 landed a different POAM-033 first. |
| #19 | `553f6754` | H-06c: named task records that flights join. The catalogue has 41 HTTP operations after #26's `data-schema-read`. |
| #21 | `36e3bc78` | E-01a: rrflowQL traversal through native temporal adjacency. OpenAPI `97156c7cbc21414e918825ea3e777795ed007552e2c091d6a38f18911b951b11`. |
| #22 | `f948abb1` | rrflowDB capability plan, reconciled: prompt graph families are **H-09** and project links are **H-10**; SQL access is deferred to H-08 |
| #28 | `ccad7869` | B-04a: the real-server WebSocket test accepts the in-flight delivery that can precede `unsubscribed` |

Merged in parallel by other work on Jesse's account, not by this session: #20 (Connectome UI submodule at `connectome/`), #26 (rrflowDB local integration, native schema discovery, and the PostgreSQL compatibility contract as H-08), and #27 (Connectome turnkey workbench).

## Decisions and corrections this changed

- **PostgreSQL is H-08, owned by `docs/reference/protocol/postgresql.md` (from #26).** Its packages are H-08a to H-08h, and it puts the wire adapter inside `rrd-server`.
  - Decision 12 still stands: rrflowDB has a Postgres-wire face.
  - The build-order doc's wording, "an adapter crate (pgwire + DataFusion over rrflowDB)", is superseded.
  - #22's own SQL-face plan was removed when it merged. It had proposed a separate `rrflow-postgres` crate, `SET rrflow.valid_at` session settings, and repeatable-read-only transactions. The landed contract contradicts all three: it forbids an adapter-owned ambient snapshot and requires READ COMMITTED and REPEATABLE READ.
- **POA&M numbers on main.**
  - POAM-032 is prompt flights (#17).
  - POAM-033 is rejected native commits retaining an unabortable intent (#26).
  - POAM-034 is commit-time schema constraints loading the whole scope (#18).
  - Open PR #16 also claims POAM-032 (external knowledge providers), so its record must renumber to POAM-035 when it is updated.
- **A test race on main is fixed (#28).**
  - The race lived in `rust_client_negotiates_authenticates_queries_and_reads_audit`, in `rrd-client/tests/real_server.rs`.
  - It failed 27 of 60 runs under CPU contention on main's tree, and once in candidate CI on #19.
  - The server's socket loop can send a delivery poll before it reads an `unsubscribe`. Nothing can follow `unsubscribed`, because the subscription is removed before the reply.
  - With the fix it failed 0 of 150 contention runs. `docs/reference/protocol/subscriptions.md` now states the ordering.

## Still open after this train

- E-01 is still open: context graph expansion and memory-estate seat planning still materialize a snapshot (POAM-005).
- The rrflowDB record's open decisions block M2 and L1:
  - who may accept a link when one owner holds both projects;
  - whether prompt graph families can be defined at runtime or only bundled;
  - default retention per family.
- PR #16 conflicts with main: POAM renumbering, the endpoint catalogue, and the OpenAPI digest.
- PR #23 records the ruleset (POAM-027e).
- Devagotchi PR #2 was not part of this approval.

## Lessons for later sessions

- **Generated files versus hand-written files.** A generated file (SDK projections, the file plan, indexes) may take one side and then be regenerated. A hand-written file must never be resolved by taking one side whole, because that silently drops the other side's edits. Resolve it hunk by hunk.
  - This happened once with `rrd-contract/src/lib.rs`. Strict clippy caught it before the push.
- **Shared Cargo target directory across git worktrees.** Cargo's metadata hash for path dependencies does not include the path, so builds from another checkout can be reused stale. Touch `crates/**/*.rs` whenever builds switch checkouts.
- **Count conflicts.** When both sides add operations, set the endpoint and typed-method counts from the merged contract export, not from either side.
- **Rehearse and re-root.** Rehearse an integration against the head the next merge will produce, then create the real merge commit with that exact tested tree and the real main as its parent. Confirm the main tree equals the rehearsed tree first.
