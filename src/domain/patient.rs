use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patient {
    pub id: Uuid,
    pub full_name: String,
    pub age: Option<i32>,
    pub gender: Option<String>,
    pub phone_number: Option<String>,
    pub consent_given: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
