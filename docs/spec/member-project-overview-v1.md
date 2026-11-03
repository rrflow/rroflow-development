# Member project monitoring overview v1

Status: implemented with local PostgreSQL and 54 desktop/phone browser checks passing. This is the first monitoring
view in the React member console, not completion of the dashboard migration or
evidence of deployment.

## A project's home

The selected project's page at `/app/manage` now shows captured transcript turns,
indexed SDK activity, and collectors with saved delivery reports. These counts
remain separate: transcript and SDK instrumentation can observe overlapping work.
Every activity count, agent breakdown and signal match opens the corresponding
evidence view with the same workspace and filters. The existing evidence,
signals, sources and dataset screens remain until their React parity work passes.

Signal panels show the most common stored matches in each kind of activity.
Matches are review candidates, not confirmed failures or training labels. The
transcript panel retains custom signal names and links to the signal workspace.
SVG bars use local semantic colors and add no third-party chart or image runtime.
The data labels remain text so the visual is not the only representation.

Delivery cards show up to four saved reports and link to all source details.
Reports age using the server's `as_of`, its freshness interval, and elapsed time
since loading. Refresh requests newer observations; a timer never invents new
reports. A recent report with a failed delivery check, capture error, retrying SDK
queue or SDK queue error is labelled as a problem. Missing SDK counts remain
unreported. Collector approval is never used as proof of arrival or uptime.

## Data boundary

React uses same-origin Rust endpoints with the selected `X-Rroflow-Project`:

- `/v1/summary`: received transcript counts, agents, signal matches and usage
  coverage, with the current custom signal names.
- `/v1/sdk/summary`: new metadata-only aggregate over published SDK activity.
- `/v1/sources/health`: bounded latest collector observations.

The SDK aggregate reads relational metadata rather than serialized activity,
prompt, response or tool payloads. One read-only repeatable-read transaction
provides activity totals, activities with findings, up to twelve agent buckets,
up to twelve signal buckets, the omitted agent count and indexing coverage.
Signal buckets count distinct matching activities. Failed indexing keeps the
last published counts visible and reports pending/error captures separately.
Unsupported storage returns `available: false`, not a zero count.

Every response's workspace must match the requested project. Unknown shapes,
missing or unsafe counts, invalid timestamps, and request failures produce
unavailable states. An expired session clears the member view. Aborted responses
cannot repopulate a previous project. These requests never load conversations
just to produce overview metrics. Authorization and project membership remain
server responsibilities; the browser check adds protection against misrouted or
stale responses.

## Ownership and proof

Authored UI lives in `console/src/project-overview.tsx` and its validated consumer
types in `console/src/monitoring-api.ts`. Existing semantic styles remain in
`console/src/styles.css`. `npm --prefix console run export` builds and embeds the
bundle; generated files are not hand-edited. Rust owns the aggregate in
`sdk_activity/overview.rs`.

PostgreSQL tests cover isolation, aggregate counts, indexing backlog and errors.
Packaged-service browser tests cover real received fixture activity, scoped
drill-downs, no content requests, unavailable/foreign/stale responses,
late responses after project changes, session expiry, accessibility, phone layout
and dark appearance. Screenshots are fixture proof.
The first UI run found accessible-name mismatches on metric links; the authored
links were corrected to use their complete visible text.

Remaining platform work includes full React evidence/signal/source/dataset
parity, the generated OpenAPI client, integrated onboarding, real provider
qualification and hosted acceptance. This overview does not claim spend totals,
subscription coverage or automatic learning that its data sources do not prove.
