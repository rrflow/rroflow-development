-- New SDK metadata is optional for legacy events, but older strict readers
-- reject it. Mark the serialized-data compatibility boundary for release guards.
-- No observations are rewritten or deleted.
COMMENT ON TABLE capture_events IS
  'rroflow.capture/v1 with optional versioned native facts; readers must support NativeFacts v1 before accepting new SDK metadata';
