CREATE TABLE IF NOT EXISTS model_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    model_id UUID NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    priority INTEGER NOT NULL DEFAULT 0,
    confidence_threshold REAL NOT NULL DEFAULT 0.25 CHECK (confidence_threshold >= 0 AND confidence_threshold <= 1),
    fps_limit REAL CHECK (fps_limit IS NULL OR fps_limit > 0),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (camera_id, model_id)
);
CREATE INDEX IF NOT EXISTS model_assignments_camera_priority_idx ON model_assignments (camera_id, priority DESC);

CREATE TABLE IF NOT EXISTS fusion_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    detection_id UUID NOT NULL REFERENCES detections(id) ON DELETE CASCADE,
    source_model_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    fused_confidence REAL NOT NULL CHECK (fused_confidence >= 0 AND fused_confidence <= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS fusion_results_camera_time_idx ON fusion_results (camera_id, created_at DESC);
