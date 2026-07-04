CREATE TABLE IF NOT EXISTS patient_case_declarations (
    id UUID PRIMARY KEY,
    medical_case_id UUID NOT NULL REFERENCES medical_cases(id) ON DELETE CASCADE,
    patient_id UUID NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    statement TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT patient_case_declarations_medical_case_id_key UNIQUE (medical_case_id)
);

CREATE INDEX IF NOT EXISTS idx_patient_case_declarations_patient_id
    ON patient_case_declarations(patient_id);
