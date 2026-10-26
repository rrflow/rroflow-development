# Installing the rroflow SDK and linking subscriptions

Date: 2026-10-26. Status: plan. Goal (Jesse, 2026-10-26): install the rroflow SDK once; it finds the coding agents and subscriptions already on the machine and offers to link them to rroflow.ai for review. Devagotchi (devagotchi.com) later makes this effortless and grows into a "layer 2" for users on a free tier.

## What the machine already tells us (facts, checked 2026-10-26)

| Agent | Installed? | Signed in, and how | Plan | Turns to capture | Per-turn trigger |
|---|---|---|---|---|---|
| Claude Code | `claude` on PATH; `~/.claude/` | `~/.claude.json` → `oauthAccount {accountUuid, emailAddress, organizationUuid}` (no tokens in that file) | **Not recorded locally** in the install checked; known only from the user or from rate-limit events in transcripts | `~/.claude/projects/**/*.jsonl` (parser built) | `Stop` hook in `~/.claude/settings.json` |
| Codex | `codex` on PATH; `~/.codex/` | `codex login status` prints the mode (ChatGPT, API key, access token, Bedrock …) without exposing a token (`codex-rs/cli/src/login.rs`, openai/codex `970111f`, 2026-10-23) | `plan_type` in the `rate_limits` snapshot of rollout `token_count` events (`RateLimitSnapshot`, `codex-rs/protocol/src/protocol.rs`), with primary/secondary windows and credits | `~/.codex/sessions/**/rollout-*.jsonl[.zst]` (parser built) | `notify` program (`agent-turn-complete` payload with thread and turn id) or the hooks engine (`Stop`, `SessionEnd`) |

Rules that bind detection:
- **Never read, copy or send credentials.** Anthropic's terms forbid collecting or intermediating Claude.ai credentials (see the DevPulse ledger, R279); token files (`~/.claude/.credentials.json`, the OS keychain, `~/.codex/auth.json`) are never opened. Sign-in state comes from the agents' own status commands and from account metadata files that hold no secrets.
- **Consent per agent, before anything leaves the machine.** Detection is local and prints what it found; linking sends only what the person approves.
- **Personal data minimised.** The devspace stores the agent, its version, the plan when known, and a SHA-256 of the provider account id (so two machines on one account are recognised) — not the email.

## Increments (one PR each)

1. **Release binaries and installers.** A release workflow builds `rroflow` for Windows (x64, arm64), macOS (arm64, x64) and Linux (x64, arm64) with checksums, and publishes one-line installers (`install.ps1`, `install.sh`) on GitHub Releases, later served from rroflow.ai. Standard Rust tooling for this is `cargo-dist`. Until then: `cargo install --git https://github.com/rrflow/rroflow-development rroflow`.
2. **`rroflow setup`: detect, then offer.** Prints each agent found: version, signed-in mode, plan when known, how many past sessions exist. Then, per agent: link it? backfill past sessions? capture every turn from now on? Runs `rroflow login` when not signed in. Re-running is safe and shows what is linked.
3. **Per-turn capture.** `rroflow capture --agent claude|codex` reads the hook payload, sends only the turns not yet sent (a local cursor per session; ids are deterministic, so a repeat replaces). `setup` installs it as the Claude Code `Stop` hook and the Codex hook, and `rroflow setup --remove` takes it out. This also fixes compaction loss: turns are captured before the transcript is rewritten.
4. **Linked agents in the devspace.** Ingest gains `PUT /v1/agents/{agent}` (agent, version, sign-in mode, plan, account hash, last seen) and the dashboard an "Agents and subscriptions" panel: plan, rate-limit windows and usage-limit hits per agent.
5. **Plan windows as data.** Codex `rate_limits` snapshots (used %, window, resets at, plan) stored per turn, so limits and wasted spend are charted, not guessed. Claude Code's rate-limit events the same way when present.

## Devagotchi and the free tier (open)

The direction is stated; the definition is not yet. To decide before building:
- What "layer 2" means for a free-tier user: Devagotchi running `rroflow` for them (install, hooks, upgrades), Devagotchi's board consuming rroflow signals, or both.
- Whether the free tier is a devspace limit (turns, retention, agents) or a feature limit (signals on, learning off).
- Which side owns the install: the rroflow installer, or Devagotchi's `devagotchi start` installing rroflow as a managed component.
