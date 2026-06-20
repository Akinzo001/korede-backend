use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct PaymentInitializationRequest {
    pub donor_email: String,
    pub donor_display_name: String,
    pub amount_kobo: i64,
    pub reference: String,
    pub case_public_slug: String,
    pub case_title: String,
    pub payment_label: String,
}

#[derive(Debug, Clone)]
pub struct PaymentInitialization {
    pub provider_reference: String,
    pub customer_code: Option<String>,
    pub dedicated_account_id: Option<i64>,
    pub bank_name: String,
    pub bank_slug: Option<String>,
    pub account_name: String,
    pub account_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentVerificationStatus {
    Pending,
    Success,
    Failed,
}

#[derive(Debug, Clone)]
pub struct PaymentVerification {
    pub provider_reference: String,
    pub amount_kobo: i64,
    pub status: PaymentVerificationStatus,
    pub paid_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum PaymentGatewayError {
    #[error("missing payment configuration: {0}")]
    MissingConfig(&'static str),

    #[error("payment provider request failed")]
    RequestFailed,

    #[error("payment provider rejected the request: {0}")]
    Provider(String),
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn initialize_payment(
        &self,
        request: PaymentInitializationRequest,
    ) -> Result<PaymentInitialization, PaymentGatewayError>;

    async fn verify_payment(
        &self,
        reference: &str,
    ) -> Result<PaymentVerification, PaymentGatewayError>;

    fn generate_reference(&self) -> String;
}
