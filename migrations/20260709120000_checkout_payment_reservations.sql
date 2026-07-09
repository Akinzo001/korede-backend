ALTER TABLE case_donations
    ADD COLUMN reservation_expires_at TIMESTAMPTZ,
    ADD COLUMN expired_at TIMESTAMPTZ,
    ADD COLUMN is_late_payment BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN payment_note TEXT;

UPDATE case_donations
SET reservation_expires_at = created_at + INTERVAL '5 minutes'
WHERE method = 'checkout'
  AND status = 'pending'
  AND reservation_expires_at IS NULL;

CREATE INDEX idx_case_donations_active_checkout_reservations
    ON case_donations(medical_case_id, reservation_expires_at)
    WHERE method = 'checkout' AND status = 'pending';
