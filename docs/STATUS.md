# rroflow.ai status

Verified 2026-11-02. This is a status summary; the [capability map](plan/capability-map.md)
owns feature coverage and [platform v1](plan/platform-v1.md) owns the implementation sequence.
Source implementation, release publication and live deployment are separate evidence gates.

## What is merged in source

| Change | Reviewable evidence | Boundary |
|---|---|---|
| Organizations, projects, roles, invitations, project keys and pooled allowances | [PR46](https://github.com/rrflow/rroflow-development/pull/46), [contract](spec/tenancy-v1.md) | No public signup or live tenant acceptance implied |
| Server receipt provenance and replacement ownership | [PR47](https://github.com/rrflow/rroflow-development/pull/47), [contract](spec/provenance-v1.md) | Historical records remain unknown; per-key rate limits and remaining client claims are incomplete |
| React organization/project/key management | [PR48](https://github.com/rrflow/rroflow-development/pull/48), `/app/manage` | First React feature; complete monitoring parity is still pending |
| Encrypted PostgreSQL federated browser sessions | [PR49](https://github.com/rrflow/rroflow-development/pull/49), [contract](spec/browser-sessions-v1.md) | Established sessions survive reconstruction; pending PKCE login attempts remain process-local |

PR49 exact head `87a8f82319006667ac9655ed4415b6fb65b6604f` passed
[CI 41532841203](https://github.com/rrflow/rroflow-development/actions/runs/41532841203)
and [release-build verification 41532841180](https://github.com/rrflow/rroflow-development/actions/runs/41532841180).
Its packaged-server job passed 44 desktop/phone browser scenarios. It merged as
`1d3da697e44939af6f84a13605384b8367464a34`. This did not publish or activate a new release.

The existing monitoring implementation also includes turn/evidence inspection,
built-in and custom signals, spend/limits, source-health and queue views, SDK activity,
reviews, consented metadata datasets and the source research library. Consult the
[capability map](plan/capability-map.md) for partial coverage and acceptance gaps;
there is no claim of full Raindrop/Sentry/PostHog feature parity.

## Native accounts and current work

[Native browser accounts](spec/native-accounts-v1.md): email ownership before password
creation, sign-in, recovery, persistent native sessions, encrypted mail outbox and
React account screens are merged in [PR50](https://github.com/rrflow/rroflow-development/pull/50).
Exact head `8c29848daaa91cb6baeda40d960600f01de48ba8` passed 188 Rust tests and all
46 packaged desktop/phone browser cases in
[CI 41537979270](https://github.com/rrflow/rroflow-development/actions/runs/41537979270),
plus [release-build verification](https://github.com/rrflow/rroflow-development/actions/runs/41537979222).
The merge is `08cf874e080f25b84d39ed79d90d20c13aabf59d`. This is fixture evidence. Production SMTP delivery and public-signup security review are
not qualified. Signup defaults off. Native CLI device authorization, explicit
account linking and MFA/passkeys remain open.

[Collector credential serialization](spec/collector-account-storage-v1.md) is under verification
to prevent competing commands from spending the same refresh token or undoing logout.

The live local review previously supplied to the owner is an older, labelled fixture
build. It is not evidence that the newest member console is deployed or that the
owner's Claude/Codex activity is being reliably captured.

## Published release and deployment

Latest published release verified through GitHub: [v0.4.0](https://github.com/rrflow/rroflow-development/releases/tag/v0.4.0),
published 2026-10-31 from `dac115de5b6077186f7511a738ca25561caaaa4d`.
It predates PR46-50. GitHub publication remains
gated by exact main-push CI, checksums and artifact verification.

**Current Texas deployment is not verified.** The October 26 report of a loopback
host installation is historical evidence only, preserved in this file's Git history.
It does not prove a reachable public/member service, current release or two-account
isolation. Do not reuse the obsolete Gold key instructions from that version.

Activation still needs trusted host access, confirmed application-VM placement,
verified backup/restore, the reviewed migration path, endpoint/identity configuration,
and real member/provider acceptance. See [Texas readiness](plan/texas-readiness.md)
and [private access references](plan/project-access-tooling.md). Secret values stay
outside the repository. A public URL is not a testable deployment until those checks pass.

## Next delivery gates

1. Select and qualify email delivery and complete the public-signup security review.
2. Finish native CLI authorization, explicit linking and turnkey account/project onboarding.
3. Complete the React monitoring console and remaining provenance/organization controls.
4. Prove reliable project/global agent and SDK capture with real provider sessions, including replay after outages.
5. Qualify Texas activation and two real isolated accounts through the released artifact.

Dataset production remains consented; labels do not grant training permission.
Supervised/unsupervised learning and reversible promotion retain the evaluation gates
in the [mechanism spec](spec/mechanism-spec-v0.md). BSL terms remain undecided; this
work does not change the current license.
