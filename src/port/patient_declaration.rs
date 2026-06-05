use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::patient_declaration::PatientDeclaration;

#[derive(Debug, Clone)]
pub struct UpsertPatientDeclaration {
    pub patient_id: Uuid,
    pub statement: String,
}

#[derive(Debug, Error)]
pub enum PatientDeclarationRepositoryError {
    #[error("patient declaration not found")]
    NotFound,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait PatientDeclarationRepository: Send + Sync {
    async fn upsert_patient_declaration(
        &self,
        declaration: UpsertPatientDeclaration,
    ) -> Result<PatientDeclaration, PatientDeclarationRepositoryError>;

    async fn find_patient_declaration(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError>;

    async fn find_patient_declaration_by_username(
        &self,
        username: &str,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError>;
}
