use axum::{Json, Router, extract::State, routing::post};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{AppState, error::ApiError},
    domain::patient::Patient,
    port::patient::{NewPatient, PatientRepositoryError},
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/register", post(register_patient))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterPatientRequest {
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub password: String,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub phone_number: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterPatientResponse {
    pub patient: PatientResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PatientResponse {
    pub id: Uuid,
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub full_name: String,
    pub email: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub phone_number: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/api/v1/patients/register",
    tag = "Patients",
    request_body = RegisterPatientRequest,
    responses(
        (status = 200, description = "Patient registered successfully.", body = RegisterPatientResponse),
        (status = 400, description = "Invalid registration request."),
        (status = 409, description = "Patient username or email already exists.")
    )
)]
pub async fn register_patient(
    State(state): State<AppState>,
    Json(request): Json<RegisterPatientRequest>,
) -> Result<Json<RegisterPatientResponse>, ApiError> {
    validate_patient_registration(&request)?;

    let username = normalize_username(&request.username)?;

    if state
        .patient_repository
        .find_patient_by_username(&username)
        .await?
        .is_some()
    {
        return Err(PatientRepositoryError::DuplicateUsername.into());
    }

    let password_hash = state
        .password_hasher
        .hash_password(&request.password)
        .map_err(|_| ApiError::Internal("failed to hash password".to_owned()))?;

    let patient = state
        .patient_repository
        .create_patient(NewPatient {
            username,
            first_name: request.first_name.trim().to_owned(),
            last_name: request.last_name.trim().to_owned(),
            email: request
                .email
                .as_ref()
                .map(|email| email.trim().to_lowercase())
                .filter(|email| !email.is_empty()),
            password_hash,
            date_of_birth: request.date_of_birth,
            gender: request
                .gender
                .as_ref()
                .map(|gender| gender.trim().to_lowercase())
                .filter(|gender| !gender.is_empty()),
            phone_number: request
                .phone_number
                .as_ref()
                .map(|phone_number| phone_number.trim().to_owned())
                .filter(|phone_number| !phone_number.is_empty()),
        })
        .await?;

    Ok(Json(RegisterPatientResponse {
        patient: PatientResponse::from(patient),
    }))
}

fn validate_patient_registration(request: &RegisterPatientRequest) -> Result<(), ApiError> {
    normalize_username(&request.username)?;

    if request.first_name.trim().is_empty() {
        return Err(ApiError::BadRequest("first_name is required".to_owned()));
    }

    if request.last_name.trim().is_empty() {
        return Err(ApiError::BadRequest("last_name is required".to_owned()));
    }

    if request.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters".to_owned(),
        ));
    }

    if let Some(email) = &request.email {
        let email = email.trim();
        if !email.is_empty() && (!email.contains('@') || !email.contains('.')) {
            return Err(ApiError::BadRequest("email is invalid".to_owned()));
        }
    }

    Ok(())
}

fn normalize_username(username: &str) -> Result<String, ApiError> {
    let username = username.trim().to_lowercase();

    if username.len() < 3 || username.len() > 32 {
        return Err(ApiError::BadRequest(
            "username must be between 3 and 32 characters".to_owned(),
        ));
    }

    if !username
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return Err(ApiError::BadRequest(
            "username may only contain letters, numbers, underscores, and hyphens".to_owned(),
        ));
    }

    Ok(username)
}

impl From<Patient> for PatientResponse {
    fn from(patient: Patient) -> Self {
        Self {
            id: patient.id,
            username: patient.username,
            first_name: patient.first_name,
            last_name: patient.last_name,
            full_name: patient.full_name,
            email: patient.email,
            date_of_birth: patient.date_of_birth,
            gender: patient.gender,
            phone_number: patient.phone_number,
            created_at: patient.created_at,
            updated_at: patient.updated_at,
        }
    }
}
