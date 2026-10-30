# Configurable public and member branding

Status: implemented in source, 2026-10-30. This does not activate a hosted service,
subscription entitlement or learning feature. Branding is owned by the deployment
operator and is shared by that deployment's public site and member dashboard.

## Change the presentation without changing application code

Merge [the example](../../deploy/brand.example.toml) into the TOML file passed to
`rroflow-ingest serve --config`. Replace the example public names and links, then
restart the service. Omit `[brand]` to retain RROFLOW defaults. Unknown fields,
invalid links, excessive text and unreadable color combinations prevent startup.

| Setting | Surfaces | Limits |
|---|---|---|
| `name` | Page titles, welcome heading and product references | 80 characters |
| `wordmark` | Public header/footer, sign-in and workspace navigation | 24 characters |
| `mark` | Small vector-style brand tile | 2 characters |
| `tagline` | Footer and sign-in story | 160 characters |
| `intro_title`, `intro_emphasis` | Public introduction | 120 characters each |
| `intro_description` | Public introduction and search description | 500 characters |
| `documentation_url` | SDK connection guide | Absolute HTTPS URL |
| `links` | Public footer | Up to 6 label/URL pairs; label up to 60 characters |
| `accent` | Light-mode emphasis, links, charts and focus treatments | `#RRGGBB`, dark enough for light surfaces |
| `highlight` | Brand tile, vector artwork highlights and dark-mode accent | `#RRGGBB`, light enough for dark ink/surfaces |

Text must contain printable characters and is always HTML-escaped. It cannot
install scripts or markup. Link URLs are bounded to 2,048 characters; credentials,
non-HTTPS schemes, whitespace and backslashes are rejected. To remove partner
links, set `links = []` within `[brand]`; the default list is used only when the
field is omitted. All configured values are public presentation data: no secrets
or private document URLs belong here.

The default palette remains warm and quiet. The example uses blue accents and
the same local vector artwork. Accent text must meet 4.5:1 contrast on the light
surfaces it uses, while highlight/dark-mode accent must meet 4.5:1 against the
specified dark surfaces. Semantic error/warning colors and the structural
surface palette remain application-owned. Arbitrary CSS, external fonts, remote
logo images and layout scripts are not accepted through this configuration.

## Rendered at startup, no branding fetch required

The Rust gateway validates the brand and renders both HTML pages at startup.
Authored templates retain default content for the static site build. Explicit
slots identify configurable text; research documents and captured evidence are
never rewritten. The small `/assets/brand.css` response carries only validated
hex colors and fixed layout rules. The SVG illustration remains embedded and
uses the highlight variable, so it needs no image download.

The HTML and brand stylesheet use `Cache-Control: no-store` to avoid serving an
old deployment identity after a restart. Existing same-origin CSP remains in
place, without inline script/style permission or extra network destinations.
Sign-in origins and collectors continue to use their separate deployment
configuration. Brand links do not configure authentication or ingestion.

There is no tenant-supplied brand editor in this increment. Member data is still
scoped by the credential. The public brand says nothing about a member's plan,
and cannot turn on a subscription, remove consent checks, or make datasets and
learning workers active. Capability-status and coverage explanations remain
application-owned; configurable introductory copy must describe the service
truthfully.

## Verification

Rust checks cover separate default/custom instances, escaping, unknown settings,
safe links, contrast validation, unchanged CSP and unauthenticated data refusal.
The packaged browser suite exercises the default UI and an independent Orbit
fixture on desktop and phone: first-response copy, themed SVG, footer links,
member navigation, evidence and dark mode. Screenshots contain disposable
fixture records, not production activity. Automated accessibility checks are
supplemented by visual review and do not replace real-user acceptance.

This implements public/member identity and accent customization. Full layout
theming, tenant-managed brands, infrastructure layout configuration, commercial
licensing and the hosted subscription/dataset roadmap remain separate work.
