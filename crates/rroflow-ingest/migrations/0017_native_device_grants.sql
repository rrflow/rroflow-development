-- Native collector grants are scoped to one project and capture operations.
CREATE TABLE native_device_requests (
    device_sha256 TEXT PRIMARY KEY CHECK (device_sha256 ~ '^[0-9a-f]{64}$'),
    user_code_sha256 TEXT NOT NULL UNIQUE CHECK (user_code_sha256 ~ '^[0-9a-f]{64}$'),
    context_sha256 TEXT NOT NULL CHECK (context_sha256 ~ '^[0-9a-f]{64}$'),
    client_id TEXT NOT NULL CHECK (octet_length(client_id) BETWEEN 1 AND 128),
    device_name TEXT NOT NULL CHECK (char_length(device_name) BETWEEN 1 AND 80),
    expires_at TIMESTAMPTZ NOT NULL,
    interval_seconds INTEGER NOT NULL CHECK (interval_seconds BETWEEN 5 AND 900),
    next_poll_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','denied','exchanged')),
    user_id TEXT REFERENCES native_credentials(user_id) ON DELETE CASCADE,
    credential_epoch BIGINT,
    project_id TEXT,
    grant_id TEXT,
    exchange_sha256 TEXT,
    response_key TEXT,
    response_nonce BYTEA,
    response_ciphertext BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((response_key IS NULL AND response_nonce IS NULL AND response_ciphertext IS NULL) OR
           (response_key IS NOT NULL AND response_nonce IS NOT NULL AND response_ciphertext IS NOT NULL AND octet_length(response_nonce)=12 AND octet_length(response_ciphertext) BETWEEN 17 AND 4096))
);
CREATE INDEX native_device_requests_expiry ON native_device_requests(expires_at);

CREATE TABLE native_device_grants (
    id TEXT PRIMARY KEY,
    context_sha256 TEXT NOT NULL CHECK (context_sha256 ~ '^[0-9a-f]{64}$'),
    client_id TEXT NOT NULL CHECK (octet_length(client_id) BETWEEN 1 AND 128),
    device_name TEXT NOT NULL CHECK (char_length(device_name) BETWEEN 1 AND 80),
    user_id TEXT NOT NULL REFERENCES native_credentials(user_id) ON DELETE CASCADE,
    credential_epoch BIGINT NOT NULL,
    project_id TEXT NOT NULL REFERENCES devspaces(id) ON DELETE CASCADE,
    access_sha256 TEXT NOT NULL UNIQUE CHECK (access_sha256 ~ '^[0-9a-f]{64}$'),
    access_expires_at TIMESTAMPTZ NOT NULL,
    refresh_sha256 TEXT NOT NULL UNIQUE CHECK (refresh_sha256 ~ '^[0-9a-f]{64}$'),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX native_device_grants_user ON native_device_grants(user_id);
CREATE INDEX native_device_grants_expiry ON native_device_grants(expires_at);

-- Keep spent token relationships to detect reuse. Only the latest response for
-- a grant retains encrypted credentials; older receipts keep hashes alone.
CREATE TABLE native_device_rotations (
    refresh_sha256 TEXT PRIMARY KEY CHECK (refresh_sha256 ~ '^[0-9a-f]{64}$'),
    grant_id TEXT NOT NULL REFERENCES native_device_grants(id) ON DELETE CASCADE,
    request_sha256 TEXT NOT NULL CHECK (request_sha256 ~ '^[0-9a-f]{64}$'),
    response_key TEXT,
    response_nonce BYTEA,
    response_ciphertext BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((response_key IS NULL AND response_nonce IS NULL AND response_ciphertext IS NULL) OR
           (response_key IS NOT NULL AND response_nonce IS NOT NULL AND response_ciphertext IS NOT NULL AND octet_length(response_nonce)=12 AND octet_length(response_ciphertext) BETWEEN 17 AND 4096))
);
CREATE INDEX native_device_rotations_grant ON native_device_rotations(grant_id);
