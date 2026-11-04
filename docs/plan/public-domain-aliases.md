# Public domain aliases

Status: owner direction recorded, 2026-11-03; alias routing is not implemented or
deployed by this record. The owner wants to retain rroflow.ai and its true public
landing while considering wardenmesh.com as an additional brand and landing.

The proposed boundary is two public presentations leading to the same canonical
member service, accounts, organizations, projects and plans. An alias must not
create a second identity database or split captured activity. The choice between
a distinct WardenMesh page and identical branding remains open.

Current `[brand]` configuration applies to the entire deployment. It does not
select a brand by incoming hostname. A future implementation needs explicit
allowlisted public origins and page configuration, canonical links, edge/TLS
routes and tests for unknown hosts. No domain, issuer or callback should be
hardcoded into application logic. Keep credentials out of public configuration.

Alias landing links should send members to the configured canonical sign-in
origin. Do not expand cookies across unrelated domains or add cross-origin
password submission. Nomen redirect registration and native account origin
checks remain authoritative. Existing rroflow.ai content must remain available.

Acceptance requires both HTTPS pages, their configured branding, working links
into the same member service, no duplicate accounts or data, and explicit
production evidence. DNS, certificate issuance and account linking are not
established by this plan.
