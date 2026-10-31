# Transcript turn usage completeness

Status: implemented in development source; see the containing pull request for
validation. This corrects hosted `rroflow.turn/v1` projections from Claude Code
and Codex transcript capture. It does not change SDK native-event projections,
add pricing or prove provider billing accuracy.

## Contract

A model name is independent of usage availability. Each input, output,
cache-read and cache-write count is optional. An explicitly reported integer
zero stays zero. An absent, null, negative, non-integer or otherwise malformed
count is unknown. Reported usage survives even if the model name is absent.

For the messages accumulated into a turn, a field is a complete total only when
every counted message reports that field and its sum fits in `u64`. One missing
field makes that total unknown; a later report does not repair the missing
earlier value. Other complete fields remain available. Overflow becomes unknown
in the hosted projection rather than wrapping, panicking or becoming zero.

Claude assistant records participate in completeness checking even when their
usage object is absent. Codex `last_token_usage` objects participate when present;
null informational token-count notifications are not an additional usage report.
Codex does not report cache writes through this mapping, so that field remains
unknown. The existing `input_includes_cache` distinction is preserved.

## Boundaries

The legacy Raindrop exporter keeps its existing numeric projection for valid
inputs, including its historical zero placeholders. It does not provide the
hosted projection's completeness guarantee. Its payload compatibility is tested
separately; invalid numeric overflow is now bounded. Do not use that legacy
projection as proof of complete spend coverage.

This correction does not resolve duplicate provider usage notifications,
cumulative-total attribution, repeated Claude message IDs, mixed-model turns or
usage never exposed by the source. Those require separate source-version tests
before a complete cost claim. No price table or subscription bill is inferred.

Existing stored records are not silently rewritten. Re-reading and ingesting a
source can replace its deterministic turn record using the corrected projection;
previous review snapshots retain the evidence originally reviewed.

## Verification

Collector tests cover known models without usage, usage without model names,
explicit zeros, missing/malformed fields, partial reports and overflow. Both
bounded and unbounded transcript readers are exercised. The Raindrop compatibility
test compares full event/span payloads before and after observation tracking.
The collector's durable delivery and account-binding tests run in the same suite.
