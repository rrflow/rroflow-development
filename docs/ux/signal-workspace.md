# Your monitoring workspace and signal library

Status: implementation on `codex/signal-workspace`, 2026-10-27. This is a review
build, not a claim of deployment or learning activation.

## Start with what matters

- **Overview:** activity received, signals worth looking at, and source history.
- **Findings:** the moments a rule picked out. Inspect the request, response and
  completed tools, then answer "Yes, this matches" or "No, it does not". The decision
  is tied to that exact captured revision. A match is not proof of failure.
- **Projects:** choose the project or folder you care about and open its activity.
- **Signal library:** browse built-in checks and organize your own rules in named
  groups. Filter by group, built-in/your signals, or a phrase in the description.
- **Datasets:** see the captured work and saved decisions that could support future
  datasets, and understand the permissions and evaluation steps still to build.
- **Improve:** the path from reviewing evidence to evaluated, reversible changes.

Raw IDs, provider fields and usage remain in the turn's Details tab. Research &
guides preserves the source notes, including the eight Raindrop documents.

## Create a signal without code

Choose **Signal library > Create a signal**. Give it a name and a group, then
choose one rule: a number of failed tools, a completed tool with a particular name,
a phrase in the message sent to the AI, or a phrase in its response. Limit it to
an agent or project if useful. Phrase matching is literal and ignores letter case;
no generated code, regular expression or model call runs.

**Try on recent activity** inspects up to 100 recent turns within the selected
agent/project scope. The result separates matches, checked turns, and turns whose
text was unavailable. Open a captured turn to inspect context. This is a preview,
not measured precision, and it changes no saved findings. You can save a rule even
when no recent examples match, after inspecting that result.

**Save and turn on** persists the rule and starts checking newly received turns.
Custom rules run on the server's received projection, so phrase checks require
opted-in, scrubbed uploaded text. Tool names and error counts need no prompt text.
The collector's built-in rules still evaluate locally before text is stripped.

**Pause** stops a custom rule from matching later arrivals. Existing findings stay
until their turn is re-sent; a re-sent turn is evaluated using current rules. No
history scan is silently started by saving. Saved review snapshots remain immutable
when capture changes. Definitions are immutable in this increment: create a new
signal to change a condition; pause the old one. Up to 32 definitions are supported.
Editing/version history and archiving are future usability work, not hidden buttons.

Built-in choices refresh in this collector version after successful delivery, at
most once per minute. Updated choices are cached durably for offline use and
reapplied on the next scan. Older collectors still need an upgrade/relink. Capture
continues when settings cannot refresh, using cached choices and reporting the
settings error separately. Closing a phone does not install or stop a source agent;
source startup and heartbeat acceptance remain separate work.

## Dataset and learning boundaries

The counts shown are received turns and the latest 1,000 saved decisions. They are
not counts of consented training rows. No dataset is published by this screen.
Supervised learning uses explicit corrections; unsupervised work proposes patterns
from eligible unlabelled examples. Neither starts from a click on a verdict.
Purpose-specific consent, fixed source snapshots, leakage-safe splits, durable
build jobs and held-out evaluation follow [automatic datasets v0](../spec/automatic-datasets-v0.md).
That contract remains the owner; the UI does not invent a training service.

## Contract and verification

`SignalConfig` adds defaulted `revision` and `custom` fields, using the existing
per-devspace settings stores; no new database or table is selected. Migration 0004
preserves existing built-in choices, adds default revision/custom fields and refuses
older Postgres writers that omit them. The changed migration fingerprint makes the
release activator refuse an unreviewed upgrade or downgrade across this boundary.
Take and verify a backup before migrating; restoring an older binary requires a
reviewed data restore, not bypassing that guard. JSONL deployments must obey the same
release boundary because their files have no database constraint. GET returns
the revision. PUT supplies the last-read revision, replaces the whole configuration
and returns the incremented revision. Stale writes receive 409. Old clients that
omit revision can write only the initial revision; they must GET before later writes.
JSONL serializes writes within the single service process; Postgres locks the owning
devspace row before comparing and writing. Multiple JSONL service processes over
one directory remain unsupported.

`POST /v1/signals/preview` accepts a bounded custom definition under the authenticated
account. Ingest owns the `custom_` namespace and discards submitted hits in that
namespace before computing current matches. Custom definitions keep version 1 and
are retained for interpreting evidence; only their enabled state can change.
Changing a rule needs a fresh identity. Saved reviews retain exact capture snapshots.

Acceptance covers isolated preview/ingest/restart, missing-text coverage, validation,
concurrent settings edits, collector cache identity/rollback refusal, and a real
server desktop/phone journey from creation through a new arrival, review and pause.
Fixture screenshots demonstrate UI behavior, not live provider coverage or Texas
activation. Full manual assistive-technology and real-user acceptance remain open.
