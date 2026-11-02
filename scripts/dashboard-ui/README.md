# Isolated dashboard browser verification

Development-only Playwright/axe tests, pinned by `package-lock.json`. CI runs them
against the just-built Linux ingest binary, with a random loopback port, fresh
temporary JSONL store and generated fixture credential. The helper has no option
to select an existing server or data directory. It never reads a user profile,
private transcripts, production keys or the serving preview.

The 1440-pixel desktop and 390-pixel phone projects exercise sign-in, overview,
search/pagination/history, session drill-down, safe transcript text, incremental
tool rendering, keyboard tabs, persisted verdicts/settings, dark appearance,
research access, mobile navigation and logout. Each principal view is checked for
horizontal overflow and automatically detectable WCAG A/AA issues. Screenshots
and failure traces are uploaded as CI artifacts. Every screenshot uses explicitly
named `ui_fixture` records; none is evidence of production activity or deployment.

On a development or CI environment, using a separately built fixture binary:

```sh
npm ci --ignore-scripts
npx --no-install playwright install --with-deps chromium
RROFLOW_UI_BINARY=/absolute/path/to/rroflow-ingest npm test
```

Browser dependencies are installed only in the CI runner during the automated
workflow. These tests were added without controlling the personal browser or
restarting the local serving preview. They are not pixel-baseline tests: images
must be visually reviewed. Automated accessibility checks do not replace manual
assistive-technology and real-user acceptance. Hosted Shippin OIDC is a separate
acceptance flow; the fixture server uses the devspace-key fallback.

Primary setup references: [Playwright CI](https://playwright.dev/docs/ci-intro),
[accessibility checks](https://playwright.dev/docs/accessibility-testing), and
[visual comparison limits](https://playwright.dev/docs/test-snapshots), observed
2026-10-27. Versions were resolved from the primary npm registry and pinned before
execution.

Windows is also supported for disposable fixture runs: set `RROFLOW_UI_BINARY` to an isolated build of `rroflow-ingest.exe`, then run `npm test`. The helper always creates a fresh temporary store, random key and loopback listener and closes that child afterward. It cannot attach to the personal preview. The tests now also cover contextual guidance, focus return, the connection-choice dialog and evidence-linked next actions.

The PostgreSQL member fixtures also cover automatic dataset consent and SDK monitoring.
Set `RROFLOW_DATASET_UI_DATABASE_URL` to a disposable PostgreSQL database and put the
matching `rroflow` collector binary beside `RROFLOW_UI_BINARY`. SDK fixtures use the real
Claude/Codex/Grok adapters and durable recorder, with explicit provider response fixtures;
they verify the background projection, filters, exact event lookup and historical reviews.
These tests do not call paid providers. Without that database the member cases are skipped;
CI always supplies it. Never point the fixture variable at a personal or production database.

`member-console.spec.mjs` exercises `/app/manage` through real organization,
project and key APIs. It verifies the handoff to project-scoped monitoring,
immediate key revocation, viewer restrictions and discarded stale responses when
switching organizations. Desktop/phone screenshots use disposable memberships;
the test identity server does not prove a production account-login journey.

`native-account.spec.mjs` runs email registration, password sign-in and recovery into
actual native-member project records on desktop and phone. Its isolated PostgreSQL
schema and private encryption key let the test inspect a queued verification link;
no production token-reading endpoint exists. HTTPS browser routing preserves secure
cookie behavior while forwarding to the loopback Rust fixture. This is not proof of
real TLS termination, SMTP delivery or a live member. `pg` is test-only; the deployed
service remains Rust and static assets. The fixture always removes its own schema
and stops only the child it started.
