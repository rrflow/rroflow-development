# Member management

`/app/manage` is the first React-owned console feature: organizations, projects,
connection-key creation/revocation and a project-scoped handoff to monitoring.
It calls the existing Rust tenancy APIs with the signed-in account. It does not
install collectors, grant subscriptions, or implement native account signup.
Full monitoring-console migration remains platform-v1 slice 3.

## Build ownership

Author `src/`, `index.html` and Vite configuration here. Run `npm ci --ignore-scripts`
then `npm run export` to type-check, build and update the three embedded files in
`crates/rroflow-ingest/src/member-console/`. Never edit those generated files.
`npm run verify` rebuilds and rejects stale bytes or orphaned files; CI runs it.
Rust serves the page and assets without a Node server or external font/media
requests. Brand text and colors come from the existing validated deployment
configuration. CSS layers raw values, semantic tokens, controls and compositions.

The export limits compressed JavaScript to 90,000 bytes, CSS to 10,000 and HTML
to 2,048. These are build budgets, not a claim that every development server
compresses responses. The Texas reverse-proxy configuration enables compression.
React loads only on `/app/manage`; the existing monitoring shell retains its
40 KiB compressed gate.

## Verification and limits

`scripts/dashboard-ui/member-console.spec.mjs` drives the rendered screen against
the real Rust service and disposable PostgreSQL. It covers project-scoped ingest,
revocation, viewer permissions, switching organizations during a pending request,
literal untrusted names, signed-out states, desktop/phone reflow and automated
accessibility checks. Its member tokens come from a test identity server, so
these runs are not evidence of production sign-in or real-provider capture.

The current sign-in action uses the configured account origin and existing login
route. That route lands on monitoring; signed-in members can open Organizations
& connections from there. Native signup, invitations/member management in this
screen, full React monitoring parity and production two-account acceptance remain
separate tasks. Connection secrets are shown once and never put in browser storage.

Architecture: `docs/plan/platform-v1.md` and `docs/spec/tenancy-v1.md`.
