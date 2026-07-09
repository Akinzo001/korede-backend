use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HospitalSettlementStatus {
    Pending,
    RecipientCreated,
    Processing,
    OtpRequired,
    Paid,
    Failed,
    Reversed,
    FailedConfig,
    BankDetailsRequired,
}

impl HospitalSettlementStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::RecipientCreated => "recipient_created",
            Self::Processing => "processing",
            Self::OtpRequired => "otp_required",
            Self::Paid => "paid",
            Self::Failed => "failed",
            Self::Reversed => "reversed",
            Self::FailedConfig => "failed_config",
            Self::BankDetailsRequired => "bank_details_required",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "recipient_created" => Self::RecipientCreated,
            "processing" => Self::Processing,
            "otp_required" => Self::OtpRequired,
            "paid" => Self::Paid,
            "failed" => Self::Failed,
            "reversed" => Self::Reversed,
            "failed_config" => Self::FailedConfig,
            "bank_details_required" => Self::BankDetailsRequired,
            _ => Self::Pending,
        }
    }

    pub fn can_retry(&self) -> bool {
        matches!(
            self,
            Self::Pending
                | Self::RecipientCreated
                | Self::Failed
                | Self::FailedConfig
                | Self::BankDetailsRequired
                | Self::Reversed
        )
    }

    pub fn requires_admin_action(&self) -> bool {
        matches!(
            self,
            Self::Failed
                | Self::FailedConfig
                | Self::BankDetailsRequired
                | Self::Reversed
                | Self::OtpRequired
        )
    }

    pub fn admin_action_required_values() -> &'static [&'static str] {
        &[
            "failed",
            "failed_config",
            "bank_details_required",
            "reversed",
            "otp_required",
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HospitalSettlement {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub medical_case_id: Uuid,
    pub amount_kobo: i64,
    pub status: HospitalSettlementStatus,
    pub settlement_reference: String,
    pub bank_name: String,
    pub bank_code: Option<String>,
    pub account_name: String,
    pub account_number: String,
    pub paystack_recipient_code: Option<String>,
    pub paystack_transfer_code: Option<String>,
    pub paystack_transfer_id: Option<i64>,
    pub paystack_status: Option<String>,
    pub failure_reason: Option<String>,
    pub initiated_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
