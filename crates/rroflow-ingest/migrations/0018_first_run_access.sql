-- The setup password is exposed by design: an installing AI may paste it into chat.
-- It works once, expires, and only its digest is stored. Claiming deletes the row.
CREATE TABLE instance_setup (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    context_sha256 TEXT NOT NULL CHECK (context_sha256 ~ '^[0-9a-f]{64}$'),
    token_sha256 TEXT NOT NULL CHECK (token_sha256 ~ '^[0-9a-f]{64}$'),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE instance_admins (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

-- The session that received a generated password, until it is saved or used to sign in.
ALTER TABLE native_credentials
    ADD COLUMN unsaved_session_sha256 TEXT CHECK (unsaved_session_sha256 ~ '^[0-9a-f]{64}$');
