-- No backfill: the receiver cannot reconstruct credentials for old records.
CREATE TABLE record_provenance (
    devspace_id TEXT NOT NULL REFERENCES devspaces(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('turn', 'event', 'source')),
    record_id TEXT NOT NULL,
    receipt JSONB NOT NULL,
    PRIMARY KEY (devspace_id, kind, record_id)
);
