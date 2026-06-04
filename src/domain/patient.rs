// `DateTime<Utc>` represents a date/time stored in UTC.
use chrono::{DateTime, NaiveDate, Utc};

// `Serialize` converts Rust values to JSON.
// `Deserialize` converts JSON into Rust values.
use serde::{Deserialize, Serialize};

// `Uuid` is used for unique IDs.
use uuid::Uuid;

// Domain model for a patient.
//
// For now this intentionally stores only minimal patient information
// because medical data is sensitive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patient {
    // Unique patient ID.
    pub id: Uuid,

    // Unique username used by hospitals to attach cases to this patient.
    pub username: String,

    // Patient's first name.
    pub first_name: Option<String>,

    // Patient's last name.
    pub last_name: Option<String>,

    // Patient's full name.
    pub full_name: String,

    // Optional email address.
    pub email: Option<String>,

    // Whether the patient's email address has been verified.
    pub email_verified: bool,

    // When the patient's email address was verified.
    pub email_verified_at: Option<DateTime<Utc>>,

    // Password hash for patient account login.
    pub password_hash: Option<String>,

    // Optional date of birth.
    pub date_of_birth: Option<NaiveDate>,

    // Patient age.
    //
    // Optional because some cases may not provide it immediately.
    pub age: Option<i32>,

    // Patient gender.
    //
    // Optional for now to keep intake flexible.
    pub gender: Option<String>,

    // Patient or guardian phone number.
    //
    // Optional because contact details may be managed by the hospital.
    pub phone_number: Option<String>,

    // Whether the patient has agreed for their case to be listed/shared.
    pub consent_given: bool,

    // When this patient record was created.
    pub created_at: DateTime<Utc>,

    // When this patient record was last updated.
    pub updated_at: DateTime<Utc>,
}
