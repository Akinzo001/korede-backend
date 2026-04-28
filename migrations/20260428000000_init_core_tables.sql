CREATE TABLE hospitals (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    cac_registration_number TEXT,
    medical_license_number TEXT,
    corporate_account_name TEXT NOT NULL,
    corporate_account_number TEXT NOT NULL,
    bank_name TEXT NOT NULL,
    verification_status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE patients (
    id UUID PRIMARY KEY,
    full_name TEXT NOT NULL,
    age INTEGER,
    gender TEXT,
    phone_number TEXT,
    consent_given BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE medical_cases (
    id UUID PRIMARY KEY,
    hospital_id UUID NOT NULL REFERENCES hospitals(id),
    patient_id UUID NOT NULL REFERENCES patients(id),
    title TEXT NOT NULL,
    diagnosis_summary TEXT NOT NULL,
    bill_amount_kobo BIGINT NOT NULL,
    amount_raised_kobo BIGINT NOT NULL,
    status TEXT NOT NULL,
    solana_reference TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_medical_cases_status ON medical_cases(status);
CREATE INDEX idx_medical_cases_hospital_id ON medical_cases(hospital_id);
CREATE INDEX idx_medical_cases_patient_id ON medical_cases(patient_id);
