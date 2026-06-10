CREATE UNIQUE INDEX IF NOT EXISTS idx_medical_cases_one_open_case_per_patient
ON medical_cases(patient_id)
WHERE status IN (
    'draft',
    'pending_review',
    'active',
    'funded',
    'treatment_commenced'
);
