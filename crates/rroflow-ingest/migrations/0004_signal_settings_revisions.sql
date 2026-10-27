-- Preserve existing built-in choices while introducing revisioned custom definitions.
-- This is also a release compatibility boundary: older writers must fail rather
-- than silently replacing settings with a shape that discards custom definitions.
UPDATE signal_settings
SET config = config || jsonb_build_object(
    'revision', COALESCE(config->'revision', '0'::jsonb),
    'custom', COALESCE(config->'custom', '[]'::jsonb)
);

ALTER TABLE signal_settings ADD CONSTRAINT signal_settings_revision_v1 CHECK (
    config ? 'revision' AND config ? 'custom'
    AND jsonb_typeof(config->'revision') = 'number'
    AND jsonb_typeof(config->'custom') = 'array'
);
