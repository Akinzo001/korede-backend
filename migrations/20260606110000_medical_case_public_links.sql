ALTER TABLE medical_cases
ADD COLUMN IF NOT EXISTS public_slug TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_medical_cases_public_slug
ON medical_cases(public_slug)
WHERE public_slug IS NOT NULL;
