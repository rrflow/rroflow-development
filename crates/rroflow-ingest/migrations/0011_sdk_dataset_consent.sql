-- New optional SDK source consent and v2 typed dataset inputs need new readers.
-- Existing permissions are never broadened; absent sdk_agents means none.
COMMENT ON TABLE dataset_policies IS 'Dataset consent with optional explicit sdk_agents; v2 SDK metadata inputs require compatible workers';
