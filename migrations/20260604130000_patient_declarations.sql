DROP TABLE IF EXISTS patient_case_declarations;

CREATE TABLE IF NOT EXISTS patient_declarations (
    id UUID PRIMARY KEY,
    patient_id UUID NOT NULL REFERENCES patients(id),
    statement TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT patient_declarations_patient_id_key UNIQUE (patient_id)
);
