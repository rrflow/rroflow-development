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

For the unique messages accumulated into a turn, a field is a complete total only when
every counted message reports that field and its sum fits in `u64`. One missing
field makes that total unknown; a later report does not repair the missing
earlier value. Other complete fields remain available. Overflow becomes unknown
in the hosted projection rather than wrapping, panicking or becoming zero.

Claude assistant messages participate in completeness checking even when their
usage object is absent. Legacy Codex `last_token_usage` objects participate when
present; null informational token-count notifications are not an additional usage
report. The existing `input_includes_cache` distinction is preserved.

## Claude message attribution

Hosted input/cache totals count each session/message ID once, following
[Anthropic's usage guidance](https://code.claude.com/docs/en/agent-sdk/cost-tracking).
Repeated assistant blocks for parallel tools do not add usage again. The guidance
identifies per-step output as a placeholder; it stays unknown here until an
authoritative turn-result mapping exists.

Original text/tool records remain intact. Identical replay retains its first
turn owner. Missing IDs, mismatched sessions or conflicting counts make affected
totals unknown. IDs in different sessions are independent. This is attribution
within a parsed transcript, not a claim of global deduplication across arbitrary
copied files or a change to the SDK wrapper's separate native-event contract.

## Codex response attribution

When a rollout has its own thread's `token_usage_record` entries, hosted usage
comes exclusively from those response records. The mapping is pinned to the
[Codex protocol at 67f0994](https://github.com/openai/codex/blob/67f0994b532b8740f14b5b738af7b7b2a915b1d6/codex-rs/protocol/src/protocol.rs)
and its [rollout usage tests](https://github.com/openai/codex/blob/67f0994b532b8740f14b5b738af7b7b2a915b1d6/codex-rs/core/tests/suite/token_usage_rollout.rs).

Each response ID is counted once and joined to its explicit turn ID in its own
thread. Late records do not charge whichever turn happens to be last. Identical
replays are ignored; a conflicting response ID makes both affected turns unknown.
Two distinct responses with identical counts both count. Missing response IDs or
duplicate turn identities cannot produce a known total. Foreign thread records
are ignored and inherited parent history remains excluded by the rollout parser.

Only response `usage` is summed; notification, turn and thread cumulative totals
are not added. The modern `cache_write_input_tokens` field is retained when
reported. Missing or overflowing fields remain unknown independently. A turn
without its own response records stays unknown in a modern rollout, including an
older turn in mixed historical data; it does not fall back to notification sums.

## Boundaries

The legacy Raindrop exporter keeps its existing numeric projection for valid
inputs, including its historical zero placeholders. It does not provide the
hosted projection's completeness guarantee. Its payload compatibility is tested
separately; invalid numeric overflow is now bounded. Do not use that legacy
projection as proof of complete spend coverage.

Old Codex rollouts with no response-specific records still use the legacy
notification mapping with explicit field completeness. Duplicate notifications
and cumulative-counter resets in that older format remain an open attribution
gap. Authoritative Claude output totals, mixed-model turns and usage never exposed by
either source also need separate coverage before a complete cost claim. No price
table or subscription bill is inferred.

Existing stored records are not silently rewritten. Re-reading and ingesting a
source can replace its deterministic turn record using the corrected projection;
previous review snapshots retain the evidence originally reviewed.

## Verification

Collector tests cover known models without usage, usage without model names,
explicit zeros, missing/malformed fields, partial reports and overflow. Both
bounded and unbounded transcript readers are exercised. The Raindrop compatibility
test compares full event/span payloads before and after observation tracking.
The collector's durable delivery and account-binding tests run in the same suite.
Response tests additionally cover identity conflicts, duplicate/equal-count
responses, late arrival, foreign threads and cumulative-total exclusion.

An isolated local dry run on 2026-10-31 compared 52 turns from three real Codex
logs with an independent aggregation of 1,280 unique response records. All
compared usage fields matched. It used an empty configuration, no credentials,
no upload and a temporary metadata dump that was removed. This is source-parser
evidence, not a hosted member-capture or provider-billing test.

A second isolated audit compared four stable Claude logs: 393 assistant blocks,
188 unique usage records and 205 repeated blocks excluded from usage totals. All
input/cache fields matched independent aggregation; output remained unknown.
Parallel-tool fixtures prove that usage deduplication retains the tool calls.
