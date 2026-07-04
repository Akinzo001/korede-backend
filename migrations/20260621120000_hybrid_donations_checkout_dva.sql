CREATE TABLE case_payment_dvas (
    medical_case_id UUID PRIMARY KEY REFERENCES medical_cases(id) ON DELETE CASCADE,
    paystack_reference TEXT NOT NULL UNIQUE,
    paystack_customer_code TEXT,
    paystack_dedicated_account_id BIGINT NOT NULL UNIQUE,
    account_number TEXT NOT NULL UNIQUE,
    account_name TEXT NOT NULL,
    bank_name TEXT NOT NULL,
    bank_slug TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    deactivated_at TIMESTAMPTZ,
    deactivation_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE case_donations
    ADD COLUMN method TEXT NOT NULL DEFAULT 'checkout';

CREATE INDEX idx_case_payment_dvas_active
    ON case_payment_dvas(is_active, medical_case_id);
