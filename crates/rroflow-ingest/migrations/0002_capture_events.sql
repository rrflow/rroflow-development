CREATE TABLE capture_events (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    event_id text NOT NULL,
    capture_id text NOT NULL,
    sequence bigint NOT NULL CHECK (sequence >= 0),
    agent text NOT NULL,
    observed_at timestamptz NOT NULL,
    record jsonb NOT NULL,
    PRIMARY KEY (devspace_id, event_id),
    UNIQUE (devspace_id, capture_id, sequence)
);
CREATE INDEX capture_events_recent ON capture_events(devspace_id, observed_at DESC, event_id);
