-- Email ownership is established BEFORE a password or account exists.
CREATE TABLE native_credentials (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    email TEXT NOT NULL UNIQUE CHECK (octet_length(email) BETWEEN 3 AND 254 AND email=lower(email)),
    password_phc TEXT NOT NULL CHECK (octet_length(password_phc) BETWEEN 40 AND 512),
    credential_epoch BIGINT NOT NULL DEFAULT 1 CHECK (credential_epoch > 0),
    verified_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    changed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE native_actions (
    token_sha256 TEXT PRIMARY KEY CHECK (token_sha256 ~ '^[0-9a-f]{64}$'),
    context_sha256 TEXT NOT NULL CHECK (context_sha256 ~ '^[0-9a-f]{64}$'),
    email TEXT NOT NULL CHECK (octet_length(email) BETWEEN 3 AND 254 AND email=lower(email)),
    purpose TEXT NOT NULL CHECK (purpose IN ('register','reset')),
    credential_epoch BIGINT,
    expires_at TIMESTAMPTZ NOT NULL CHECK (expires_at = date_trunc('second', expires_at)),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((purpose='register' AND credential_epoch IS NULL) OR (purpose='reset' AND credential_epoch > 0))
);
CREATE INDEX native_actions_email ON native_actions(context_sha256,email);
CREATE INDEX native_actions_expiry ON native_actions(expires_at);

-- The mail contains a bearer link, so it is encrypted even in the durable queue.
-- A transaction holds the row lock while sending; a crash can retry the same mail.
CREATE TABLE native_mail_outbox (
    token_sha256 TEXT PRIMARY KEY REFERENCES native_actions(token_sha256) ON DELETE CASCADE,
    key_id TEXT NOT NULL CHECK (octet_length(key_id) BETWEEN 1 AND 64),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce)=12),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 17 AND 4096),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    sent_at TIMESTAMPTZ
);
CREATE INDEX native_mail_pending ON native_mail_outbox(next_attempt_at) WHERE sent_at IS NULL;

CREATE TABLE native_sessions (
    session_sha256 TEXT PRIMARY KEY CHECK (session_sha256 ~ '^[0-9a-f]{64}$'),
    context_sha256 TEXT NOT NULL CHECK (context_sha256 ~ '^[0-9a-f]{64}$'),
    user_id TEXT NOT NULL REFERENCES native_credentials(user_id) ON DELETE CASCADE,
    credential_epoch BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX native_sessions_user ON native_sessions(user_id,created_at);
CREATE INDEX native_sessions_expiry ON native_sessions(expires_at);

-- Bounds persist across serving instances/restarts. Keys contain only digests.
CREATE TABLE native_rate_windows (
    scope TEXT NOT NULL,
    subject_sha256 TEXT NOT NULL CHECK (subject_sha256 ~ '^[0-9a-f]{64}$'),
    expires_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts > 0),
    PRIMARY KEY(scope,subject_sha256)
);
