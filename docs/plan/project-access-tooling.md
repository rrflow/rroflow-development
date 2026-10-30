# Project-local deployment access checks

The operator command `scripts/project-access.mjs` consumes the private
`.secrets/access-references.json` in an explicitly selected checkout. It uses
installed Node, GitHub CLI and OpenSSH; it installs no service or package.

```powershell
node scripts/project-access.mjs --project D:/projects/rroflow.ai --online --inspect-host
node --test scripts/test-project-access.mjs
```

The credential files stay with that project. The reference manifest has schema
`rroflow.local-access/v1` and `texas`, `nomen`, `cloudflare` objects. Texas fields
are `host`, `user`, `port`, `ssh_identity_file`, `trusted_known_hosts_file`.
Nomen fields are `issuer`, `ingest_key_file`. Cloudflare fields are `zone`,
`token_file`. Relative file references resolve from `.secrets`; absolute operator
paths are supported. Do not put credential values in the manifest or Git.

Default checks are local only. `--online` reads the Cloudflare zone, token status
and three DNS names. Account-token verification is attempted first; user-token
verification is the fallback. The tool reports advertised DNS-edit permission
without testing a DNS mutation. It verifies Nomen discovery, signs the same
short-lived private-key assertion as the ingest, and introspects an intentionally
inactive random token. HTTP 200 with `active:false` proves acceptance of that
ingest credential, not member login, audience mapping, CLI/dashboard registration
or tenant isolation. Discovery cannot redirect credential-bearing requests to
another origin. Redirects are disabled, bodies bounded and private errors hidden.

`--inspect-host` runs a fixed read-only inventory via SSH only when both original
trust and identity file references exist. It disables automatic host-key updates,
global/configured SSH overrides, interactive password prompts and alternative
identities. A missing/mismatched host key must be resolved from trusted inventory,
never by accepting a network scan as authority. It reports hypervisor presence;
an SSH connection to a Proxmox host is not an application-VM installation target.

Exit 2 means an access prerequisite is missing or a requested check did not pass;
exit 1 means arguments/configuration could not be loaded. Exit 0 is only an access
check result. No result certifies a live member service, subscriptions or datasets.
The tool never changes DNS, deploys, edits trust, writes credentials or provisions
anything. Use the existing [hosted activation gates](hosted-releases.md) afterward.

API references: [Cloudflare zone listing](https://developers.cloudflare.com/api/resources/zones/methods/list/),
[user token verification](https://developers.cloudflare.com/api/resources/user/subresources/tokens/methods/verify/).
Nomen authentication follows `crates/rroflow-ingest/src/nomen.rs` in this repo.
