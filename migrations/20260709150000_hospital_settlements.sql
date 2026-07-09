ALTER TABLE hospitals
ADD COLUMN IF NOT EXISTS corporate_bank_code TEXT;

CREATE TABLE hospital_settlements (
    id UUID PRIMARY KEY,
    hospital_id UUID NOT NULL REFERENCES hospitals(id),
    medical_case_id UUID NOT NULL REFERENCES medical_cases(id),
    amount_kobo BIGINT NOT NULL CHECK (amount_kobo > 0),
    status TEXT NOT NULL,
    settlement_reference TEXT NOT NULL UNIQUE,
    bank_name TEXT NOT NULL,
    bank_code TEXT,
    account_name TEXT NOT NULL,
    account_number TEXT NOT NULL,
    paystack_recipient_code TEXT,
    paystack_transfer_code TEXT,
    paystack_transfer_id BIGINT,
    paystack_status TEXT,
    failure_reason TEXT,
    initiated_at TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (medical_case_id)
);

CREATE INDEX idx_hospital_settlements_hospital_id
    ON hospital_settlements(hospital_id);

CREATE INDEX idx_hospital_settlements_status
    ON hospital_settlements(status);

CREATE INDEX idx_hospital_settlements_created_at
    ON hospital_settlements(created_at DESC);

CREATE INDEX idx_hospital_settlements_paystack_transfer_code
    ON hospital_settlements(paystack_transfer_code)
    WHERE paystack_transfer_code IS NOT NULL;
