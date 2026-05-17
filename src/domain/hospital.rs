// `DateTime<Utc>` represents a date/time stored in UTC.
use chrono::{DateTime, Utc};

// `Serialize` converts Rust values to JSON.
// `Deserialize` converts JSON into Rust values.
use serde::{Deserialize, Serialize};

// `Uuid` is used for unique IDs.
use uuid::Uuid;

// The verification state of a hospital on Korede.
//
// An enum is a type where the value must be one of the listed variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// When serialized to JSON, variants become snake_case strings.
//
// Example:
// Pending -> "pending"
// MedicalReview would become "medical_review"
#[serde(rename_all = "snake_case")]
pub enum HospitalVerificationStatus {
    // Hospital has registered but has not been verified yet.
    Pending,

    // Hospital has passed platform verification.
    Verified,

    // Hospital verification was rejected.
    Rejected,

    // Hospital was previously allowed but is now blocked/suspended.
    Suspended,
}

// Domain model for a hospital.
//
// This is a pure business object. It does not know about SQLx, Axum,
// HTTP, or PostgreSQL tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hospital {
    // Unique hospital ID.
    pub id: Uuid,

    // Public hospital name.
    pub name: String,

    // Login email for the hospital account.
    pub email: String,

    // Hashed password used for authentication.
    //
    // This must never contain the raw password.
    pub password_hash: String,

    // Optional hospital phone number.
    pub phone_number: Option<String>,

    // Physical address of the hospital.
    pub official_address: Option<String>,

    // Name of the administrator who registered the hospital account.
    pub administrator_name: Option<String>,

    // Optional CAC registration number.
    //
    // `Option<String>` means the value can be present or missing.
    pub cac_registration_number: Option<String>,

    // Optional medical license number.
    pub medical_license_number: Option<String>,

    // Account name for direct hospital settlement.
    pub corporate_account_name: String,

    // Account number for direct hospital settlement.
    pub corporate_account_number: String,

    // Bank name for direct hospital settlement.
    pub bank_name: String,

    // Current verification status.
    pub verification_status: HospitalVerificationStatus,

    // When this hospital record was created.
    pub created_at: DateTime<Utc>,

    // When this hospital record was last updated.
    pub updated_at: DateTime<Utc>,
}
