ALTER TABLE rule_conditions ADD COLUMN IF NOT EXISTS minimum_priority TEXT;
ALTER TABLE rule_conditions ADD COLUMN IF NOT EXISTS minimum_anomaly_score REAL;

CREATE TABLE IF NOT EXISTS identity_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(), identity_id UUID NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    familiarity_score REAL NOT NULL CHECK (familiarity_score BETWEEN 0 AND 1), confidence REAL NOT NULL CHECK (confidence BETWEEN 0 AND 1), scored_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS identity_scores_time_idx ON identity_scores (scored_at DESC);

CREATE TABLE IF NOT EXISTS behaviour_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(), behaviour_id UUID NOT NULL REFERENCES behaviours(id) ON DELETE CASCADE,
    anomaly_score REAL NOT NULL CHECK (anomaly_score BETWEEN 0 AND 1), behaviour_level TEXT NOT NULL, confidence REAL NOT NULL CHECK (confidence BETWEEN 0 AND 1), scored_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS behaviour_scores_time_idx ON behaviour_scores (scored_at DESC);

CREATE TABLE IF NOT EXISTS anomaly_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(), event_id UUID REFERENCES events(id) ON DELETE CASCADE, identity_id UUID REFERENCES identities(id) ON DELETE SET NULL, behaviour_id UUID REFERENCES behaviours(id) ON DELETE SET NULL,
    anomaly_score REAL NOT NULL CHECK (anomaly_score BETWEEN 0 AND 1), priority_score REAL NOT NULL CHECK (priority_score BETWEEN 0 AND 1), priority TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS anomaly_events_priority_time_idx ON anomaly_events (priority, created_at DESC);
