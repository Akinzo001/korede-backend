use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    donation::{Donation, DonationStatus},
    medical_case::MedicalCase,
    public_case::PublicCaseDetails,
};

#[derive(Debug, Clone)]
pub struct NewDonation {
    pub medical_case_id: Uuid,
    pub donor_display_name: String,
    pub donor_email: String,
    pub amount_kobo: i64,
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
}

#[derive(Debug, Clone)]
pub struct DonationPaymentUpdate {
    pub donation_id: Uuid,
    pub paystack_transaction_reference: String,
    pub paid_at: chrono::DateTime<chrono::Utc>,
    pub proof_status: crate::domain::donation::DonationProofStatus,
    pub sui_network: Option<String>,
    pub sui_tx_digest: Option<String>,
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

#[derive(Debug, Error)]
pub enum DonationRepositoryError {
    #[error("donation not found")]
    NotFound,

    #[error("donation reference already exists")]
    DuplicateReference,

    #[error("payment amount exceeds remaining case amount")]
    AmountExceedsRemaining,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait DonationRepository: Send + Sync {
    async fn create_pending_donation(
        &self,
        donation: NewDonation,
    ) -> Result<Donation, DonationRepositoryError>;

    async fn find_donation_by_reference(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<Donation>, DonationRepositoryError>;

    async fn get_public_case_details(
        &self,
        public_slug: &str,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError>;

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
    ) -> Result<Donation, DonationRepositoryError>;

    async fn mark_donation_failed(
        &self,
        update: DonationFailureUpdate,
    ) -> Result<(), DonationRepositoryError>;
}
