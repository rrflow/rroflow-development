# Site v1

Status: implemented. Moved out of `lib.rs` in refactor R1 (self-host release 1, TASK-004). The one
change is that the embedded assets share a helper; a test pins their headers and bytes, and passes
against the code before the move.

The public product page, the original dashboard and the files they load. Code is in
`crates/rroflow-ingest/src/site.rs` and `site/`. The member console (`/app/manage`, `/app/account`,
`/app/device`, `/app/settings`) is a separate feature, `member_console`. M2 retires the original
dashboard when the member console replaces it.

## Routes

| Method | Path | Access | Serves |
|---|---|---|---|
| GET | `/` | Public | The public product page, rendered from the `[brand]` configuration |
| GET | `/app`, `/app/` | Public | The original dashboard. It reads data only with a credential |
| GET | `/assets/brand.css` | Public | The brand stylesheet, rendered from the same configuration |
| GET | `/assets/public.css`, `/assets/dashboard.css` | Public | Embedded styles |
| GET | `/assets/public.js`, `/assets/dashboard.js`, `/assets/dashboard-a2ui.js`, `/assets/dashboard-datasets.js`, `/assets/dashboard-sdk.js` | Public | Embedded scripts |
| GET | `/assets/research.json` | `project.read` | The indexed source notes |

## Headers

- **Pages** (`/`, `/app`): a content security policy that allows only this origin, no framing,
  `nosniff`, `no-referrer` and `no-store`.
- **Embedded assets:** their content type and `nosniff`.
- **Every response** also gets `cache-control: no-store` and `referrer-policy: no-referrer` from the
  browser-session layer.

No page loads a third-party script or holds a provider credential.
