CREATE TABLE IF NOT EXISTS analytics_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    period TEXT NOT NULL CHECK (period IN ('daily','weekly','monthly')),
    period_start TIMESTAMPTZ NOT NULL,
    summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (period, period_start)
);
CREATE INDEX IF NOT EXISTS analytics_snapshots_period_idx ON analytics_snapshots (period, period_start DESC);
