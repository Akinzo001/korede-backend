use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    hospital::{Hospital, HospitalVerificationStatus},
    hospital_document::{HospitalDocument, HospitalDocumentStatus, HospitalDocumentType},
    hospital_email_otp::HospitalEmailOtp,
    hospital_login_otp::HospitalLoginOtp,
    hospital_password_reset_otp::HospitalPasswordResetOtp,
};

#[derive(Debug, Clone)]
pub struct NewHospital {
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub phone_number: Option<String>,
    pub official_address: String,
    pub administrator_name: String,
    pub cac_registration_number: Option<String>,
    pub medical_license_number: Option<String>,
    pub corporate_account_name: String,
    pub corporate_account_number: String,
    pub bank_name: String,
}

#[derive(Debug, Clone)]
pub struct NewHospitalDocument {
    pub hospital_id: Uuid,
    pub document_type: HospitalDocumentType,
    pub storage_provider: String,
    pub storage_key: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct NewHospitalEmailOtp {
    pub hospital_id: Uuid,
    pub email: String,
    pub otp_hash: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct NewHospitalLoginOtp {
    pub hospital_id: Uuid,
    pub email: String,
    pub otp_hash: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct NewHospitalPasswordResetOtp {
    pub hospital_id: Uuid,
    pub email: String,
    pub otp_hash: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct NewHospitalAuditLog {
    pub hospital_id: Option<Uuid>,
    pub email: Option<String>,
    pub event_type: String,
    pub success: bool,
    pub reason: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct HospitalDocumentReview {
    pub status: HospitalDocumentStatus,
    pub review_message: Option<String>,
}

#[derive(Debug, Error)]
pub enum HospitalRepositoryError {
    #[error("hospital email already exists")]
    DuplicateEmail,

    #[error("hospital not found")]
    NotFound,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait HospitalRepository: Send + Sync {
    async fn create_hospital(
        &self,
        hospital: NewHospital,
    ) -> Result<Hospital, HospitalRepositoryError>;

    async fn find_hospital_by_email(
        &self,
        email: &str,
    ) -> Result<Option<Hospital>, HospitalRepositoryError>;

    async fn find_hospital_by_id(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<Hospital>, HospitalRepositoryError>;

    async fn list_hospitals(&self) -> Result<Vec<Hospital>, HospitalRepositoryError>;

    async fn save_hospital_document(
        &self,
        document: NewHospitalDocument,
    ) -> Result<HospitalDocument, HospitalRepositoryError>;

    async fn list_hospital_documents(
        &self,
        hospital_id: Uuid,
    ) -> Result<Vec<HospitalDocument>, HospitalRepositoryError>;

    async fn review_hospital_document(
        &self,
        hospital_id: Uuid,
        document_id: Uuid,
        review: HospitalDocumentReview,
    ) -> Result<HospitalDocument, HospitalRepositoryError>;

    async fn update_hospital_verification_status(
        &self,
        hospital_id: Uuid,
        status: HospitalVerificationStatus,
    ) -> Result<Hospital, HospitalRepositoryError>;

    async fn create_email_otp(
        &self,
        otp: NewHospitalEmailOtp,
    ) -> Result<HospitalEmailOtp, HospitalRepositoryError>;

    async fn find_latest_email_otp(
        &self,
        email: &str,
    ) -> Result<Option<HospitalEmailOtp>, HospitalRepositoryError>;

    async fn increment_email_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn mark_email_otp_used(&self, otp_id: Uuid) -> Result<(), HospitalRepositoryError>;

    async fn mark_hospital_email_verified(
        &self,
        hospital_id: Uuid,
    ) -> Result<Hospital, HospitalRepositoryError>;

    async fn invalidate_active_email_otps(
        &self,
        hospital_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn latest_email_otp_created_at(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, HospitalRepositoryError>;

    async fn create_login_otp(
        &self,
        otp: NewHospitalLoginOtp,
    ) -> Result<HospitalLoginOtp, HospitalRepositoryError>;

    async fn find_login_otp_by_id(
        &self,
        otp_id: Uuid,
    ) -> Result<Option<HospitalLoginOtp>, HospitalRepositoryError>;

    async fn increment_login_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn mark_login_otp_used(&self, otp_id: Uuid) -> Result<(), HospitalRepositoryError>;

    async fn invalidate_active_login_otps(
        &self,
        hospital_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn create_password_reset_otp(
        &self,
        otp: NewHospitalPasswordResetOtp,
    ) -> Result<HospitalPasswordResetOtp, HospitalRepositoryError>;

    async fn find_latest_password_reset_otp(
        &self,
        email: &str,
    ) -> Result<Option<HospitalPasswordResetOtp>, HospitalRepositoryError>;

    async fn increment_password_reset_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn mark_password_reset_otp_used(
        &self,
        otp_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn invalidate_active_password_reset_otps(
        &self,
        hospital_id: Uuid,
    ) -> Result<(), HospitalRepositoryError>;

    async fn latest_password_reset_otp_created_at(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, HospitalRepositoryError>;

    async fn update_hospital_password(
        &self,
        hospital_id: Uuid,
        password_hash: String,
    ) -> Result<(), HospitalRepositoryError>;

    async fn save_audit_log(
        &self,
        audit_log: NewHospitalAuditLog,
    ) -> Result<(), HospitalRepositoryError>;
}
