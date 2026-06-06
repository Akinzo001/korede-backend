// `DateTime<Utc>` represents a date/time stored in UTC.
use chrono::{DateTime, NaiveDate, Utc};

// `Serialize` converts Rust values to JSON.
// `Deserialize` converts JSON into Rust values.
use serde::{Deserialize, Serialize};

// `Uuid` is used for unique IDs.
use uuid::Uuid;

// The lifecycle status of a medical fundraising case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Store/return enum variants as snake_case strings in JSON.
//
// Example:
// PendingReview -> "pending_review"
#[serde(rename_all = "snake_case")]
pub enum MedicalCaseStatus {
    // Case exists but is not ready for review.
    Draft,

    // Case has been submitted and is waiting for platform review.
    PendingReview,

    // Case is live and donors can contribute.
    Active,

    // Case has reached its target funding amount.
    Funded,

    // Hospital has confirmed treatment has started.
    TreatmentCommenced,

    // Hospital has confirmed the patient has been discharged.
    Discharged,

    // Case was cancelled and should no longer accept donations.
    Cancelled,
}

impl MedicalCaseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Active => "active",
            Self::Funded => "funded",
            Self::TreatmentCommenced => "treatment_commenced",
            Self::Discharged => "discharged",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "draft" => Self::Draft,
            "pending_review" => Self::PendingReview,
            "funded" => Self::Funded,
            "treatment_commenced" => Self::TreatmentCommenced,
            "discharged" => Self::Discharged,
            "cancelled" => Self::Cancelled,
            _ => Self::Active,
        }
    }
}

// Domain model for a medical fundraising case.
//
// We use `MedicalCase` instead of `Case` because `case` can be confusing
// in programming contexts and in legal/medical contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalCase {
    // Unique case ID.
    pub id: Uuid,

    // Hospital responsible for validating and receiving settlement.
    pub hospital_id: Uuid,

    // Patient who benefits from this case.
    pub patient_id: Uuid,

    // Public campaign title.
    pub title: String,

    // Short medical summary provided/verified by the hospital.
    pub diagnosis_summary: String,

    // Total bill amount in kobo.
    //
    // We store money as integers to avoid floating-point rounding errors.
    pub bill_amount_kobo: i64,

    // Amount raised so far in kobo.
    pub amount_raised_kobo: i64,

    // Current lifecycle status.
    pub status: MedicalCaseStatus,

    // Optional admission date for the hospital case.
    pub admitted_at: Option<NaiveDate>,

    // Optional blockchain network where this case was recorded.
    //
    // Example: "sui_testnet".
    pub blockchain_network: Option<String>,

    // Optional blockchain transaction digest/hash.
    pub blockchain_tx_digest: Option<String>,

    // Optional blockchain record object/account ID.
    pub blockchain_record_id: Option<String>,

    // When this case record was created.
    pub created_at: DateTime<Utc>,

    // When this case record was last updated.
    pub updated_at: DateTime<Utc>,
}
