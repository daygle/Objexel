ALTER TABLE cameras
    ADD COLUMN IF NOT EXISTS last_connected_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_snapshot_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error TEXT;

CREATE INDEX IF NOT EXISTS cameras_status_idx ON cameras (status);
