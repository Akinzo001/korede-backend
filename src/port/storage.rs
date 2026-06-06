use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::hospital_document::HospitalDocumentType;

#[derive(Debug, Clone)]
pub struct StoredDocument {
    pub storage_provider: String,
    pub storage_key: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
}

#[derive(Debug, Error)]
pub enum DocumentStorageError {
    #[error("failed to store document")]
    StoreFailed,

    #[error("missing storage configuration: {0}")]
    MissingConfig(&'static str),
}

#[async_trait]
pub trait DocumentStorage: Send + Sync {
    async fn save_document(
        &self,
        hospital_id: Uuid,
        document_type: HospitalDocumentType,
        original_filename: &str,
        mime_type: &str,
        contents: &[u8],
    ) -> Result<StoredDocument, DocumentStorageError>;

    async fn save_case_document(
        &self,
        hospital_id: Uuid,
        case_id: Uuid,
        document_type: &str,
        original_filename: &str,
        mime_type: &str,
        contents: &[u8],
    ) -> Result<StoredDocument, DocumentStorageError>;
}
