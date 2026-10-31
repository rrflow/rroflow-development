# Public and member entry

Status: implemented in development; hosted activation remains unverified.

Visitors land on `/`: a public explanation of activity, signals, evidence,
connections and the learning direction. `/app` contains the sign-in and member
workspace. Public browsing does not request private activity or identity. The
page has authored vector artwork, system fonts, no external runtime assets and
no invented usage figures. The artwork is explicitly an illustration.

`site/` is the Astro static authoring source chosen in the product plan. Run
`npm ci --ignore-scripts` then `npm run export` there to update the embedded
`public.html`, `public.css` and `public.js`. Do not edit those generated files.
CI runs `npm run verify` to reject drift. The Rust gateway serves all three from
the same immutable artifact as the API and workspace; deployment needs no Node
runtime. The existing dashboard stylesheet supplies the shared visual tokens.

## Account entry

`GET /auth/config` returns `enabled` and `member_origin` (null when disabled).
The latter is the already-validated canonical HTTPS browser origin, not a
request-supplied redirect. Public workspace links use that origin. The dashboard
sign-in link starts OAuth there so the browser binding cookie and callback share
a host. The callback finishes at `/app`, or `/app?signin=failed`. Tokens remain
server-side; this change does not alter ownership or credential acceptance.

Existing `/#activity?...` and other unambiguous workspace bookmarks move to
`/app#...`, preserving filters. `/#learning` now names the public learning
section; use `/app#learning` for the member destination. Copied turn links use
`/app`. The public page stays readable when sign-in configuration cannot load.

The member sidebar distinguishes verified account access from devspace-key
access and displays the returned name as text. The first-steps checklist uses
verified workspace access, received turn count and saved review history. Clicking
setup does not complete a step. Unavailable review history cannot count as a
completed review. The checklist opens for an empty workspace and collapses once
turns exist; the monitoring overview stays prominent. Private member context and
progress clear at logout. These milestones do not assert connection health or
complete provider coverage.

## Acceptance boundaries

The same gateway can expose public entry and member UI, but this does not make
the hosted service live. Real Nomen registration/sign-in, same-account collector
capture and two-account readback still need acceptance on the verified Texas VM.
Shippin-site's inspected MemberAccountPort/Supabase adapter is not a released
RROFLOW subscription contract. Free/paid entitlements, billing, self-service
credential management and retention controls remain separate unfinished work.
Automatic datasets, supervised training and unsupervised discovery remain
planned. Consumer subscription browser/mobile history is not automatically read.

Validation covers public and private route separation, disabled/configured auth
states, callback routing, unchanged tenant/session security, evidence-derived
onboarding, logout cleanup, desktop/phone reflow and accessibility. Browser tests
use disposable Rust-server fixture records. Canonical-origin UI tests supply
fixture configuration; identity exchange tests use a mock provider. Neither is
evidence of a live account or Texas deployment.

Astro implementation references: [static deployment](https://docs.astro.build/en/guides/deploy/)
and [external scripts](https://docs.astro.build/en/guides/client-side-scripts/).
