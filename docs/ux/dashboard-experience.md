# Dashboard experience: notes to implementation

Date: 2026-10-27. Status: implemented on `codex/dashboard-experience`, based on
`codex/sdk-capture`. This records a dashboard increment, not a production launch.
The local serving process has not been replaced. Texas deployment and hosted
account activation remain unverified. No personal browser was controlled during
this increment; the checks below do not establish visual acceptance in a browser.

## What the user can do in this build

The workspace has seven focused destinations: Overview, Activity, Signals,
Sources, Learning, Settings, and Research & guides. The sidebar replaces the
previous long page of stacked sections. Small screens have a navigation drawer;
closed navigation is removed from keyboard focus. Appearance follows the device
or an explicit light/dark preference. There are no external scripts or fonts.

Activity has server-side search, agent/project/signal/session filters, a time
range and cursor pagination. The URL retains the view and filters for back/forward
navigation. Search covers captured prompt, response, model and session ID, not
uncaptured content or every tool payload. A turn opens in a native dialog with
Conversation, Tools, Signals and Details tabs. The session button opens that
session; a copied turn link still requires authorization to the same workspace.
Long tool traces initially render 50 calls, with explicit expansion.

Signal cards show the rule revision, summary and source evidence. Reviews persist
MATCH/NOT MATCH against the exact evidence digest. Settings persist through the
existing devspace API. Neither action silently activates a learned policy.

The research library includes the full text of every Markdown note under `docs/`,
including all eight Raindrop documents. It can be searched by title/path/category
and read in the dashboard without navigating away. Its generated inventory carries
SHA-256 fingerprints of canonical UTF-8/LF source text. CI detects a stale index.
The original notes are preserved; the library is a projection, not a new authority.

## Review of the existing 33 source notes

This was an inventory and targeted requirement review: product/experience notes
were read directly, while large historical ledgers were inspected by their
headings and relevant evidence sections. It is not a claim that every historical
statement was independently reverified. The complete source texts are retained
for inspection in the library.

| Existing notes | Consequence in this increment |
|---|---|
| `docs/README.md`, `plan/rroflow-ai-plan.md`, `plan/canon-alignment-notes.md`, `plan/capability-map.md` | Separate monitoring from the RRFlow engine; distinguish built capabilities from proposals; preserve source provenance. |
| `plan/browser-sign-in.md`, `plan/identity-and-devspaces.md` | Shippin sign-in is primary; devspace keys are an advanced preview path. Every data query remains account-scoped. |
| `plan/sdk-install-and-linking.md`, `plan/source-collection.md`, `plan/subscription-support.md` | Separate provider, app surface and execution machine. Received records are not proof of a live source. Grok SDK fixture coverage is not subscription-history support. |
| `plan/texas-readiness.md` | No cloud-ready or Texas-deployed badge without verified infrastructure and activation. The connection guide identifies unreleased development commands. |
| Raindrop `findings-brief.md`, `live-capture-checklist.md`, `live-capture-ledger.md`, `prototype-ledger.md` | Real captured turns and evidence are primary navigation. Empty states say what has not arrived. Counts name their scope; no invented activity or confidence. |
| Raindrop `signal-authoring-capture.md`, `source-ledger.md` | Plain-language signal summaries, inspectable evidence, revision-pinned verdicts and no automatic classifier rewrite after a label. |
| Raindrop `ux-onboarding-and-gating.md`, `ux-settings.md` | Friendly first-capture entry, visible settings and source limitations, light/dark/system themes. No fabricated quota or unlock meter. |
| `research/devpulse/devpulse-research-ledger.md` | Protect wallet/time, preserve unknown usage, distinguish content from metrics, and report observed truth. Spend remains explicitly unpriced. |
| `research/rea/rea-source-ledger.md`, `spec/mechanism-spec-v0.md`, `spec/automatic-datasets-v0.md` | Review leads toward consented datasets and evaluated learning. Clusters are candidates, not labels. Unimplemented jobs have no pretend action buttons. |
| `ux/vibe-coder-onboarding.md` | Progressive disclosure and first useful evidence. Its older payload-on proposal is superseded by the newer explicit content opt-in contract. |
| `platform/compute-and-data-workload-classification.md`, `platform/skills-and-routines-inventory.md` | Separate serving, capture and later dataset/training work. Do not imply training runs inside the dashboard process. |
| RRFlow `build-order.md`, `capability-study.md`, `definitions-and-schema-audit.md`, `development-salvage-assessment.md`, `merge-train-2026-10-19.md` | Engine research remains available but does not become RROFLOW account or monitoring UI authority. |
| RRFlow `public-site-build-order.md`, `site-video-pipeline.md` | The planned Astro public story site and Remotion explainers remain separate from this working Rust-served dashboard. |

## API and verification boundaries

`GET /v1/turns` adds literal `search`, RFC 3339 `since`, opaque `cursor`, and exact
`event` parameters. Unknown parameters and malformed navigation fail with 400.
Pages order by timestamp descending, then event ID ascending, with undated turns
last. Both stores use the same ordering and filter before limiting. Cursors are
positions, not authorization; the authenticated devspace always scopes the query.
This is not a snapshot export: concurrent edits to a turn's timestamp can change
its page position. Dataset snapshots need their own immutable contract.

The Rust tests exercise assets, authentication, account isolation, equal-time and
undated pagination, literal search, date filters and exact-event lookup. CI runs
the same scenarios against Postgres. The Node tests execute the shipped script
with an isolated DOM/fetch harness: navigation, auth state, late responses after
logout/filter changes, content escaping, exact-review writes, settings writes,
research access and bounded tool rendering. They are not screenshot or browser
layout tests. A responsive layout, focus treatment and tab semantics are present
in the implementation; visual and assistive-technology acceptance remains open.

## Remaining product acceptance

- Activate a reviewed build and verify the UI against real records in the serving
  environment. The local process restart was previously rejected by automatic
  execution review with only “blocked by policy”; this is not missing user consent.
- Confirm Texas VM identity/access and implement a GitHub-built immutable hosted
  release, transactional activation, recovery, TLS and working Shippin login.
- Complete source installation/startup, live heartbeats and queue health, reliable
  hooks, retention/quotas and settings refresh. A finite capture pass is not an
  always-running service.
- Verify live provider integrations individually. Subscription ownership alone
  cannot establish observability of browser/mobile histories.
- Add server-enforced free/paid entitlements, validated pricing/plan limits,
  redaction editing and dataset consent. Then implement dataset revisions,
  supervised/unsupervised jobs and held-out reversible promotion.

These remain acceptance work. A clearer dashboard does not close them.
