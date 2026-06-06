ALTER TABLE medical_cases
ADD COLUMN IF NOT EXISTS admitted_at DATE;

ALTER TABLE medical_cases
ADD COLUMN IF NOT EXISTS blockchain_network TEXT;

ALTER TABLE medical_cases
ADD COLUMN IF NOT EXISTS blockchain_tx_digest TEXT;

ALTER TABLE medical_cases
ADD COLUMN IF NOT EXISTS blockchain_record_id TEXT;

CREATE TABLE medical_case_billing_items (
    id UUID PRIMARY KEY,
    medical_case_id UUID NOT NULL REFERENCES medical_cases(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    amount_kobo BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_medical_case_billing_items_case_id
ON medical_case_billing_items(medical_case_id);

CREATE TABLE medical_case_documents (
    id UUID PRIMARY KEY,
    medical_case_id UUID NOT NULL REFERENCES medical_cases(id) ON DELETE CASCADE,
    hospital_id UUID NOT NULL REFERENCES hospitals(id) ON DELETE CASCADE,
    document_type TEXT NOT NULL,
    storage_provider TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    uploaded_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_medical_case_documents_case_id
ON medical_case_documents(medical_case_id);

CREATE INDEX idx_medical_case_documents_hospital_id
ON medical_case_documents(hospital_id);
