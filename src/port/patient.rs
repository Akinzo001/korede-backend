use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    patient::Patient, patient_email_otp::PatientEmailOtp,
    patient_password_reset_otp::PatientPasswordResetOtp,
};

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

#[derive(Debug, Clone)]
pub struct NewPatientEmailOtp {
    pub patient_id: Uuid,
    pub email: String,
    pub otp_hash: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewPatientPasswordResetOtp {
    pub patient_id: Uuid,
    pub email: String,
    pub otp_hash: String,
    pub expires_at: DateTime<Utc>,
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
    async fn create_patient(&self, patient: NewPatient) -> Result<Patient, PatientRepositoryError>;

    async fn find_patient_by_username(
        &self,
        username: &str,
    ) -> Result<Option<Patient>, PatientRepositoryError>;

    async fn find_patient_by_id(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<Patient>, PatientRepositoryError>;

    async fn find_patient_by_email(
        &self,
        email: &str,
    ) -> Result<Option<Patient>, PatientRepositoryError>;

    async fn create_email_otp(
        &self,
        otp: NewPatientEmailOtp,
    ) -> Result<PatientEmailOtp, PatientRepositoryError>;

    async fn find_latest_email_otp(
        &self,
        email: &str,
    ) -> Result<Option<PatientEmailOtp>, PatientRepositoryError>;

    async fn increment_email_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), PatientRepositoryError>;

    async fn mark_email_otp_used(&self, otp_id: Uuid) -> Result<(), PatientRepositoryError>;

    async fn mark_patient_email_verified(
        &self,
        patient_id: Uuid,
    ) -> Result<Patient, PatientRepositoryError>;

    async fn invalidate_active_email_otps(
        &self,
        patient_id: Uuid,
    ) -> Result<(), PatientRepositoryError>;

    async fn latest_email_otp_created_at(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, PatientRepositoryError>;

    async fn create_password_reset_otp(
        &self,
        otp: NewPatientPasswordResetOtp,
    ) -> Result<PatientPasswordResetOtp, PatientRepositoryError>;

    async fn find_latest_password_reset_otp(
        &self,
        email: &str,
    ) -> Result<Option<PatientPasswordResetOtp>, PatientRepositoryError>;

    async fn increment_password_reset_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), PatientRepositoryError>;

    async fn mark_password_reset_otp_used(&self, otp_id: Uuid) -> Result<(), PatientRepositoryError>;

    async fn invalidate_active_password_reset_otps(
        &self,
        patient_id: Uuid,
    ) -> Result<(), PatientRepositoryError>;

    async fn latest_password_reset_otp_created_at(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, PatientRepositoryError>;

    async fn update_patient_password(
        &self,
        patient_id: Uuid,
        password_hash: String,
    ) -> Result<(), PatientRepositoryError>;
}
