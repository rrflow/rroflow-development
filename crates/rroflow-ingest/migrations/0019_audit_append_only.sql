-- Audit rows are evidence. The service only ever inserts them; from here the database also
-- refuses to update, delete or truncate them, whoever asks.
CREATE FUNCTION audit_events_refuse_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_events is append-only'
        USING ERRCODE = 'insufficient_privilege';
END;
$$;

CREATE TRIGGER audit_events_append_only
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION audit_events_refuse_change();

CREATE TRIGGER audit_events_no_truncate
    BEFORE TRUNCATE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION audit_events_refuse_change();
