use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{error::ApiError, AppState},
    domain::{
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::HospitalDocument,
        patient_declaration::PatientDeclaration,
    },
    port::{auth::AuthenticatedAdmin, hospital::HospitalRepositoryError},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/hospitals", get(list_hospitals))
        .route("/hospitals/:hospital_id", get(get_hospital))
        .route(
            "/hospitals/:hospital_id/documents",
            get(list_hospital_documents),
        )
        .route(
            "/patients/:username/declaration",
            get(get_patient_declaration),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminLoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub phone_number: Option<String>,
    pub official_address: Option<String>,
    pub administrator_name: Option<String>,
    pub cac_registration_number: Option<String>,
    pub medical_license_number: Option<String>,
    pub corporate_account_name: String,
    pub corporate_account_number: String,
    pub bank_name: String,
    pub verification_status: HospitalVerificationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalsResponse {
    pub hospitals: Vec<AdminHospitalResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalDocumentResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub document_type: String,
    pub storage_provider: String,
    pub storage_key: String,
    pub status: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalDocumentsResponse {
    pub documents: Vec<AdminHospitalDocumentResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPatientDeclarationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub statement: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn login_admin(
    State(state): State<AppState>,
    Json(request): Json<AdminLoginRequest>,
) -> Result<Json<AdminLoginResponse>, ApiError> {
    validate_admin_login_request(&request)?;

    let email = request.email.trim().to_lowercase();
    let password = request.password.trim();

    if email != state.super_admin_email
        || !constant_time_eq(password.as_bytes(), state.super_admin_password.as_bytes())
    {
        return Err(invalid_admin_credentials());
    }

    let access_token = state
        .token_service
        .create_admin_access_token(&state.super_admin_email)
        .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;

    Ok(Json(AdminLoginResponse {
        access_token,
        token_type: "Bearer".to_owned(),
        expires_in: state.jwt_expires_in_seconds,
        role: "admin".to_owned(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals",
    tag = "Admin",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "All registered hospitals.", body = AdminHospitalsResponse),
        (status = 401, description = "Missing or invalid admin bearer token.")
    )
)]
pub async fn list_hospitals(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
) -> Result<Json<AdminHospitalsResponse>, ApiError> {
    let hospitals = state
        .hospital_repository
        .list_hospitals()
        .await?
        .into_iter()
        .map(AdminHospitalResponse::from)
        .collect();

    Ok(Json(AdminHospitalsResponse { hospitals }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals/{hospital_id}",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("hospital_id" = Uuid, Path, description = "Hospital ID")
    ),
    responses(
        (status = 200, description = "Hospital details.", body = AdminHospitalResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital not found.")
    )
)]
pub async fn get_hospital(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(hospital_id): Path<Uuid>,
) -> Result<Json<AdminHospitalResponse>, ApiError> {
    let hospital = state
        .hospital_repository
        .find_hospital_by_id(hospital_id)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    Ok(Json(AdminHospitalResponse::from(hospital)))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals/{hospital_id}/documents",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("hospital_id" = Uuid, Path, description = "Hospital ID")
    ),
    responses(
        (status = 200, description = "Hospital document metadata.", body = AdminHospitalDocumentsResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital not found.")
    )
)]
pub async fn list_hospital_documents(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(hospital_id): Path<Uuid>,
) -> Result<Json<AdminHospitalDocumentsResponse>, ApiError> {
    state
        .hospital_repository
        .find_hospital_by_id(hospital_id)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    let documents = state
        .hospital_repository
        .list_hospital_documents(hospital_id)
        .await?
        .into_iter()
        .map(AdminHospitalDocumentResponse::from)
        .collect();

    Ok(Json(AdminHospitalDocumentsResponse { documents }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/patients/{username}/declaration",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("username" = String, Path, description = "Patient username")
    ),
    responses(
        (status = 200, description = "Patient declaration by username.", body = AdminPatientDeclarationResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Patient declaration was not found.")
    )
)]
pub async fn get_patient_declaration(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<AdminPatientDeclarationResponse>, ApiError> {
    let declaration = state
        .patient_declaration_repository
        .find_patient_declaration_by_username(&username)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    Ok(Json(AdminPatientDeclarationResponse::from(declaration)))
}

fn validate_admin_login_request(request: &AdminLoginRequest) -> Result<(), ApiError> {
    if request.email.trim().is_empty() || !request.email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    if request.password.trim().is_empty() {
        return Err(ApiError::BadRequest("password is required".to_owned()));
    }

    Ok(())
}

fn invalid_admin_credentials() -> ApiError {
    ApiError::Unauthorized("invalid admin credentials".to_owned())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    let max_len = left.len().max(right.len());

    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        diff |= (left_byte ^ right_byte) as usize;
    }

    diff == 0
}

impl From<Hospital> for AdminHospitalResponse {
    fn from(hospital: Hospital) -> Self {
        Self {
            id: hospital.id,
            name: hospital.name,
            email: hospital.email,
            email_verified: hospital.email_verified,
            email_verified_at: hospital.email_verified_at,
            phone_number: hospital.phone_number,
            official_address: hospital.official_address,
            administrator_name: hospital.administrator_name,
            cac_registration_number: hospital.cac_registration_number,
            medical_license_number: hospital.medical_license_number,
            corporate_account_name: hospital.corporate_account_name,
            corporate_account_number: hospital.corporate_account_number,
            bank_name: hospital.bank_name,
            verification_status: hospital.verification_status,
            created_at: hospital.created_at,
            updated_at: hospital.updated_at,
        }
    }
}

impl From<HospitalDocument> for AdminHospitalDocumentResponse {
    fn from(document: HospitalDocument) -> Self {
        Self {
            id: document.id,
            hospital_id: document.hospital_id,
            document_type: document.document_type.as_str().to_owned(),
            storage_provider: document.storage_provider.as_str().to_owned(),
            storage_key: document.storage_key,
            status: document.status.as_str().to_owned(),
            original_filename: document.original_filename,
            mime_type: document.mime_type,
            file_size_bytes: document.file_size_bytes,
            uploaded_at: document.uploaded_at,
            reviewed_at: document.reviewed_at,
        }
    }
}

impl From<PatientDeclaration> for AdminPatientDeclarationResponse {
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

    #[test]
    fn validates_admin_login_request() {
        let request = AdminLoginRequest {
            email: "admin@example.com".to_owned(),
            password: "password".to_owned(),
        };

        assert!(validate_admin_login_request(&request).is_ok());
    }

    #[test]
    fn rejects_invalid_admin_email() {
        let request = AdminLoginRequest {
            email: "admin".to_owned(),
            password: "password".to_owned(),
        };

        assert!(matches!(
            validate_admin_login_request(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn constant_time_eq_matches_equal_values() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"wrong"));
        assert!(!constant_time_eq(b"secret", b"secret1"));
    }
}
