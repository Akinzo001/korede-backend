use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct DonationProofRequest {
    pub case_id: String,
    pub hospital_id: String,
    pub amount_kobo: u64,
    pub payment_reference: String,
}

#[derive(Debug, Clone)]
pub struct DonationProofReceipt {
    pub network: String,
    pub tx_digest: String,
}

#[derive(Debug, Error)]
pub enum DonationProofError {
    #[error("missing required Sui configuration: {0}")]
    MissingConfig(&'static str),

    #[error("failed to publish donation proof")]
    PublishFailed,

    #[error("provider error: {0}")]
    Provider(String),
}

#[async_trait]
pub trait DonationProofPublisher: Send + Sync {
    async fn publish_donation_proof(
        &self,
        request: DonationProofRequest,
    ) -> Result<DonationProofReceipt, DonationProofError>;
}
