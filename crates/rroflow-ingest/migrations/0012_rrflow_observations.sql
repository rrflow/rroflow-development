-- Optional typed engine references add a strict-reader compatibility boundary.
-- No records are rewritten and this does not attest the source engine.
COMMENT ON TABLE capture_events IS
  'rroflow.capture/v1 with NativeFacts v1 and EngineObservation v1; readers must support RRFlow observation references before ingesting them';
