# Dedicated member sign-in

Status: implemented in source, 2026-11-03; deployment remains pending. This is not a
claim of a working production Nomen deployment.

`/auth/login` opens `/app/account` when either supported sign-in method is
configured. The public landing remains at `/`. The account page asks the Rust
gateway which methods are enabled; it never infers them from a product name.

When browser federation is enabled, **Continue with Nomen** is the first and
primary action. It starts the existing server-side authorization-code flow with
S256 PKCE at `/auth/federation`. `[brand].federation_label` changes only the public
label, defaults to Nomen and accepts at most 80 printable characters. Issuer,
client, callback origin and secrets remain separate authentication configuration.

When native accounts are enabled, email/password, verification and recovery stay
available below that action. Password visibility is an explicit keyboard-accessible
toggle and resets when changing forms. A failed options request offers a retry;
unavailable methods do not produce inert forms. Federation-only installations get
the same dedicated page without password, registration or recovery controls.

Native sign-in currently accepts an email address, not a separately allocated
username. Separate handles require uniqueness, rename and recovery rules before
implementation. Nomen and native identities are not silently linked by matching
emails. Explicit account linking remains open. Native collector authorization
uses its existing password flow; the device-return page does not offer federation
until that identity can authorize the same device grant safely.

## Reference adoption

The local Nomen checkout at revision
`93af5ac8301f53b29a62e9c2ae90e6790ac06be4` supplied these interaction references:

| Source | Decision and destination |
|---|---|
| `apps/login/src/components/sign-in-with-idp.tsx` | Adopt configuration-driven provider choice in `console/src/account.tsx`; preserve RROFLOW's Rust PKCE route. |
| `apps/login/src/components/password-form.tsx` | Adopt separate recovery and pending/error feedback in the existing native account page. |
| `apps/login/src/components/username-form.tsx` | Reference only; Nomen's login-name namespace and organization context do not define native RROFLOW usernames. |

No Next.js application, credential RPC or filesystem dependency is copied into
RROFLOW. This source review does not verify which revision runs in Wisconsin.

## Verification boundary

Rust checks retain callback/browser binding, origin and cookie protections. The
packaged browser suite runs the real Rust gateway against disposable PostgreSQL
on desktop and phone: native registration/recovery, Nomen-first selection,
federation-only selection, configurable labels, options retry, accessible controls
and collector return. The configured external issuer is a fixture: the tests inspect the real
authorization redirect without completing a live Nomen login. This does not
prove successful production federation.
