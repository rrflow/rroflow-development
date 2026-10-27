-- Immutable human verdicts retain the exact evidence they reviewed.
CREATE TABLE signal_reviews (
    devspace_id TEXT NOT NULL REFERENCES devspaces(id),
    review_id TEXT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    record JSONB NOT NULL,
    PRIMARY KEY(devspace_id, review_id)
);
CREATE INDEX signal_reviews_recent ON signal_reviews(devspace_id, recorded_at DESC);
