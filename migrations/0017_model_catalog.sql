CREATE TABLE model_catalog (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    model_type TEXT NOT NULL,
    download_url TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    input_width INTEGER NOT NULL DEFAULT 640,
    input_height INTEGER NOT NULL DEFAULT 640,
    class_list JSONB NOT NULL DEFAULT '[]'::jsonb,
    archive_format TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE model_downloads (
    id UUID PRIMARY KEY,
    catalog_id TEXT NOT NULL REFERENCES model_catalog(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    progress_percent SMALLINT NOT NULL DEFAULT 0,
    bytes_downloaded BIGINT NOT NULL DEFAULT 0,
    total_bytes BIGINT,
    error TEXT,
    model_id UUID REFERENCES models(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX model_downloads_updated_idx ON model_downloads(updated_at DESC);
