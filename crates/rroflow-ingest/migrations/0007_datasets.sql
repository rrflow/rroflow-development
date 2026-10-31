CREATE TABLE dataset_policies (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    purpose text NOT NULL CHECK (purpose IN ('signal_reviews','patterns')),
    revision bigint NOT NULL CHECK (revision > 0),
    enabled boolean NOT NULL,
    document jsonb NOT NULL,
    actor text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    next_scan_at timestamptz NOT NULL DEFAULT now(),
    last_input_digest text,
    last_source_count bigint,
    last_error text,
    last_scan_at timestamptz,
    last_exclusions jsonb NOT NULL DEFAULT '{}',
    PRIMARY KEY(devspace_id,purpose)
);
CREATE TABLE dataset_consent_history (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    purpose text NOT NULL,
    revision bigint NOT NULL,
    document jsonb NOT NULL,
    actor text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(devspace_id,purpose,revision)
);
CREATE TABLE dataset_jobs (
    devspace_id text NOT NULL REFERENCES devspaces(id),
    id text NOT NULL,
    purpose text NOT NULL,
    policy_revision bigint NOT NULL,
    status text NOT NULL CHECK (status IN ('queued','running','published','failed','revoked','expired')),
    input_digest text NOT NULL,
    input jsonb,
    artifact jsonb,
    artifact_digest text,
    row_count bigint NOT NULL,
    exclusions jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    published_at timestamptz,
    lease_token text,
    lease_until timestamptz,
    attempts integer NOT NULL DEFAULT 0,
    error_code text,
    PRIMARY KEY(devspace_id,id)
);
CREATE INDEX dataset_jobs_claim ON dataset_jobs(status,lease_until,created_at);
CREATE INDEX dataset_jobs_owner ON dataset_jobs(devspace_id,created_at DESC);
CREATE INDEX dataset_policies_due ON dataset_policies(next_scan_at,devspace_id,purpose) WHERE enabled;
