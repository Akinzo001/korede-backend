CREATE TABLE IF NOT EXISTS hospital_password_reset_otps (
    id UUID PRIMARY KEY,
    hospital_id UUID NOT NULL REFERENCES hospitals(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    otp_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_hospital_password_reset_otps_hospital_id_created_at
    ON hospital_password_reset_otps(hospital_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_hospital_password_reset_otps_email_created_at
    ON hospital_password_reset_otps(LOWER(email), created_at DESC);

CREATE TABLE IF NOT EXISTS patient_password_reset_otps (
    id UUID PRIMARY KEY,
    patient_id UUID NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    otp_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_patient_password_reset_otps_patient_id_created_at
    ON patient_password_reset_otps(patient_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_patient_password_reset_otps_email_created_at
    ON patient_password_reset_otps(LOWER(email), created_at DESC);
