ALTER TABLE hospitals ADD COLUMN IF NOT EXISTS email_verified BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE hospitals ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS hospital_email_otps (
    id UUID PRIMARY KEY,
    hospital_id UUID NOT NULL REFERENCES hospitals(id),
    email TEXT NOT NULL,
    otp_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hospital_email_otps_hospital_id ON hospital_email_otps(hospital_id);
CREATE INDEX IF NOT EXISTS idx_hospital_email_otps_email ON hospital_email_otps(LOWER(email));
CREATE INDEX IF NOT EXISTS idx_hospital_email_otps_active ON hospital_email_otps(LOWER(email), used_at, expires_at);
