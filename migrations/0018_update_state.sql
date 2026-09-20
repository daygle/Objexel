CREATE TABLE update_state (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    latest_version TEXT,
    release_url TEXT,
    notes TEXT,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
