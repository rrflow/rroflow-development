-- Latest-only observations; received activity is not a source heartbeat.
CREATE TABLE source_health (
    devspace_id TEXT NOT NULL REFERENCES devspaces(id),
    source_id TEXT NOT NULL,
    observation JSONB NOT NULL,
    PRIMARY KEY (devspace_id, source_id)
);
