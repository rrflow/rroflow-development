-- Organizations sit above the existing storage partition: a project IS a devspace, so no
-- monitoring record moves. `owner_issuer`/`owner_subject` keep meaning "this person's personal
-- default project", the one a credential opens when a request names no project. Devspaces
-- registered by ingest key stay outside any organization until an operator claims them.

CREATE TABLE users (
    id           TEXT PRIMARY KEY CHECK (id ~ '^u_[0-9a-f]{32}$'),
    -- Display only; never used to decide access.
    display_name TEXT CHECK (char_length(display_name) <= 200),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- How a person proves who they are. One user may hold several.
CREATE TABLE user_identities (
    issuer     TEXT        NOT NULL,
    subject    TEXT        NOT NULL,
    user_id    TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (issuer, subject)
);

CREATE INDEX user_identities_user ON user_identities (user_id);

CREATE TABLE organizations (
    id         TEXT PRIMARY KEY CHECK (id ~ '^o_[0-9a-f]{32}$'),
    slug       TEXT        NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,47}$'),
    name       TEXT        NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    -- Created at first sign-in. It always has exactly one member, its creator, so the
    -- project a sign-in opens by default can never be taken away from that person.
    personal   BOOLEAN     NOT NULL DEFAULT false,
    created_by TEXT        REFERENCES users (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX organizations_created_by ON organizations (created_by) WHERE NOT personal;

CREATE TABLE organization_members (
    organization_id TEXT        NOT NULL REFERENCES organizations (id),
    user_id         TEXT        NOT NULL REFERENCES users (id),
    role            TEXT        NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, user_id)
);

CREATE INDEX organization_members_user ON organization_members (user_id);

ALTER TABLE devspaces
    ADD COLUMN organization_id TEXT REFERENCES organizations (id),
    ADD COLUMN slug            TEXT CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,47}$'),
    ADD COLUMN name            TEXT CHECK (char_length(name) BETWEEN 1 AND 100);

-- Every existing account gets a personal organization holding its devspace.
DO $$
DECLARE
    d RECORD;
    new_user TEXT;
    new_org TEXT;
BEGIN
    FOR d IN SELECT id, owner_issuer, owner_subject FROM devspaces WHERE owner_subject IS NOT NULL LOOP
        new_user := 'u_' || replace(gen_random_uuid()::text, '-', '');
        new_org := 'o_' || replace(gen_random_uuid()::text, '-', '');
        INSERT INTO users (id) VALUES (new_user);
        INSERT INTO user_identities (issuer, subject, user_id)
            VALUES (d.owner_issuer, d.owner_subject, new_user);
        INSERT INTO organizations (id, slug, name, personal, created_by)
            VALUES (new_org, 'personal-' || substr(new_org, 3, 12), 'Personal', true, new_user);
        INSERT INTO organization_members (organization_id, user_id, role)
            VALUES (new_org, new_user, 'owner');
        UPDATE devspaces SET organization_id = new_org, slug = 'default', name = 'Default project'
            WHERE id = d.id;
    END LOOP;
END $$;

ALTER TABLE devspaces
    ADD CONSTRAINT devspaces_owner_in_organization
        CHECK (owner_subject IS NULL OR organization_id IS NOT NULL),
    ADD CONSTRAINT devspaces_project_identity
        CHECK ((organization_id IS NULL) = (slug IS NULL) AND (organization_id IS NULL) = (name IS NULL));

CREATE UNIQUE INDEX devspaces_organization_slug ON devspaces (organization_id, slug)
    WHERE organization_id IS NOT NULL;

-- Write-only project keys. Only a digest is kept; the key is shown once.
CREATE TABLE project_keys (
    id           TEXT PRIMARY KEY CHECK (id ~ '^k_[0-9a-f]{32}$'),
    devspace_id  TEXT        NOT NULL REFERENCES devspaces (id) ON DELETE CASCADE,
    label        TEXT        NOT NULL CHECK (char_length(label) BETWEEN 1 AND 100),
    key_sha256   TEXT        NOT NULL UNIQUE CHECK (key_sha256 ~ '^[0-9a-f]{64}$'),
    -- The start of the key, so a person can recognise it in a list.
    prefix       TEXT        NOT NULL,
    created_by   TEXT        NOT NULL REFERENCES users (id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,
    revoked_by   TEXT REFERENCES users (id)
);

CREATE INDEX project_keys_project ON project_keys (devspace_id, created_at DESC, id);

-- Invitations never grant `owner`; an owner promotes a member explicitly.
CREATE TABLE organization_invitations (
    id              TEXT PRIMARY KEY CHECK (id ~ '^i_[0-9a-f]{32}$'),
    organization_id TEXT        NOT NULL REFERENCES organizations (id),
    role            TEXT        NOT NULL CHECK (role IN ('admin', 'member', 'viewer')),
    token_sha256    TEXT        NOT NULL UNIQUE CHECK (token_sha256 ~ '^[0-9a-f]{64}$'),
    created_by      TEXT        NOT NULL REFERENCES users (id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    accepted_by     TEXT REFERENCES users (id),
    accepted_at     TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ
);

CREATE INDEX organization_invitations_org ON organization_invitations (organization_id, created_at DESC, id);

-- Who changed what, written in the transaction of the change. Rows are only ever inserted.
-- A NULL actor is an operator action. `detail` never holds keys, tokens or record content.
CREATE TABLE audit_events (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    organization_id TEXT        NOT NULL REFERENCES organizations (id),
    actor_user_id   TEXT REFERENCES users (id),
    action          TEXT        NOT NULL,
    target          TEXT,
    detail          JSONB       NOT NULL DEFAULT '{}'::jsonb,
    at              TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX audit_events_organization ON audit_events (organization_id, id DESC);
