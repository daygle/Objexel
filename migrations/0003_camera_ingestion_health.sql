ALTER TABLE cameras
    ADD COLUMN last_connected_at TIMESTAMPTZ,
    ADD COLUMN last_snapshot_at TIMESTAMPTZ,
    ADD COLUMN last_error TEXT;

CREATE INDEX cameras_status_idx ON cameras (status);
