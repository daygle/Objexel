CREATE TABLE rules (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT NOT NULL DEFAULT '',
    cooldown_seconds BIGINT NOT NULL DEFAULT 0,
    suppression_seconds BIGINT NOT NULL DEFAULT 0,
    severity TEXT NOT NULL DEFAULT 'info' CHECK (severity IN ('info', 'warning', 'critical')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE rule_conditions (
    id UUID PRIMARY KEY,
    rule_id UUID NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    object_class TEXT,
    zone_id UUID REFERENCES zones(id) ON DELETE CASCADE,
    observation_type TEXT,
    confidence_threshold REAL,
    minimum_duration_ms BIGINT
);

CREATE INDEX rule_conditions_rule_idx ON rule_conditions (rule_id);

ALTER TABLE events
    ADD COLUMN rule_id UUID REFERENCES rules(id) ON DELETE CASCADE,
    ADD COLUMN track_id UUID REFERENCES tracks(id) ON DELETE SET NULL,
    ADD COLUMN observation_id UUID REFERENCES observations(id) ON DELETE SET NULL,
    ADD COLUMN event_type TEXT,
    ADD COLUMN summary TEXT,
    ADD COLUMN severity TEXT DEFAULT 'info';

UPDATE events
SET event_type = COALESCE(event_type, kind),
    summary = COALESCE(summary, kind),
    severity = COALESCE(severity, 'info');

ALTER TABLE events
    ALTER COLUMN event_type SET NOT NULL,
    ALTER COLUMN summary SET NOT NULL,
    ALTER COLUMN severity SET NOT NULL,
    ADD CONSTRAINT events_severity_check CHECK (severity IN ('info', 'warning', 'critical'));

CREATE INDEX events_created_idx ON events (created_at DESC);
CREATE INDEX events_rule_idx ON events (rule_id, created_at DESC);
