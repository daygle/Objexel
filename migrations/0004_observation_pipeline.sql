ALTER TABLE models
    ADD COLUMN active BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE detections
    ADD COLUMN track_id UUID,
    ADD COLUMN object_class TEXT,
    ADD COLUMN bbox_x REAL,
    ADD COLUMN bbox_y REAL,
    ADD COLUMN bbox_width REAL,
    ADD COLUMN bbox_height REAL;

UPDATE detections SET object_class = label;

ALTER TABLE detections
    ALTER COLUMN object_class SET NOT NULL,
    ALTER COLUMN confidence SET NOT NULL,
    ADD CONSTRAINT detections_confidence_range CHECK (confidence >= 0 AND confidence <= 1),
    ADD CONSTRAINT detections_bbox_non_negative CHECK (bbox_width IS NULL OR (bbox_width >= 0 AND bbox_height >= 0));

CREATE TABLE tracks (
    id UUID PRIMARY KEY,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    object_class TEXT NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    duration_ms BIGINT NOT NULL DEFAULT 0,
    movement_path JSONB NOT NULL DEFAULT '[]'
);

ALTER TABLE detections ADD CONSTRAINT detections_track_fk FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE SET NULL;
CREATE INDEX detections_track_idx ON detections (track_id, observed_at DESC);
CREATE INDEX tracks_camera_active_idx ON tracks (camera_id, last_seen DESC);

CREATE TABLE observations (
    id UUID PRIMARY KEY,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    observation_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX observations_camera_time_idx ON observations (camera_id, created_at DESC);
CREATE INDEX observations_track_idx ON observations (track_id, created_at DESC);
