// `DateTime<Utc>` represents a date/time stored in UTC.
use chrono::{DateTime, Utc};

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

    // Optional Solana transaction/reference once blockchain recording exists.
    pub solana_reference: Option<String>,

    // When this case record was created.
    pub created_at: DateTime<Utc>,

    // When this case record was last updated.
    pub updated_at: DateTime<Utc>,
}
