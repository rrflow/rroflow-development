# Hosted service acceptance: owner references

Owner direction: 2026-11-02. Status: required outcome, not delivered. This clarifies
the existing [platform plan](../plan/platform-v1.md) and [capability map](../plan/capability-map.md).
The three supplied screenshots are visual acceptance references, not evidence that
RROFLOW already implements the capabilities shown.

## Required hosted journey

1. A visitor opens rroflow.ai and can request an early-access invitation with an
   explicit email purpose. Confirmation must follow a durable accepted request.
2. An authorized service administrator reviews requests, invites or approves users,
   and manages their access and plans through audited controls. Ordinary members
   cannot acquire administrator rights or paid entitlement by signing up.
3. A member signs in, selects a project, connects Claude Code, Codex or a supported
   application SDK once, and sees real received events and delivery health.
4. Capture persists locally through connection failures and resumes against the
   same account/project. A phone or remote dispatch reports from the execution
   source; owning a subscription alone is not proof of capture access.
5. Overview, signal trends, searchable signal rows and evidence links reflect those
   records. Two accounts must remain isolated through every read and mutation.
6. Dataset consent, immutable examples and reviewed outcomes feed the governed
   learning path. Matches and model guesses never become ground-truth labels by
   default. Promotion remains evaluated and reversible.

Shippin remains the subscription authority. Administrator plan controls must use
the accepted entitlement contract and enforce it server-side; the UI is not a
billing authority. A working early-access request is distinct from public signup
being enabled or a paid subscription being active.

## Visual acceptance

- Workspace home follows the reference panel hierarchy: an overview with meaningful
  activity counts and a time range, issues, signals and models. Unknown values are
  unavailable, never invented zeros or synthetic percentage changes.
- Signals use the reference compact table, search, groups and source/category
  filters, with real trends above and a clear create action. Definitions and
  evidence drill down without losing account/project/time context.
- The persistent navigation target includes Home, Events, Signals (positive,
  negative, other, instrumented, self diagnostics, tool errors), Users, Issues,
  Briefs, Datasets, Costs, Dashboards, Experiments, Simulations and Agent; Settings,
  Support and MCP remain discoverable. Plan usage is visibly tied to actual
  server allowance records. A label alone never counts as an implemented screen.
- Preserve lightweight vectors, readable table density, tooltips, accessible
  dialogs, keyboard navigation and mobile reflow. Use RROFLOW branding and real
  data. Do not reproduce a competitor quota or upgrade state as our own.

## Current boundary

The React project overview is merged in PR53. The member signal library is being
qualified and now uses a compact table in response to these references. Native
accounts/device grants and capture queues have source-level tests. This does not
establish the full reference layout, hosted early-access requests, service-admin
user/plan controls, actual provider subscription coverage or a Texas release.

Current operational checks find the expected Cloudflare zone and active DNS-edit
token. Trusted Texas SSH file references are still missing on the executing
account. Nomen discovery succeeds, but the current introspection probe returns
`unauthorized_client`; previous authentication evidence must not be reused as a
current pass. Runtime values and secrets stay outside the repository.

## Reference identity

- Workspace overview: `codex-clipboard-OVW1GF.png`, SHA-256 `0d04586f7e1cd6e86941412efb332d01acf927cbff90f425e0b5b69e2043562f`.
- Signal dashboard and table: `codex-clipboard-p41TJa.png`, SHA-256 `b379fdc4ce7ca1e95954e569ab9bff0e5d2699eb7537222a1e126bd5a8499751`.
- Persistent navigation and plan usage: `codex-clipboard-CGfHIb.png`, SHA-256 `b299187adfd122e9f67303df7dfff2eabee29cdbf4bf65ddcbc9af5f9e98c81c`.
