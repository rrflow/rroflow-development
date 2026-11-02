# RROFLOW platform v1

Date: 2026-11-02. Status: active implementation. Owner of the slice sequence below.
Tenancy (PR46), initial receipt provenance (PR47) and initial React organization/project/key
management (PR48) and durable browser sessions (PR49) are merged in source;
see [session storage](../spec/browser-sessions-v1.md). Native browser signup/recovery is under verification
([contract](../spec/native-accounts-v1.md)); native device flow, explicit linking and full React monitoring
parity remain incomplete. Nothing here establishes deployment. Texas activation remains
separately blocked (see "Dependencies").

Goal (owner, 2026-11-02): RROFLOW should work the way Sentry or PostHog do, for AI. A person
signs up, creates a project, installs an SDK or connects their agents, and from then on every
time that project starts it reports, with provenance, into clearly organized signals. This
builds on the [capability map](capability-map.md), which stays the owner of capability status.

## Baseline when this plan was written

This table records the starting gaps, before PR46–48. The slice descriptions and linked
contracts below record their implementation scope; it is not a current deployment inventory.

| Need | Today (v0.4.0 source) | Gap |
|---|---|---|
| Member app | Hand-written HTML/JS dashboard embedded in the Rust service; Astro public page | No component app, routing, typed API client or org/project navigation |
| Login | Nomen OAuth only; sessions held in server memory | No first-party accounts; a restart signs everyone out |
| Tenancy | One devspace per Nomen account | No teams, roles, invitations or several projects per person |
| Keys | Operator edits a config file and restarts | No self-service creation or revocation; every key reads as well as writes |
| Onboarding | `rroflow setup` plus a guide | No project picker, generated instructions or live wait for the first event |
| SDK | JavaScript wrappers; caller supplies devspace and spool, and wraps each call | Not automatic: nothing reports unless the code is changed call by call |
| Agents | Machine-wide session collector; nothing per project | Sessions cannot be routed to a chosen project |
| Provenance | Client-supplied fields on the record (`cwd`, `git_branch`, `collector`) | Nothing separates what the server proved from what a client claimed |
| Organizing signals | Findings per turn, library, drill-down | No grouped issues, lifecycle, assignment or alerts |

## Decisions

Owner-answered on 2026-11-02 and recorded in Shippin ADR 0013:

- **Native accounts plus Nomen.** RROFLOW stores its own users (email and password) and also
  offers "Continue with Shippin" through Nomen, which brokers Google and GitHub. A sign-in
  never grants a paid plan; Shippin stays the subscription authority.
- **React, TypeScript and Vite** for the member console, built to static files served by the
  Rust gateway at `/app`. The gateway stays the only data client. The Astro public site stays.

Proposed here, to be challenged in review:

- **Organization, then project, then key.** A project *is* an existing devspace: every
  monitoring table already partitions on `devspace_id`, so no record moves. The organization
  is the allowance-pooling unit, so creating projects cannot multiply a free allowance.
- **No secret in the repository.** A committed `.rroflow.json` names the project and endpoint
  only. A signed-in member's own credential authorizes delivery, which also proves who sent
  it. Servers and CI use a write-only project key from their environment. This differs from
  Sentry's public DSN on purpose: keys can currently replace a turn by `event_id`, so a key
  published in a repository would be an integrity problem until slice 3 lands.
- **Secrets stay outside the Rust process's concerns.** Launch with `vaultix run --` or a
  mounted file; the service reads environment variables and private files. No Infisical
  client is compiled in, and the service does not need Vaultix reachable while running.

## Slices

One reviewable pull request each, in dependency order. Each must pass the existing CI gates.

### 1. Tenancy foundation (implemented in source)

Organizations, users, memberships, projects, write-only project keys, invitations, roles and
an audit trail. Org-pooled allowances. Member requests select a project with a header that is
checked against membership. See [tenancy v1](../spec/tenancy-v1.md). Closes part of C23, C39,
C40 and C41.

### 2. Native accounts and federation

Email and password with argon2id, email verification, password reset and rate limiting;
sessions in Postgres so a restart does not sign people out; generic OIDC federation with Nomen
as the first issuer; account linking only on verified email with explicit confirmation; a
device flow served by RROFLOW so `rroflow login` works for native accounts; reviews record RROFLOW
user ids rather than Nomen IDs. Passkeys and MFA follow. Requires an email provider (open question)
and a security review before public signup.

### 3. Provenance contract

The receiving identity and replacement enforcement are being implemented first; see
[receipt provenance v1](../spec/provenance-v1.md) for exact behavior and remaining work.

`rroflow.provenance/v1` separates two groups on every stored record:

- **Attested** (set by the server, refused from clients): organization, project, credential
  kind and id (member or key, never the token), received time, service revision.
