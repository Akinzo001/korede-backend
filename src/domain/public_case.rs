use crate::domain::{donation::Donation, medical_case::MedicalCase};

#[derive(Debug, Clone)]
pub struct PublicCaseDetails {
    pub medical_case: MedicalCase,
    pub donations: Vec<Donation>,
}
