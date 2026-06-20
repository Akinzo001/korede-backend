use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DonationStatus {
    Pending,
    Paid,
    Failed,
    RejectedOverflow,
}

impl DonationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Paid => "paid",
            Self::Failed => "failed",
            Self::RejectedOverflow => "rejected_overflow",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "paid" => Self::Paid,
            "failed" => Self::Failed,
            "rejected_overflow" => Self::RejectedOverflow,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DonationProofStatus {
    Pending,
    Published,
    Failed,
}

impl DonationProofStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Published => "published",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "published" => Self::Published,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Donation {
    pub id: Uuid,
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
    pub status: DonationStatus,
    pub paid_at: Option<DateTime<Utc>>,
    pub proof_status: DonationProofStatus,
    pub sui_network: Option<String>,
    pub sui_tx_digest: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
