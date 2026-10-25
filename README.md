# rroflow-development

The RROFLOW lab: tools that feed real agent runs into evaluation, so RRFlow's learning layer is built and measured on real traffic.

Engine code lives in [rrflow/rrflow-development](https://github.com/rrflow/rrflow-development). This repository holds the outward pieces: instrumentation, the eval harness, research material and the rroflow.ai site. It moves to `rrflow/rroflow` at alpha.

| Path | What it is | State |
|---|---|---|
| [`crates/rroflow-raindrop`](crates/rroflow-raindrop) | Sends Claude Code sessions to Raindrop as events plus OTLP spans, with secrets redacted | Working; verified against Raindrop on 2026-10-24 |

Build: `cargo build --release`. CI runs format, clippy, tests and a fixture parse on every push and pull request.
