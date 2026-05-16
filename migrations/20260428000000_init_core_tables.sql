-- This migration creates the first core tables for Korede.
--
-- SQLx migrations run once and are tracked in the database.
-- After this file runs successfully, SQLx records it so it will not
-- be applied again on the next app startup.

-- Hospitals are the verified medical institutions that create cases
-- and receive direct settlement.
CREATE TABLE hospitals (
    -- Unique hospital ID.
    id UUID PRIMARY KEY,

    -- Public hospital name.
    name TEXT NOT NULL,

    -- Optional CAC registration number.
    cac_registration_number TEXT,

    -- Optional medical license number.
    medical_license_number TEXT,

    -- Bank account name for hospital settlement.
    corporate_account_name TEXT NOT NULL,

    -- Bank account number for hospital settlement.
    corporate_account_number TEXT NOT NULL,

    -- Bank name for hospital settlement.
    bank_name TEXT NOT NULL,

    -- Verification state.
    --
    -- Stored as TEXT for now and validated by Rust enums.
    verification_status TEXT NOT NULL,

    -- Timestamp when the row was created.
    created_at TIMESTAMPTZ NOT NULL,

    -- Timestamp when the row was last updated.
    updated_at TIMESTAMPTZ NOT NULL
);

-- Patients are the beneficiaries of medical cases.
CREATE TABLE patients (
    -- Unique patient ID.
    id UUID PRIMARY KEY,

    -- Patient full name.
    full_name TEXT NOT NULL,

    -- Optional patient age.
    age INTEGER,

    -- Optional patient gender.
    gender TEXT,

    -- Optional patient or guardian phone number.
    phone_number TEXT,

    -- Whether the patient consented to having the case shared.
    consent_given BOOLEAN NOT NULL,

    -- Timestamp when the row was created.
    created_at TIMESTAMPTZ NOT NULL,

    -- Timestamp when the row was last updated.
    updated_at TIMESTAMPTZ NOT NULL
);

-- Medical cases connect a patient to a hospital and represent a bill
-- that donors can fund.
CREATE TABLE medical_cases (
    -- Unique medical case ID.
    id UUID PRIMARY KEY,

    -- The hospital that verified and owns this case.
    hospital_id UUID NOT NULL REFERENCES hospitals(id),

    -- The patient attached to this case.
    patient_id UUID NOT NULL REFERENCES patients(id),

    -- Public campaign title.
    title TEXT NOT NULL,

    -- Hospital-verified diagnosis summary.
    diagnosis_summary TEXT NOT NULL,

    -- Total hospital bill in kobo.
    bill_amount_kobo BIGINT NOT NULL,

    -- Amount raised so far in kobo.
    amount_raised_kobo BIGINT NOT NULL,

    -- Case lifecycle status.
    --
    -- Stored as TEXT for now and validated by Rust enums.
    status TEXT NOT NULL,

    -- Legacy Solana transaction/reference.
    -- New code should use generic blockchain_* columns added by later migrations.
    solana_reference TEXT,

    -- Timestamp when the row was created.
    created_at TIMESTAMPTZ NOT NULL,

    -- Timestamp when the row was last updated.
    updated_at TIMESTAMPTZ NOT NULL
);

-- Index used when filtering cases by status.
CREATE INDEX idx_medical_cases_status ON medical_cases(status);

-- Index used when listing cases for a hospital.
CREATE INDEX idx_medical_cases_hospital_id ON medical_cases(hospital_id);

-- Index used when looking up cases for a patient.
CREATE INDEX idx_medical_cases_patient_id ON medical_cases(patient_id);
