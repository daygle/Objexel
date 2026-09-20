CREATE TABLE IF NOT EXISTS zones (
    id UUID PRIMARY KEY,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    polygon_coordinates JSONB NOT NULL,
    colour TEXT NOT NULL DEFAULT '#74e0b4',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT zones_polygon_not_empty CHECK (jsonb_array_length(polygon_coordinates) >= 3)
);

CREATE INDEX zones_camera_enabled_idx ON zones (camera_id, enabled);

CREATE TABLE IF NOT EXISTS zone_events (
    id UUID PRIMARY KEY,
    zone_id UUID NOT NULL REFERENCES zones(id) ON DELETE CASCADE,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK (event_type IN ('entered', 'exited', 'occupied')),
    occurred_at TIMESTAMPTZ NOT NULL,
    duration_ms BIGINT
);

CREATE INDEX zone_events_camera_time_idx ON zone_events (camera_id, occurred_at DESC);
CREATE INDEX zone_events_zone_time_idx ON zone_events (zone_id, occurred_at DESC);
