use async_trait::async_trait;
use chrono::NaiveDate;
use thiserror::Error;

use crate::domain::patient::Patient;

#[derive(Debug, Clone)]
pub struct NewPatient {
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub phone_number: Option<String>,
}

#[derive(Debug, Error)]
pub enum PatientRepositoryError {
    #[error("patient username already exists")]
    DuplicateUsername,

    #[error("patient email already exists")]
    DuplicateEmail,

    #[error("patient not found")]
    NotFound,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait PatientRepository: Send + Sync {
    async fn create_patient(
        &self,
        patient: NewPatient,
    ) -> Result<Patient, PatientRepositoryError>;

    async fn find_patient_by_username(
        &self,
        username: &str,
    ) -> Result<Option<Patient>, PatientRepositoryError>;
}
