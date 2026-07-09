use async_trait::async_trait;
use chrono::NaiveDate;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    medical_case::MedicalCase, medical_case_billing_item::MedicalCaseBillingItem,
    medical_case_document::MedicalCaseDocument,
};

#[derive(Debug, Clone)]
pub struct NewMedicalCase {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub patient_id: Uuid,
    pub patient_declaration_id: Uuid,
    pub patient_declaration_statement: String,
    pub title: String,
    pub public_slug: String,
    pub diagnosis_summary: String,
    pub bill_amount_kobo: i64,
    pub admitted_at: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
pub struct NewMedicalCaseBillingItem {
    pub description: String,
    pub amount_kobo: i64,
}

#[derive(Debug, Clone)]
pub struct NewMedicalCaseDocument {
    pub document_type: String,
    pub storage_provider: String,
    pub storage_key: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct CreatedMedicalCase {
    pub case: MedicalCase,
    pub billing_items: Vec<MedicalCaseBillingItem>,
    pub documents: Vec<MedicalCaseDocument>,
}

#[derive(Debug, Clone)]
pub struct HospitalMedicalCaseSummary {
    pub case: MedicalCase,
    pub patient_name: String,
}

#[derive(Debug, Error)]
pub enum MedicalCaseRepositoryError {
    #[error("medical case not found")]
    NotFound,

    #[error("patient declaration not found")]
    PatientDeclarationNotFound,

    #[error("patient already has an open medical case")]
    PatientHasOpenCase,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait MedicalCaseRepository: Send + Sync {
    async fn create_published_case(
        &self,
        medical_case: NewMedicalCase,
        billing_items: Vec<NewMedicalCaseBillingItem>,
        documents: Vec<NewMedicalCaseDocument>,
    ) -> Result<CreatedMedicalCase, MedicalCaseRepositoryError>;

    async fn list_patient_cases(
        &self,
        patient_id: Uuid,
    ) -> Result<Vec<MedicalCase>, MedicalCaseRepositoryError>;

    async fn list_hospital_active_cases(
        &self,
        hospital_id: Uuid,
    ) -> Result<Vec<HospitalMedicalCaseSummary>, MedicalCaseRepositoryError>;

    async fn list_hospital_completed_cases(
        &self,
        hospital_id: Uuid,
    ) -> Result<Vec<HospitalMedicalCaseSummary>, MedicalCaseRepositoryError>;

    async fn find_case_by_public_slug(
        &self,
        public_slug: &str,
    ) -> Result<Option<MedicalCase>, MedicalCaseRepositoryError>;

    async fn find_case_by_id(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<MedicalCase>, MedicalCaseRepositoryError>;

    async fn patient_has_open_case(
        &self,
        patient_id: Uuid,
    ) -> Result<bool, MedicalCaseRepositoryError>;
}
