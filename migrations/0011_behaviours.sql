ALTER TABLE rule_conditions ADD COLUMN IF NOT EXISTS behaviour_type TEXT;

CREATE TABLE IF NOT EXISTS behaviours (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    object_class TEXT NOT NULL,
    behaviour_type TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    summary TEXT NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    recording_id UUID REFERENCES recordings(id) ON DELETE SET NULL,
    clip_id UUID REFERENCES clips(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS behaviours_camera_time_idx ON behaviours (camera_id, end_time DESC);
CREATE INDEX IF NOT EXISTS behaviours_type_time_idx ON behaviours (behaviour_type, end_time DESC);
CREATE UNIQUE INDEX IF NOT EXISTS behaviours_track_type_end_idx ON behaviours (track_id, behaviour_type, end_time);
