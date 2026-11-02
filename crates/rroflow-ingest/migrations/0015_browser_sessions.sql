-- Opaque cookies are never stored; provider credentials are encrypted outside SQL.
-- The service's private key ring is separate from this database and its backups.
CREATE TABLE browser_sessions (
    context_sha256 TEXT NOT NULL CHECK (context_sha256 ~ '^[0-9a-f]{64}$'),
    session_sha256 TEXT NOT NULL CHECK (session_sha256 ~ '^[0-9a-f]{64}$'),
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_id TEXT NOT NULL CHECK (char_length(key_id) BETWEEN 1 AND 64),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 12),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 17 AND 65552),
    expires_at TIMESTAMPTZ NOT NULL CHECK (expires_at = date_trunc('second', expires_at)),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (context_sha256, session_sha256)
);
CREATE INDEX browser_sessions_expiry ON browser_sessions(context_sha256, expires_at);
CREATE INDEX browser_sessions_user ON browser_sessions(user_id);
