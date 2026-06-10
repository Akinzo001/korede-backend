use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{error::ApiError, AppState},
    domain::{medical_case::MedicalCase, patient_declaration::PatientDeclaration},
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/:public_slug", get(get_case_by_public_slug))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicMedicalCaseResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub patient_id: Uuid,
    pub patient_declaration: PublicPatientDeclarationResponse,
    pub title: String,
    pub public_slug: String,
    pub public_link: String,
    pub diagnosis_summary: String,
    pub bill_amount_kobo: i64,
    pub amount_raised_kobo: i64,
    pub remaining_amount_kobo: i64,
    pub status: String,
    pub admitted_at: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicPatientDeclarationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub statement: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[utoipa::path(
    get,
    path = "/cases/{public_slug}",
    tag = "Cases",
    params(
        ("public_slug" = String, Path, description = "Public medical case slug")
    ),
    responses(
        (status = 200, description = "Public medical case details.", body = PublicMedicalCaseResponse),
        (status = 404, description = "Medical case was not found.")
    )
)]
pub async fn get_case_by_public_slug(
    State(state): State<AppState>,
    Path(public_slug): Path<String>,
) -> Result<Json<PublicMedicalCaseResponse>, ApiError> {
    let public_slug = public_slug.trim();
    if public_slug.is_empty() {
        return Err(ApiError::NotFound("medical case not found".to_owned()));
    }

    let medical_case = state
        .medical_case_repository
        .find_case_by_public_slug(public_slug)
        .await?
        .ok_or_else(|| ApiError::NotFound("medical case not found".to_owned()))?;

    let declaration = state
        .patient_declaration_repository
        .find_patient_declaration(medical_case.patient_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    Ok(Json(PublicMedicalCaseResponse::from_parts(
        medical_case,
        declaration,
    )))
}

fn public_case_link(public_slug: &str) -> String {
    format!("/cases/{public_slug}")
}

impl PublicMedicalCaseResponse {
    fn from_parts(medical_case: MedicalCase, declaration: PatientDeclaration) -> Self {
        let public_slug = medical_case.public_slug.unwrap_or_default();
        let remaining_amount_kobo =
            (medical_case.bill_amount_kobo - medical_case.amount_raised_kobo).max(0);

        Self {
            id: medical_case.id,
            hospital_id: medical_case.hospital_id,
            patient_id: medical_case.patient_id,
            patient_declaration: PublicPatientDeclarationResponse::from(declaration),
            title: medical_case.title,
            public_slug: public_slug.clone(),
            public_link: public_case_link(&public_slug),
            diagnosis_summary: medical_case.diagnosis_summary,
            bill_amount_kobo: medical_case.bill_amount_kobo,
            amount_raised_kobo: medical_case.amount_raised_kobo,
            remaining_amount_kobo,
            status: medical_case.status.as_str().to_owned(),
            admitted_at: medical_case.admitted_at,
            created_at: medical_case.created_at,
            updated_at: medical_case.updated_at,
        }
    }
}

impl From<PatientDeclaration> for PublicPatientDeclarationResponse {
    fn from(declaration: PatientDeclaration) -> Self {
        Self {
            id: declaration.id,
            patient_id: declaration.patient_id,
            statement: declaration.statement,
            created_at: declaration.created_at,
            updated_at: declaration.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::medical_case::MedicalCaseStatus;

    fn patient_declaration(now: DateTime<Utc>) -> PatientDeclaration {
        PatientDeclaration {
            id: Uuid::new_v4(),
            patient_id: Uuid::new_v4(),
            statement: "I consent to having this case shared for treatment funding.".to_owned(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn public_medical_case_response_includes_remaining_amount() {
        let now = Utc::now();
        let declaration = patient_declaration(now);
        let medical_case = MedicalCase {
            id: Uuid::new_v4(),
            hospital_id: Uuid::new_v4(),
            patient_id: declaration.patient_id,
            title: "Right femur fracture surgery".to_owned(),
            public_slug: Some("oluwaseun34-case-12345678".to_owned()),
            diagnosis_summary: "Patient sustained a severe comminuted fracture.".to_owned(),
            bill_amount_kobo: 150_000_000,
            amount_raised_kobo: 45_000_000,
            status: MedicalCaseStatus::Active,
            admitted_at: NaiveDate::from_ymd_opt(2026, 6, 1),
            blockchain_network: None,
            blockchain_tx_digest: None,
            blockchain_record_id: None,
            created_at: now,
            updated_at: now,
        };

        let response = PublicMedicalCaseResponse::from_parts(medical_case, declaration);

        assert_eq!(response.public_slug, "oluwaseun34-case-12345678");
        assert_eq!(response.public_link, "/cases/oluwaseun34-case-12345678");
        assert_eq!(response.amount_raised_kobo, 45_000_000);
        assert_eq!(response.remaining_amount_kobo, 105_000_000);
        assert_eq!(
            response.patient_declaration.statement,
            "I consent to having this case shared for treatment funding."
        );
    }

    #[test]
    fn public_medical_case_response_clamps_remaining_amount_at_zero() {
        let now = Utc::now();
        let declaration = patient_declaration(now);
        let medical_case = MedicalCase {
            id: Uuid::new_v4(),
            hospital_id: Uuid::new_v4(),
            patient_id: declaration.patient_id,
            title: "Emergency surgery".to_owned(),
            public_slug: Some("case-12345678".to_owned()),
            diagnosis_summary: "Urgent care needed.".to_owned(),
            bill_amount_kobo: 1_000,
            amount_raised_kobo: 1_500,
            status: MedicalCaseStatus::Funded,
            admitted_at: None,
            blockchain_network: None,
            blockchain_tx_digest: None,
            blockchain_record_id: None,
            created_at: now,
            updated_at: now,
        };

        let response = PublicMedicalCaseResponse::from_parts(medical_case, declaration);

        assert_eq!(response.remaining_amount_kobo, 0);
    }
}
