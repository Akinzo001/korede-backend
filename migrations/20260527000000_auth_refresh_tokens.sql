CREATE TABLE IF NOT EXISTS auth_refresh_tokens (
    id UUID PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    subject_id TEXT NOT NULL,
    email TEXT NOT NULL,
    role TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    CONSTRAINT auth_refresh_tokens_role_check CHECK (role IN ('admin', 'hospital'))
);

CREATE INDEX IF NOT EXISTS idx_auth_refresh_tokens_subject_role
    ON auth_refresh_tokens (subject_id, role);

CREATE INDEX IF NOT EXISTS idx_auth_refresh_tokens_expires_at
    ON auth_refresh_tokens (expires_at);
