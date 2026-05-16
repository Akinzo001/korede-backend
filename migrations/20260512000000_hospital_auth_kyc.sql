ALTER TABLE hospitals ADD COLUMN IF NOT EXISTS email TEXT;
ALTER TABLE hospitals ADD COLUMN IF NOT EXISTS password_hash TEXT;
ALTER TABLE hospitals ADD COLUMN IF NOT EXISTS phone_number TEXT;

UPDATE hospitals
SET email = CONCAT('legacy+', id::TEXT, '@korede.local')
WHERE email IS NULL;

UPDATE hospitals
SET password_hash = 'legacy-login-disabled'
WHERE password_hash IS NULL;

ALTER TABLE hospitals ALTER COLUMN email SET NOT NULL;
ALTER TABLE hospitals ALTER COLUMN password_hash SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_hospitals_email_lower ON hospitals(LOWER(email));

CREATE TABLE IF NOT EXISTS hospital_documents (
    id UUID PRIMARY KEY,
    hospital_id UUID NOT NULL REFERENCES hospitals(id),
    document_type TEXT NOT NULL,
    storage_provider TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    status TEXT NOT NULL,
    uploaded_at TIMESTAMPTZ NOT NULL,
    reviewed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_hospital_documents_hospital_id ON hospital_documents(hospital_id);
CREATE INDEX IF NOT EXISTS idx_hospital_documents_document_type ON hospital_documents(document_type);
CREATE INDEX IF NOT EXISTS idx_hospital_documents_status ON hospital_documents(status);
