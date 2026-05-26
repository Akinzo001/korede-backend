CREATE TABLE IF NOT EXISTS hospital_login_otps (
    id UUID PRIMARY KEY,
    hospital_id UUID NOT NULL REFERENCES hospitals(id),
    email TEXT NOT NULL,
    otp_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hospital_login_otps_hospital_id ON hospital_login_otps(hospital_id);
CREATE INDEX IF NOT EXISTS idx_hospital_login_otps_email ON hospital_login_otps(LOWER(email));
CREATE INDEX IF NOT EXISTS idx_hospital_login_otps_active ON hospital_login_otps(hospital_id, used_at, expires_at);

CREATE TABLE IF NOT EXISTS hospital_audit_logs (
    id UUID PRIMARY KEY,
    hospital_id UUID REFERENCES hospitals(id),
    email TEXT,
    event_type TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    reason TEXT,
    ip_address TEXT,
    user_agent TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hospital_audit_logs_hospital_id ON hospital_audit_logs(hospital_id);
CREATE INDEX IF NOT EXISTS idx_hospital_audit_logs_email ON hospital_audit_logs(LOWER(email));
CREATE INDEX IF NOT EXISTS idx_hospital_audit_logs_event_type ON hospital_audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_hospital_audit_logs_created_at ON hospital_audit_logs(created_at);
