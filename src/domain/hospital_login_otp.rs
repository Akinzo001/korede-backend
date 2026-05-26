use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HospitalLoginOtp {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub email: String,
    pub otp_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub created_at: DateTime<Utc>,
}
