-- Allowances are operational policy, not billing or consent. Only the service
-- operator writes these tables; member HTTP routes are read-only.
CREATE TABLE allowance_policy (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    revision bigint NOT NULL CHECK (revision > 0),
    document jsonb NOT NULL
);
CREATE TABLE allowance_grants (
    devspace_id text PRIMARY KEY REFERENCES devspaces(id),
    revision bigint NOT NULL CHECK (revision > 0),
    document jsonb NOT NULL
);
-- Independent of mutable turns and future retention: replay never buys another
-- identity. NULL marks pre-migration history, which is not billed retroactively.
CREATE TABLE ingest_identities (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    kind text NOT NULL CHECK (kind IN ('turns', 'events')),
    event_id text NOT NULL,
    accepted_at timestamptz,
    PRIMARY KEY (devspace_id, kind, event_id)
);
INSERT INTO ingest_identities (devspace_id,kind,event_id)
    SELECT devspace_id,'turns',event_id FROM turns;
INSERT INTO ingest_identities (devspace_id,kind,event_id)
    SELECT devspace_id,'events',event_id FROM capture_events;
CREATE INDEX ingest_identity_usage ON ingest_identities(devspace_id, accepted_at, kind);
