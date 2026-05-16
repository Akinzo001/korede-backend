ALTER TABLE medical_cases ADD COLUMN IF NOT EXISTS blockchain_network TEXT;
ALTER TABLE medical_cases ADD COLUMN IF NOT EXISTS blockchain_tx_digest TEXT;
ALTER TABLE medical_cases ADD COLUMN IF NOT EXISTS blockchain_record_id TEXT;

UPDATE medical_cases
SET
    blockchain_network = COALESCE(blockchain_network, 'solana_devnet'),
    blockchain_tx_digest = COALESCE(blockchain_tx_digest, solana_reference)
WHERE solana_reference IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_medical_cases_blockchain_network
    ON medical_cases(blockchain_network);

CREATE INDEX IF NOT EXISTS idx_medical_cases_blockchain_tx_digest
    ON medical_cases(blockchain_tx_digest);
