use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalCaseDocument {
    pub id: Uuid,
    pub medical_case_id: Uuid,
    pub hospital_id: Uuid,
    pub document_type: String,
    pub storage_provider: String,
    pub storage_key: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
}
