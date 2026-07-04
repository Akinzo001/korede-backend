use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::patient_declaration::PatientDeclaration;

#[derive(Debug, Clone)]
pub struct NewPatientDeclaration {
    pub patient_id: Uuid,
    pub statement: String,
}

#[derive(Debug, Clone)]
pub struct UpdatePatientDeclaration {
    pub patient_id: Uuid,
    pub statement: String,
}

#[derive(Debug, Error)]
pub enum PatientDeclarationRepositoryError {
    #[error("patient declaration not found")]
    NotFound,

    #[error("patient declaration already exists")]
    DuplicateDeclaration,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait PatientDeclarationRepository: Send + Sync {
    async fn create_patient_declaration(
        &self,
        declaration: NewPatientDeclaration,
    ) -> Result<PatientDeclaration, PatientDeclarationRepositoryError>;

    async fn update_patient_declaration(
        &self,
        declaration: UpdatePatientDeclaration,
    ) -> Result<PatientDeclaration, PatientDeclarationRepositoryError>;

    async fn find_current_patient_declaration(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError>;

    async fn find_current_patient_declaration_by_username(
        &self,
        username: &str,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError>;

    async fn find_case_declaration(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError>;
}
