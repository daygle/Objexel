CREATE TABLE roles (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL
);

INSERT INTO roles (name, description) VALUES
    ('administrator', 'Full system access'),
    ('operator', 'Camera, event, recording, and rule access'),
    ('viewer', 'Read-only access')
ON CONFLICT (name) DO NOTHING;

CREATE TABLE permissions (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL
);

CREATE TABLE users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL REFERENCES roles(name),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    csrf_token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX sessions_user_idx ON sessions(user_id);
CREATE INDEX sessions_expiry_idx ON sessions(expires_at);
