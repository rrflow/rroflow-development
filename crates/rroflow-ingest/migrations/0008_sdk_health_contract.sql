-- This data-format migration marks a reader compatibility boundary, not a backfill.
-- Older SourceReport readers deny unknown fields and cannot read SDK observations.
-- Advancing the existing migration fingerprint makes managed release activation
-- require an explicit migration/restore procedure before changing reader versions.
-- No source observations or customer records are rewritten or discarded.
COMMENT ON TABLE source_health IS
  'rroflow.source-health/v1 observations may include sdk and session_capture fields; use an SDK-aware reader or restore a compatible pre-activation backup';
