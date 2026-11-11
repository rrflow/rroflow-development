-- Settings an instance admin changes in the console. One row; a key that is absent uses its
-- default from the settings list in the service. Never holds keys, tokens or passwords.
CREATE TABLE instance_settings (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    document JSONB NOT NULL DEFAULT '{}'::jsonb,
    revision BIGINT NOT NULL DEFAULT 0,
    updated_by TEXT REFERENCES users (id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
