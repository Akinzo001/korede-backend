use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HospitalDocumentType {
    CacCertificate,
    MedicalLicense,
}

impl HospitalDocumentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CacCertificate => "cac_certificate",
            Self::MedicalLicense => "medical_license",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HospitalDocumentStatus {
    Pending,
    Approved,
    Rejected,
}

impl HospitalDocumentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageProvider {
    Local,
    S3,
    Backblaze,
}

impl StorageProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Backblaze => "backblaze",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HospitalDocument {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub document_type: HospitalDocumentType,
    pub storage_provider: StorageProvider,
    pub storage_key: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub status: HospitalDocumentStatus,
    pub uploaded_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_message: Option<String>,
}
