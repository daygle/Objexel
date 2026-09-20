CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER cameras_set_updated_at
BEFORE UPDATE ON cameras
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();
