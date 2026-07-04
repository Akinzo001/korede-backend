ALTER TABLE case_donations
    ADD COLUMN proof_attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN proof_last_attempt_at TIMESTAMPTZ,
    ADD COLUMN proof_next_retry_at TIMESTAMPTZ,
    ADD COLUMN proof_last_error TEXT,
    ADD COLUMN proof_published_at TIMESTAMPTZ;

CREATE INDEX idx_case_donations_proof_retry
    ON case_donations(proof_status, proof_next_retry_at)
    WHERE status = 'paid' AND sui_tx_digest IS NULL;
