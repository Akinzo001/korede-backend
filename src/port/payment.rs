use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentMethod {
    Checkout,
    DvaTransfer,
}

impl PaymentMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Checkout => "checkout",
            Self::DvaTransfer => "dva_transfer",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "checkout" => Some(Self::Checkout),
            "dva_transfer" => Some(Self::DvaTransfer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckoutInitializationRequest {
    pub donor_email: String,
    pub donor_display_name: String,
    pub amount_kobo: i64,
    pub reference: String,
    pub callback_url: String,
    pub case_public_slug: String,
    pub case_title: String,
}

#[derive(Debug, Clone)]
pub struct DvaAssignmentRequest {
    pub customer_email: String,
    pub payment_label: String,
    pub case_public_slug: String,
    pub case_title: String,
    pub reference: String,
}

#[derive(Debug, Clone)]
pub struct CheckoutInitialization {
    pub provider_reference: String,
    pub authorization_url: String,
    pub access_code: String,
}

#[derive(Debug, Clone)]
pub struct DvaAssignment {
    pub provider_reference: String,
    pub customer_code: Option<String>,
    pub dedicated_account_id: i64,
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
    async fn initialize_checkout(
        &self,
        request: CheckoutInitializationRequest,
    ) -> Result<CheckoutInitialization, PaymentGatewayError>;

    async fn ensure_case_dva(
        &self,
        request: DvaAssignmentRequest,
    ) -> Result<DvaAssignment, PaymentGatewayError>;

    async fn deactivate_dva(&self, dedicated_account_id: i64) -> Result<(), PaymentGatewayError>;

    async fn verify_payment(
        &self,
        reference: &str,
    ) -> Result<PaymentVerification, PaymentGatewayError>;

    fn generate_reference(&self) -> String;
}
