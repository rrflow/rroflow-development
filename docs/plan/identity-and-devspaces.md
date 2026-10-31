# Identity and devspaces

Updated 2026-10-31. This guide describes implemented source behavior. Texas placement,
public TLS and a real hosted member session remain unverified. It supersedes the old
Gold-based credential instructions and source-build activation steps.

## Account ownership

Nomen supplies account identity. RROFLOW stores an account-owned devspace in Postgres;
it does not store a member's provider password or issue its own identity-provider account.
The server resolves the devspace from the accepted credential. A request cannot select
another person's workspace by changing a tenant parameter.

`devspaces` records the owner issuer and subject, optional identity-provider organization,
and an optional Shippin tenant reference. The issuer/subject ownership pair is unique.
The Shippin reference is an integration field, not proof of subscription synchronization
or an implemented organization/invitation workflow. Identity alone grants neither a paid
plan nor permission to collect datasets or train models.

| Surface | Implemented credential path | Boundary |
|---|---|---|
| Member dashboard | Authorization code + S256 PKCE; access token held by the Rust service; opaque HttpOnly browser cookie | Requires configured dashboard client and verified callback; live member session still needs proof |
| Collector | Device-flow login with protected account storage, or an explicit headless ingest key | Saved queues are bound to their account and ingest destination; changing a profile does not move history |
| Hosted API | Key-hash lookup or Nomen token introspection using the ingest application's private-key assertion | Checks issuer, project audience, allowed client, expiry and subject; unavailable identity service is not an authenticated member |
| Static ingest keys | Operator registration/key generation and hashed server lookup | Member-facing key management is not implemented; key access cannot authorize member-only dataset consent |

The collector's current protected `account.json` is separate from its public deployment
configuration; this guide does not claim OS-keychain token storage. See the owning
[deployment configuration guide](deployment-configuration.md) for precedence and account
switching, and [browser sign-in](browser-sign-in.md) for cookie/session behavior.

## Configure the deployment

Obtain the issuer, project audience, CLI client, dashboard client and canonical ingest/app
origins from the operator's verified deployment record. There are no compiled production
identity defaults. Keep actual credential files and deployment references outside Git.
For this project, [the access tool](project-access-tooling.md) reads private project-local
references and can check discovery, the ingest credential, DNS and a trusted SSH route.
It does not prove a real member login, paid plan or deployed service. Gold is not an access
dependency for this work.

The service reads a retained external TOML file. These are illustrative values to replace,
not a live environment:

```toml
[nomen]
issuer = "https://identity.example.test"
audiences = ["PROJECT_AUDIENCE"]
clients = ["CLI_CLIENT", "DASHBOARD_CLIENT"]
key_file = "/etc/rroflow/nomen-key.json"

[browser]
origin = "https://app.example.test"
client_id = "DASHBOARD_CLIENT"
```

The key file is the ingest application's private credential, readable only by its intended
service/operator. The dashboard and CLI are separate public clients; they do not receive
that key. Register the exact dashboard origin plus `/auth/callback`, with the matching
public-client and PKCE settings described in the browser guide. Use the configured origin
for sign-in; a cookie on one hostname does not authenticate another hostname.

Use the [GitHub hosted release and activation procedure](hosted-releases.md) after verified
VM placement and recovery checks. Adding identity settings is not an instruction to rerun
the old source-build installer or overwrite a proxy. DNS records and HTTP liveness alone
do not establish usable sign-in.

## Connect a person's agents

Provide the public deployment JSON described in [deployment configuration](deployment-configuration.md).
Run `rroflow setup --deployment /absolute/path/to/deployment.json` on the execution machine,
using the same Shippin account as the dashboard. Setup asks before linking agent sessions
and can register per-user startup. Verify the account with `rroflow whoami`, local delivery
with `rroflow sources status`, and received reports in the dashboard's Sources view.

For SDK calls, use the [SDK wrappers](../../sdk/javascript/README.md) and
[managed SDK delivery](../ux/sdk-background-delivery.md). A connected session collector
does not automatically wrap arbitrary application SDK calls. Subscription browser/mobile
history and provider-internal actions are separate coverage questions.

Existing key-owned data can be explicitly claimed by an operator after verifying the
intended account subject:

```sh
rroflow-ingest claim --config /etc/rroflow/ingest.toml --devspace EXISTING_SPACE --subject VERIFIED_SUBJECT
```

The command applies ownership safeguards; do not assign an owner's historical space to
the first visitor. It is not a general data-merge or organization-transfer facility.

## Account-facing boundaries

Authenticated turn, SDK event/activity, source-health, findings and review endpoints resolve
the caller's devspace. Public pages and the signal catalogue contain no private activity.
Dataset consent requires member identity, and account allowances are owned by the server.
See [SDK monitoring](../spec/sdk-activity-v1.md), [dataset consent](../spec/metadata-datasets-v1.md)
and [account allowances](../spec/account-allowances-v1.md) for their specific contracts.

Before calling the hosted service live, prove real sign-in, same-owner Claude and Codex
capture/readback, second-account isolation and sign-out on the verified deployment.
Production recovery, public TLS and subscription synchronization remain separate acceptance
requirements. Local fixtures and successful ingest-credential introspection do not replace
those checks.
