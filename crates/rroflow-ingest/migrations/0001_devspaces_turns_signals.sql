-- rroflow.ai ingest store, v1.
--
-- A devspace is one person's space on rroflow.ai. Its owner is a Nomen account
-- (issuer + subject). When that person grows into Shippin.cloud, the devspace is
-- attached to their Shippin tenant; the turns do not move.

CREATE TABLE devspaces (
    id                TEXT PRIMARY KEY CHECK (id ~ '^[A-Za-z0-9_-]{1,64}$'),
    -- Nomen identity that owns this devspace. NULL for devspaces registered by
    -- ingest key in the service config (before sign-in exists).
    owner_issuer      TEXT,
    owner_subject     TEXT,
    -- Nomen organisation (claim urn:zitadel:iam:user:resourceowner:id).
    owner_org         TEXT,
    -- Shippin.cloud tenant this devspace is attached to, once the user joins.
    shippin_tenant_id TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((owner_issuer IS NULL) = (owner_subject IS NULL))
);

CREATE UNIQUE INDEX devspaces_owner ON devspaces (owner_issuer, owner_subject)
    WHERE owner_subject IS NOT NULL;

-- One row per agent turn. Columns used for filtering and totals are lifted out of
-- the record; the full rroflow.turn/v1 record is kept in `record`.
CREATE TABLE turns (
    devspace_id        TEXT        NOT NULL REFERENCES devspaces (id) ON DELETE CASCADE,
    event_id           TEXT        NOT NULL CHECK (event_id <> ''),
    agent              TEXT        NOT NULL,
    provider           TEXT        NOT NULL,
    model              TEXT,
    project            TEXT,
    session_id         TEXT        NOT NULL,
    started_at         TIMESTAMPTZ,
    ended_at           TIMESTAMPTZ,
    termination        TEXT        NOT NULL,
    -- Token counts stay NULL when the provider did not report them: never priced as zero.
    input_tokens       BIGINT,
    output_tokens      BIGINT,
    cache_read_tokens  BIGINT,
    cache_write_tokens BIGINT,
    tool_calls         INTEGER     NOT NULL,
    tool_errors        INTEGER     NOT NULL,
    has_content        BOOLEAN     NOT NULL,
    record             JSONB       NOT NULL,
    received_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (devspace_id, event_id)
);

CREATE INDEX turns_recent  ON turns (devspace_id, started_at DESC NULLS LAST);
CREATE INDEX turns_session ON turns (devspace_id, session_id);

-- Signal hits, one row per hit, so signals can be counted and filtered in SQL.
-- Replaced as a set whenever their turn is re-sent.
CREATE TABLE signal_hits (
    devspace_id  TEXT             NOT NULL,
    event_id     TEXT             NOT NULL,
    ordinal      INTEGER          NOT NULL,
    signal       TEXT             NOT NULL,
    rule_version INTEGER          NOT NULL,
    severity     TEXT             NOT NULL,
    score        DOUBLE PRECISION,
    summary      TEXT             NOT NULL,
    PRIMARY KEY (devspace_id, event_id, ordinal),
    FOREIGN KEY (devspace_id, event_id) REFERENCES turns (devspace_id, event_id) ON DELETE CASCADE
);

CREATE INDEX signal_hits_by_signal ON signal_hits (devspace_id, signal);

-- Per-devspace signal choices (rroflow_core::signals::SignalConfig as JSON).
CREATE TABLE signal_settings (
    devspace_id TEXT        PRIMARY KEY REFERENCES devspaces (id) ON DELETE CASCADE,
    config      JSONB       NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
