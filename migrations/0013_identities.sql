ALTER TABLE rule_conditions ADD COLUMN IF NOT EXISTS identity_id UUID;
ALTER TABLE rule_conditions ADD COLUMN IF NOT EXISTS familiarity TEXT;

CREATE TABLE IF NOT EXISTS identities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    object_class TEXT NOT NULL,
    display_name TEXT,
    signature JSONB NOT NULL DEFAULT '{}'::jsonb,
    familiarity_score REAL NOT NULL DEFAULT 0.05 CHECK (familiarity_score >= 0 AND familiarity_score <= 1),
    familiarity TEXT NOT NULL DEFAULT 'new',
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sightings BIGINT NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS identities_class_seen_idx ON identities (object_class, last_seen DESC);

CREATE TABLE IF NOT EXISTS identity_observations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    identity_id UUID NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    similarity REAL NOT NULL CHECK (similarity >= 0 AND similarity <= 1),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(identity_id, track_id)
);
CREATE INDEX IF NOT EXISTS identity_observations_identity_time_idx ON identity_observations (identity_id, observed_at DESC);

CREATE TABLE IF NOT EXISTS identity_statistics (
    identity_id UUID PRIMARY KEY REFERENCES identities(id) ON DELETE CASCADE,
    average_duration_ms BIGINT NOT NULL DEFAULT 0,
    active_days BIGINT NOT NULL DEFAULT 0,
    top_zone_id UUID REFERENCES zones(id) ON DELETE SET NULL
);
