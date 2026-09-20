ALTER TABLE models
    ADD COLUMN model_type TEXT NOT NULL DEFAULT 'yolo',
    ADD COLUMN input_width INTEGER NOT NULL DEFAULT 640,
    ADD COLUMN input_height INTEGER NOT NULL DEFAULT 640,
    ADD COLUMN class_list JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN default_model BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE camera_models (
    camera_id UUID PRIMARY KEY REFERENCES cameras(id) ON DELETE CASCADE,
    model_id UUID NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX camera_models_model_idx ON camera_models (model_id);

CREATE TABLE benchmark_results (
    id UUID PRIMARY KEY,
    model_id UUID NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    fps REAL NOT NULL,
    average_inference_time_ms REAL NOT NULL,
    gpu_memory_usage_mb BIGINT,
    cpu_usage_percent REAL,
    test_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX benchmark_results_model_time_idx ON benchmark_results (model_id, test_timestamp DESC);
