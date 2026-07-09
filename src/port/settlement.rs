use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::settlement::{HospitalSettlement, HospitalSettlementStatus};

#[derive(Debug, Clone)]
pub struct NewHospitalSettlement {
    pub hospital_id: Uuid,
    pub medical_case_id: Uuid,
    pub amount_kobo: i64,
    pub status: HospitalSettlementStatus,
    pub settlement_reference: String,
    pub bank_name: String,
    pub bank_code: Option<String>,
    pub account_name: String,
    pub account_number: String,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettlementRecipientUpdate {
    pub settlement_id: Uuid,
    pub status: HospitalSettlementStatus,
    pub paystack_recipient_code: String,
    pub paystack_status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettlementTransferUpdate {
    pub settlement_id: Uuid,
    pub status: HospitalSettlementStatus,
    pub paystack_transfer_code: Option<String>,
    pub paystack_transfer_id: Option<i64>,
    pub paystack_status: Option<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettlementStatusUpdate {
    pub settlement_id: Uuid,
    pub status: HospitalSettlementStatus,
    pub paystack_status: Option<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminSettlementListQuery {
    pub status: Option<HospitalSettlementStatus>,
    pub hospital_id: Option<Uuid>,
    pub medical_case_id: Option<Uuid>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct AdminSettlementOperation {
    pub settlement: HospitalSettlement,
    pub hospital_name: String,
    pub case_title: String,
    pub public_slug: Option<String>,
    pub patient_id: Uuid,
    pub patient_name: String,
}

#[derive(Debug, Error)]
pub enum SettlementRepositoryError {
    #[error("hospital settlement not found")]
    NotFound,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait HospitalSettlementRepository: Send + Sync {
    async fn create_or_get_settlement(
        &self,
        settlement: NewHospitalSettlement,
    ) -> Result<HospitalSettlement, SettlementRepositoryError>;

    async fn get_settlement(
        &self,
        settlement_id: Uuid,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError>;

    async fn find_by_medical_case_id(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError>;

    async fn find_by_reference(
        &self,
        reference: &str,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError>;

    async fn find_by_transfer_code(
        &self,
        transfer_code: &str,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError>;

    async fn update_recipient(
        &self,
        update: SettlementRecipientUpdate,
    ) -> Result<HospitalSettlement, SettlementRepositoryError>;

    async fn update_transfer(
        &self,
        update: SettlementTransferUpdate,
    ) -> Result<HospitalSettlement, SettlementRepositoryError>;

    async fn update_status(
        &self,
        update: SettlementStatusUpdate,
    ) -> Result<HospitalSettlement, SettlementRepositoryError>;

    async fn count_admin_settlements(
        &self,
        query: AdminSettlementListQuery,
    ) -> Result<i64, SettlementRepositoryError>;

    async fn list_admin_settlements(
        &self,
        query: AdminSettlementListQuery,
    ) -> Result<Vec<AdminSettlementOperation>, SettlementRepositoryError>;

    async fn get_admin_settlement(
        &self,
        settlement_id: Uuid,
    ) -> Result<Option<AdminSettlementOperation>, SettlementRepositoryError>;
}

pub fn settlement_timestamp_for_status(
    status: &HospitalSettlementStatus,
) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let now = Utc::now();
    match status {
        HospitalSettlementStatus::Paid => (Some(now), None),
        HospitalSettlementStatus::Failed
        | HospitalSettlementStatus::FailedConfig
        | HospitalSettlementStatus::BankDetailsRequired
        | HospitalSettlementStatus::Reversed => (None, Some(now)),
        _ => (None, None),
    }
}
