CREATE TABLE case_donations (
    id UUID PRIMARY KEY,
    medical_case_id UUID NOT NULL REFERENCES medical_cases(id) ON DELETE CASCADE,
    donor_display_name TEXT NOT NULL,
    donor_email TEXT NOT NULL,
    amount_kobo BIGINT NOT NULL,
    paystack_reference TEXT NOT NULL,
    paystack_transaction_reference TEXT,
    paystack_access_code TEXT,
    paystack_authorization_url TEXT,
    paystack_customer_code TEXT,
    paystack_dedicated_account_id BIGINT,
    paystack_dedicated_account_number TEXT,
    paystack_dedicated_account_name TEXT,
    paystack_dedicated_bank_name TEXT,
    paystack_dedicated_bank_slug TEXT,
    status TEXT NOT NULL,
    paid_at TIMESTAMPTZ,
    proof_status TEXT NOT NULL,
    sui_network TEXT,
    sui_tx_digest TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT case_donations_paystack_reference_key UNIQUE (paystack_reference)
);

CREATE INDEX idx_case_donations_medical_case_id
    ON case_donations(medical_case_id);

CREATE UNIQUE INDEX idx_case_donations_dedicated_account_number
    ON case_donations(paystack_dedicated_account_number)
    WHERE paystack_dedicated_account_number IS NOT NULL;

CREATE INDEX idx_case_donations_status
    ON case_donations(status);

CREATE INDEX idx_case_donations_paid_at
    ON case_donations(paid_at DESC);
