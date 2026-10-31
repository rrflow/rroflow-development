-- Durable projection work is separate from immutable events and ingest allowances.
CREATE TABLE sdk_activity_work (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    capture_id text NOT NULL,
    source_revision bigint NOT NULL DEFAULT 1 CHECK(source_revision > 0),
    published_revision bigint,
    status text NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','ready','error')),
    attempts integer NOT NULL DEFAULT 0,
    lease_token text,
    lease_until timestamptz,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    changed_at timestamptz NOT NULL DEFAULT now(),
    last_error text,
    PRIMARY KEY(devspace_id,capture_id)
);
CREATE INDEX sdk_activity_work_pending ON sdk_activity_work(next_attempt_at,changed_at) WHERE status <> 'ready';
CREATE INDEX capture_events_projection ON capture_events(devspace_id,capture_id,sequence);
INSERT INTO sdk_activity_work(devspace_id,capture_id)
    SELECT DISTINCT devspace_id,capture_id FROM capture_events;
CREATE TABLE sdk_activities (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    activity_id text NOT NULL,
    capture_id text NOT NULL,
    source_revision bigint NOT NULL,
    revision text NOT NULL,
    agent text NOT NULL,
    session_id text NOT NULL,
    outcome text NOT NULL,
    first_observed_at timestamptz NOT NULL,
    signals text[] NOT NULL,
    search_text text NOT NULL,
    record jsonb NOT NULL,
    published_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(devspace_id,activity_id),
    FOREIGN KEY(devspace_id,capture_id) REFERENCES sdk_activity_work(devspace_id,capture_id)
);
CREATE INDEX sdk_activities_recent ON sdk_activities(devspace_id,first_observed_at DESC,activity_id);
CREATE INDEX sdk_activities_capture ON sdk_activities(devspace_id,capture_id);
CREATE TABLE sdk_activity_reviews (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    review_id text NOT NULL,
    activity_id text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT now(),
    record jsonb NOT NULL,
    PRIMARY KEY(devspace_id,review_id)
);

CREATE INDEX sdk_reviews_activity ON sdk_activity_reviews(devspace_id,activity_id,recorded_at DESC,review_id);