- **Claimed** (set by the client, labelled as a claim): capture method (hook, transcript, SDK,
  OTLP), collector, SDK and agent versions, machine hash, repository identity, branch, commit,
  observed time.

A record's provenance is never fabricated or backfilled as attested. Older records report
`unknown`. A write-only key may replace only records that same key wrote, and keys get per-key rate
limits so a leaked one cannot spend the whole organization's allowance. The console shows provenance
coverage per project. This comes before slices 6 and 7 so that the new capture surfaces have
provenance from their first event.

### 4. Member console

React app with `/app/:org/:project/...` routes; organization and project switcher; members,
keys and audit screens; every existing view ported. OpenAPI generated from the Rust routes
drives a typed client. The existing Playwright desktop and phone scenarios are the parity
gate: the old dashboard is removed only when they pass against the new console. Keeps the
strict same-origin content policy and deployment branding.

### 5. Turnkey onboarding

Sign up, name the organization, create a project, pick platforms, get exact commands, then a
live wait that succeeds only on a real received record, never on a button click. `rroflow init`
and `npx @rroflow/wizard` do the same from a terminal: browser sign-in, pick the project, write
`.rroflow.json`, install hooks or the SDK, send a verification event.

### 6. Automatic capture for coding agents

| Agent | Project scope | Global scope | Checked 2026-11-02 |
|---|---|---|---|
| Claude Code | `.claude/settings.json` hooks | `~/.claude/settings.json` plus managed settings | Existing wrappers use these hook names; run a real session in this slice |
| Codex | `<repo>/.codex/hooks.json` | `~/.codex/hooks.json`; `requirements.toml` for managed hooks | Official docs read; hook payload gives `session_id`, nullable `transcript_path`, `cwd`, `hook_event_name`, `model` |
| Grok Build | Project hook configuration, JSON or `config.toml` | User configuration | Changelog confirms hooks and `grok inspect`; payload and paths still need the official hooks page and a real run |

Hooks call `rroflow capture`, which walks up from the hook's `cwd` to find `.rroflow.json` and
routes the session to that project. Global mode uses the same hooks at user scope plus the
existing managed collector. A hook must exit quickly and successfully whatever happens: it
queues durably and never blocks or fails the agent.

**"Every time the project starts" has a real limit.** Codex loads project hooks only after the
person trusts the project layer and reviews the hook, and a changed hook is skipped until it
is re-trusted. RROFLOW cannot and should not bypass that. So the guarantee is built from four
layers, and the gap is shown rather than hidden:

1. Project hook files, committed once and working for everyone who trusts them.
2. Global hooks and the managed collector for a person's whole machine.
3. Managed policy for organizations that can enforce it (Codex `requirements.toml`, Claude
   managed settings).
4. A `session.started` heartbeat and the transcript collector as a backstop, so the console
   can report which people and machines in a project have stopped reporting.

### 7. Application SDKs

`@rroflow/node`: `import "@rroflow/node/auto"` or `node --import @rroflow/node/register`
instruments Anthropic, the Claude Agent SDK, OpenAI, the Codex SDK and xAI without changing
each call. The current wrappers become its internals. Python follows with the same contract.
The durable outbox stays the one Rust implementation, shipped through platform-specific npm
packages, so offline recovery does not fork. Real streaming, tool, cancellation and error runs
per provider and version are required; fixtures alone do not close C02.

### 8. Issues and alerts

Group signal hits into issues by a versioned fingerprint, with assign, resolve, ignore,
reopen, regression state and first/last seen per project, environment and release. Alert rules
with deduplication, routing and delivery receipts. Every count opens the same evidence cohort.
Closes C16 and C17.

## Dependencies and open questions

- **Texas host access** is still the blocker for anything being live: the SSH key and trusted
  host record were last reported missing. Real-member acceptance needs it.
- **Email provider** for slice 2 is not chosen. Its credentials belong in Vaultix.
- **Vaultix availability from Texas** is unverified. The file fallback means startup does not
  depend on it, but the secret-delivery path needs one real run.
- **Nomen** must register RROFLOW as an OIDC client for the new callback path in slice 2, and
  Google or GitHub brokering is configured there, not here.
- **Entitlements**: how a native account links to a Shippin membership to receive a paid plan
  needs a versioned contract with shippin-site. Allowances stay operator-set until then.
- **License** selection remains open and is unchanged by this plan.

## Proof

No slice is complete because a screen exists. Before calling the hosted service live: two real
accounts, each signing up, creating a project, connecting a real Claude, Codex and Grok source,
seeing the first real event with correct provenance, and being denied access to each other's
data; plus an invited teammate at each role, key revocation taking effect, and disconnected
delivery recovering.
