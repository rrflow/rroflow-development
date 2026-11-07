# RROFLOW local setup MCP — development

This local stdio MCP bridge connects to the **existing** installed collector. It
is not a published package or a hosted MCP endpoint. It starts native device
sign-in and returns an approval page and short code; the collector persists the
credential locally. After explicit consent it can link Claude Code and Codex
session folders (past and future) to the one project selected during device
approval and register collector startup. It does not route repositories to
separate projects. Scrubbed text
is opt-in; metrics and signal results are otherwise sent. Setup exit alone does
not establish delivery: use project connection details and verify a real event.

Requires Node 22+, this checkout's compatible collector, and a configured public
deployment profile. Native device sign-in must be enabled on the target service.
Install dependencies with `npm ci`, then configure your MCP client to launch
`node /absolute/path/to/mcp/setup/main.mjs`. Set `RROFLOW_MCP_COLLECTOR` to the
collector executable if `rroflow` is not on PATH. Do not place secrets in MCP
configuration, chat, URLs or repository files.

`rroflow_setup_start` starts sign-in; `rroflow_setup_status` reports its state;
`rroflow_sources_connect` requires explicit consent and invokes existing setup.
The process must remain running while waiting for device approval. A timeout or
failure is reported without logging private collector output. This bridge does
not install the collector itself, configure a remote install MCP, support Nomen
MCP authorization, instrument apps automatically, or prove live vendor capture.

Run `npm test` for MCP client/protocol and fixture collector tests. Real host,
native provider, startup manager and deployed service testing are still needed.
