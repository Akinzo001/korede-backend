use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    donation::{CaseDva, Donation, DonationMethod, DonationProofStatus, DonationStatus},
    medical_case::MedicalCase,
    public_case::PublicCaseDetails,
};

pub const CHECKOUT_RESERVATION_SECONDS: i64 = 5 * 60;

#[derive(Debug, Clone)]
pub struct NewDonation {
    pub medical_case_id: Uuid,
    pub donor_display_name: String,
    pub donor_email: String,
    pub amount_kobo: i64,
    pub method: DonationMethod,
    pub paystack_reference: String,
    pub paystack_transaction_reference: Option<String>,
    pub paystack_access_code: Option<String>,
    pub paystack_authorization_url: Option<String>,
    pub paystack_customer_code: Option<String>,
    pub paystack_dedicated_account_id: Option<i64>,
    pub paystack_dedicated_account_number: Option<String>,
    pub paystack_dedicated_account_name: Option<String>,
    pub paystack_dedicated_bank_name: Option<String>,
    pub paystack_dedicated_bank_slug: Option<String>,
    pub reservation_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CheckoutInitializationUpdate {
    pub donation_id: Uuid,
    pub paystack_access_code: String,
    pub paystack_authorization_url: String,
}

#[derive(Debug, Clone)]
pub struct UpsertCaseDva {
    pub medical_case_id: Uuid,
    pub paystack_reference: String,
    pub paystack_customer_code: Option<String>,
    pub paystack_dedicated_account_id: i64,
    pub account_number: String,
    pub account_name: String,
    pub bank_name: String,
    pub bank_slug: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DonationPaymentUpdate {
    pub donation_id: Uuid,
    pub paystack_transaction_reference: String,
    pub paid_at: DateTime<Utc>,
    pub is_late_payment: bool,
    pub payment_note: Option<String>,
    pub proof_status: DonationProofStatus,
    pub sui_network: Option<String>,
    pub sui_tx_digest: Option<String>,
    pub proof_attempt_count: i32,
    pub proof_last_attempt_at: Option<DateTime<Utc>>,
    pub proof_next_retry_at: Option<DateTime<Utc>>,
    pub proof_last_error: Option<String>,
    pub proof_published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct DonationPaymentResult {
    pub donation: Donation,
    pub newly_paid: bool,
}

#[derive(Debug, Clone)]
pub struct DonationFundingAvailability {
    pub confirmed_amount_kobo: i64,
    pub pending_amount_kobo: i64,
    pub remaining_amount_kobo: i64,
    pub available_amount_kobo: i64,
    pub active_pending_payment_count: i64,
    pub next_reservation_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct DonationProofAttemptUpdate {
    pub donation_id: Uuid,
    pub proof_status: DonationProofStatus,
    pub sui_network: Option<String>,
    pub sui_tx_digest: Option<String>,
    pub proof_attempt_count: i32,
    pub proof_last_attempt_at: DateTime<Utc>,
    pub proof_next_retry_at: Option<DateTime<Utc>>,
    pub proof_last_error: Option<String>,
    pub proof_published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct DonationFailureUpdate {
    pub donation_id: Uuid,
    pub status: DonationStatus,
}

#[derive(Debug, Clone)]
pub struct DonationCaseLock {
    pub donation: Donation,
    pub medical_case: MedicalCase,
    pub remaining_amount_kobo: i64,
}

#[derive(Debug, Clone)]
pub struct DonationProofJob {
    pub donation: Donation,
    pub medical_case: MedicalCase,
}

#[derive(Debug, Clone, Default)]
pub struct AdminDonationFilters {
    pub status: Option<DonationStatus>,
    pub method: Option<DonationMethod>,
    pub proof_status: Option<DonationProofStatus>,
    pub hospital_id: Option<Uuid>,
    pub medical_case_id: Option<Uuid>,
    pub paystack_reference: Option<String>,
    pub is_late_payment: Option<bool>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct AdminDonationListQuery {
    pub filters: AdminDonationFilters,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct AdminDonationOperation {
    pub donation: Donation,
    pub case_title: String,
    pub public_slug: Option<String>,
    pub bill_amount_kobo: i64,
    pub amount_raised_kobo: i64,
    pub case_status: String,
    pub hospital_id: Uuid,
    pub hospital_name: String,
    pub patient_id: Uuid,
    pub patient_name: String,
}

#[derive(Debug, Error)]
pub enum DonationRepositoryError {
    #[error("donation not found")]
    NotFound,

    #[error("donation reference already exists")]
    DuplicateReference,

    #[error("payment amount exceeds remaining case amount")]
    AmountExceedsRemaining,

    #[error("donation amount exceeds currently available amount")]
    AmountExceedsAvailable,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait DonationRepository: Send + Sync {
    async fn create_pending_donation(
        &self,
        donation: NewDonation,
    ) -> Result<Donation, DonationRepositoryError>;

    async fn attach_checkout_initialization(
        &self,
        update: CheckoutInitializationUpdate,
    ) -> Result<Donation, DonationRepositoryError>;

    async fn expire_checkout_reservations(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, DonationRepositoryError>;

    async fn get_case_funding_availability(
        &self,
        medical_case_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<DonationFundingAvailability, DonationRepositoryError>;

    async fn create_paid_donation(
        &self,
        donation: NewDonation,
        paid_at: DateTime<Utc>,
    ) -> Result<Donation, DonationRepositoryError>;

    async fn find_donation_by_reference(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<Donation>, DonationRepositoryError>;

    async fn get_public_case_details(
        &self,
        public_slug: &str,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError>;

    async fn get_public_case_details_for_case_id(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError>;

    async fn get_patient_current_donation_progress(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError>;

    async fn get_patient_case_donation_progress(
        &self,
        patient_id: Uuid,
        medical_case_id: Uuid,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError>;

    async fn find_case_dva(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<CaseDva>, DonationRepositoryError>;

    async fn find_case_dva_by_account_number(
        &self,
        account_number: &str,
    ) -> Result<Option<CaseDva>, DonationRepositoryError>;

    async fn upsert_case_dva(&self, dva: UpsertCaseDva)
        -> Result<CaseDva, DonationRepositoryError>;

    async fn deactivate_case_dva(
        &self,
        medical_case_id: Uuid,
        deactivation_error: Option<String>,
    ) -> Result<Option<CaseDva>, DonationRepositoryError>;

    async fn lock_pending_donation_for_confirmation(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<DonationCaseLock>, DonationRepositoryError>;

    async fn lock_pending_donation_by_account_number(
        &self,
        account_number: &str,
    ) -> Result<Option<DonationCaseLock>, DonationRepositoryError>;

    async fn mark_donation_paid(
        &self,
        update: DonationPaymentUpdate,
    ) -> Result<DonationPaymentResult, DonationRepositoryError>;

    async fn update_donation_proof(
        &self,
        update: DonationProofAttemptUpdate,
    ) -> Result<Donation, DonationRepositoryError>;

    async fn acquire_retryable_proof_jobs(
        &self,
        batch_size: i64,
        now: DateTime<Utc>,
    ) -> Result<Vec<DonationProofJob>, DonationRepositoryError>;

    async fn mark_donation_failed(
        &self,
        update: DonationFailureUpdate,
    ) -> Result<(), DonationRepositoryError>;

    async fn list_admin_donations(
        &self,
        query: AdminDonationListQuery,
    ) -> Result<Vec<AdminDonationOperation>, DonationRepositoryError>;

    async fn count_admin_donations(
        &self,
        filters: AdminDonationFilters,
    ) -> Result<i64, DonationRepositoryError>;

    async fn get_admin_donation(
        &self,
        donation_id: Uuid,
    ) -> Result<Option<AdminDonationOperation>, DonationRepositoryError>;
}
