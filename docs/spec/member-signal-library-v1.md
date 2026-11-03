# Member signal library v1

Status: implemented with 62 local desktop/phone cases passing; hosted qualification pending. This advances React monitoring parity;
it does not replace the existing evidence dashboard or establish hosted activation.

## Find, try and use a signal

A project's overview links to its signal library in the member console. The
library groups built-in transcript checks, the member project's custom rules,
and received signal IDs whose definitions are unavailable. Search, group and
kind filters retain historical findings. The compact desktop table follows the
[owner reference](../ux/owner-hosted-service-reference.md), with row definitions
behind disclosure controls and a stacked phone layout. Counts are received transcript matches,
not an accuracy measure or an estimate of unobserved activity. SDK findings and
reviews remain linked through the SDK activity screen.

A custom rule has a name, group and one bounded predicate: failed-tool count,
completed tool name, prompt phrase or response phrase. The optional agent and
source-project-label filters narrow activity inside the selected member project;
they never change account ownership or select a different member project.

The builder requires a successful preview of the current definition before save.
Editing any field invalidates that preview. A late response for a previous draft
cannot enable saving. The server checks at most the newest 100 scoped turns,
reports unavailable text separately, and tells the member when more history
exists. Text predicates require opted-in uploaded text. No match is an honest
result and does not prevent saving for future activity. Preview evidence links
open the exact captured turn in the same member project.

Saved custom definitions remain immutable so earlier findings retain their
meaning. Pause and resume change only future evaluation; modifying a predicate
requires a new signal. The current service supports 32 custom definitions.
Built-in checks can be paused or resumed; their tunable ranges and defaults come
from the Rust catalogue. Connected collectors apply those choices when they next
refresh settings. Neither operation rewrites historical findings or reviews.

## Consistency and access

The Rust summary, settings and preview routes remain authoritative. Requests
carry the selected `X-Rroflow-Project`. Preview responses now include `devspace`,
an additive field also checked by the React consumer. Unreadable or foreign
responses fail closed; the UI never fabricates an empty library or a successful
preview. All content renders as text, including names, groups and summaries.

Settings writes retain the server's revision token. A concurrent change returns
a conflict; the UI keeps the draft and requires reloading current settings before
retrying. A retry merges only the intended new rule or check choices into the
fresh configuration. It never silently replaces another editor's choices.
Project navigation aborts requests and removes dialogs. Session expiry clears
private member views. Viewers can inspect evidence but have no mutation controls;
the server also refuses their writes.

## Ownership and proof

Authored components and response validation live in `console/src/signal-library.tsx`
and `console/src/signal-api.ts`; existing theme variables supply visual styling.
The console is compiled to static files embedded in Rust. No new production
runtime, provider keys, cloud endpoint or external visual dependency is added.

`member-signals.spec.mjs` exercises the actual Rust service and disposable
PostgreSQL membership. It covers preview/save/new-arrival/evidence/pause,
concurrent settings, built-in thresholds, invalidated and foreign previews,
viewer enforcement, session expiry, desktop/phone layout and accessibility.
The backend custom-rule journey checks preview ownership and both supported
storage implementations. Fixture screenshots are development evidence only.

Remaining parity work includes native React finding/evidence inspection, source
and dataset screens, generated API clients, integrated onboarding and real
provider/hosted acceptance. Signals remain review candidates; this feature does
not turn a match or preview into a training label or a promoted agent change.
