# rroflow-ingest: the rroflow.ai ingest

Receives `rroflow.turn/v1` records from the `rroflow` collector and files them into the sender's **devspace**, the user's Shippin.cloud space.

- **The key decides the devspace.** A client cannot name another devspace. Keys are stored only as SHA-256 hashes in the config.
- **Idempotent.** Records upsert by `event_id`; a re-send replaces.
- **Private by default.** Content arrives only if the user opted in on the collector, and the service never logs record content.

```bash
rroflow-ingest keygen --devspace jesse     # prints the key once, plus the [[devspace]] config block
rroflow-ingest serve --config ingest.toml
```

```toml
listen = "127.0.0.1:8787"
data_dir = "/var/lib/rroflow"
max_batch = 500
max_body_bytes = 16777216

[[devspace]]
id = "jesse"                 # Shippin.cloud devspace id
key_sha256 = ["<hash printed by keygen>"]
```

| Endpoint | Purpose |
|---|---|
| `POST /v1/turns` `{"turns":[…]}` | upsert records into the key's devspace → `{devspace, accepted, new, replaced}` |
| `GET /v1/turns?limit=&signal=` | newest records in the devspace, optionally filtered by signal |
| `GET /v1/summary` | turns by agent and project, signal counts, tool calls and errors, tokens (unreported usage counted, never priced as zero), and the signal catalogue |
| `GET /healthz` | liveness |
| `GET /` | public product page, with configured member entry and explicit feature status |
| `GET /app` | member dashboard; account sign-in when configured, advanced devspace key fallback; private APIs still require authorization |
| `GET /auth/config` | public sign-in availability and canonical member origin |

**Store.** Append-only JSONL per devspace, indexed in memory, behind the `TurnStore` trait. This is interim until decision D-4 (Parquet + DataFusion, Turso, or RRFlow) picks the durable store.

**Verified 2026-10-26** with real sessions from this repo:
- 10 Claude Code and 8 Codex turns landed in devspace `jesse`.
- A re-send replaced rather than duplicated.
- A wrong key got 401.
- No prompt text or key reached the server without `--content`.

**Not yet:** deployment on Shippin.cloud with devspace ids from the Shippin IdP, TLS, and per-devspace rate limits.
