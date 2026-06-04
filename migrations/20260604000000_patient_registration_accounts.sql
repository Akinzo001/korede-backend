ALTER TABLE patients ADD COLUMN IF NOT EXISTS username TEXT;
ALTER TABLE patients ADD COLUMN IF NOT EXISTS first_name TEXT;
ALTER TABLE patients ADD COLUMN IF NOT EXISTS last_name TEXT;
ALTER TABLE patients ADD COLUMN IF NOT EXISTS email TEXT;
ALTER TABLE patients ADD COLUMN IF NOT EXISTS password_hash TEXT;
ALTER TABLE patients ADD COLUMN IF NOT EXISTS date_of_birth DATE;

CREATE UNIQUE INDEX IF NOT EXISTS idx_patients_username_lower
    ON patients(LOWER(username))
    WHERE username IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_patients_email_lower
    ON patients(LOWER(email))
    WHERE email IS NOT NULL;
