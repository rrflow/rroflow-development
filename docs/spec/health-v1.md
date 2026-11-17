# Health v1

Status: implemented. Moved out of `lib.rs` in refactor R1 (self-host release 1, TASK-004) with no
change in behavior.

What a load balancer, an installer or the host updater asks before trusting an instance. Code is in
`crates/rroflow-ingest/src/health.rs` and `health/`.

## Routes

All three are public and read nothing about any member.

| Method | Path | Answers |
|---|---|---|
| GET | `/healthz` | `200 ok` while the process serves requests |
| GET | `/readyz` | `200 ready` when the store answers within 2 seconds, else `503 store unavailable` |
| GET | `/build-info` | `{revision, target, migrations}`, embedded when the binary was built |

`migrations` is the SHA-256 fingerprint of the migration set the binary carries. The release
manifest records it (`release-manifest-v1.md`), and the host updater compares it after an update
(`host-updater-v1.md`).
