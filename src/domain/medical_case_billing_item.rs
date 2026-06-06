use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalCaseBillingItem {
    pub id: Uuid,
    pub medical_case_id: Uuid,
    pub description: String,
    pub amount_kobo: i64,
    pub created_at: DateTime<Utc>,
}
