use axum::{
    Json, Router,
    extract::{Multipart, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{AppState, error::ApiError},
    domain::{
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::{HospitalDocument, HospitalDocumentType},
    },
    port::{
        auth::AuthenticatedHospital,
        hospital::{HospitalRepositoryError, NewHospital, NewHospitalDocument},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register_hospital))
        .route("/login", post(login_hospital))
        .route("/me", get(current_hospital))
        .route("/documents/cac", post(upload_cac_document))
        .route("/documents/license", post(upload_license_document))
        .route("/documents", get(list_documents))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterHospitalRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub phone_number: Option<String>,
    pub cac_registration_number: Option<String>,
    pub medical_license_number: Option<String>,
    pub corporate_account_name: String,
    pub corporate_account_number: String,
    pub bank_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone_number: Option<String>,
    pub cac_registration_number: Option<String>,
    pub medical_license_number: Option<String>,
    pub corporate_account_name: String,
    pub corporate_account_number: String,
    pub bank_name: String,
    pub verification_status: HospitalVerificationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginHospitalRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginHospitalResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub hospital: HospitalSummaryResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalSummaryResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub verification_status: HospitalVerificationStatus,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalDocumentResponse {
    pub id: Uuid,
    pub document_type: String,
    pub status: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalDocumentsResponse {
    pub documents: Vec<HospitalDocumentResponse>,
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/register",
    tag = "Hospitals",
    request_body = RegisterHospitalRequest,
    responses(
        (status = 200, description = "Hospital registered successfully.", body = HospitalResponse),
        (status = 400, description = "Invalid registration request."),
        (status = 409, description = "Hospital email already exists.")
    )
)]
pub async fn register_hospital(
    State(state): State<AppState>,
    Json(request): Json<RegisterHospitalRequest>,
) -> Result<Json<HospitalResponse>, ApiError> {
    validate_registration(&request)?;

    let password_hash = state
        .password_hasher
        .hash_password(&request.password)
        .map_err(|_| ApiError::Internal("failed to hash password".to_owned()))?;

    let hospital = state
        .hospital_repository
        .create_hospital(NewHospital {
            name: request.name.trim().to_owned(),
            email: request.email.trim().to_lowercase(),
            password_hash,
            phone_number: request.phone_number.map(|value| value.trim().to_owned()),
            cac_registration_number: request
                .cac_registration_number
                .map(|value| value.trim().to_owned()),
            medical_license_number: request
                .medical_license_number
                .map(|value| value.trim().to_owned()),
            corporate_account_name: request.corporate_account_name.trim().to_owned(),
            corporate_account_number: request.corporate_account_number.trim().to_owned(),
            bank_name: request.bank_name.trim().to_owned(),
        })
        .await?;

    Ok(Json(HospitalResponse::from(hospital)))
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/login",
    tag = "Hospitals",
    request_body = LoginHospitalRequest,
    responses(
        (status = 200, description = "Hospital logged in successfully.", body = LoginHospitalResponse),
        (status = 401, description = "Invalid email or password.")
    )
)]
pub async fn login_hospital(
    State(state): State<AppState>,
    Json(request): Json<LoginHospitalRequest>,
) -> Result<Json<LoginHospitalResponse>, ApiError> {
    let hospital = state
        .hospital_repository
        .find_hospital_by_email(request.email.trim())
        .await?
        .ok_or_else(invalid_credentials)?;

    let password_matches = state
        .password_hasher
        .verify_password(&request.password, &hospital.password_hash)
        .map_err(|_| invalid_credentials())?;

    if !password_matches {
        return Err(invalid_credentials());
    }

    let access_token = state
        .token_service
        .create_access_token(hospital.id, &hospital.email)
        .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;

    Ok(Json(LoginHospitalResponse {
        access_token,
        token_type: "Bearer".to_owned(),
        expires_in: state.jwt_expires_in_seconds,
        hospital: HospitalSummaryResponse::from(&hospital),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/hospitals/me",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current hospital profile.", body = HospitalResponse),
        (status = 401, description = "Missing or invalid bearer token.")
    )
)]
pub async fn current_hospital(
    authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
) -> Result<Json<HospitalResponse>, ApiError> {
    let hospital = state
        .hospital_repository
        .find_hospital_by_id(authenticated.hospital_id)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    Ok(Json(HospitalResponse::from(hospital)))
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/documents/cac",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "CAC document uploaded.", body = HospitalDocumentResponse),
        (status = 401, description = "Missing or invalid bearer token."),
        (status = 413, description = "Uploaded file is too large."),
        (status = 415, description = "Unsupported file type.")
    )
)]
pub async fn upload_cac_document(
    authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<HospitalDocumentResponse>, ApiError> {
    upload_document(
        authenticated,
        state,
        multipart,
        HospitalDocumentType::CacCertificate,
    )
    .await
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/documents/license",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Medical license document uploaded.", body = HospitalDocumentResponse),
        (status = 401, description = "Missing or invalid bearer token."),
        (status = 413, description = "Uploaded file is too large."),
        (status = 415, description = "Unsupported file type.")
    )
)]
pub async fn upload_license_document(
    authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<HospitalDocumentResponse>, ApiError> {
    upload_document(
        authenticated,
        state,
        multipart,
        HospitalDocumentType::MedicalLicense,
    )
    .await
}

#[utoipa::path(
    get,
    path = "/api/v1/hospitals/documents",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Hospital KYC documents.", body = HospitalDocumentsResponse),
        (status = 401, description = "Missing or invalid bearer token.")
    )
)]
pub async fn list_documents(
    authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
) -> Result<Json<HospitalDocumentsResponse>, ApiError> {
    let documents = state
        .hospital_repository
        .list_hospital_documents(authenticated.hospital_id)
        .await?
        .into_iter()
        .map(HospitalDocumentResponse::from)
        .collect();

    Ok(Json(HospitalDocumentsResponse { documents }))
}

async fn upload_document(
    authenticated: AuthenticatedHospital,
    state: AppState,
    mut multipart: Multipart,
    document_type: HospitalDocumentType,
) -> Result<Json<HospitalDocumentResponse>, ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("invalid multipart form data".to_owned()))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let original_filename = field
            .file_name()
            .map(str::to_owned)
            .unwrap_or_else(|| "document".to_owned());

        let mime_type = field
            .content_type()
            .map(str::to_owned)
            .unwrap_or_else(|| "application/octet-stream".to_owned());

        validate_mime_type(&mime_type)?;

        let contents = field
            .bytes()
            .await
            .map_err(|_| ApiError::BadRequest("failed to read uploaded file".to_owned()))?;

        if contents.len() > state.max_upload_bytes {
            return Err(ApiError::PayloadTooLarge(
                "uploaded file is too large".to_owned(),
            ));
        }

        let stored = state
            .document_storage
            .save_document(
                authenticated.hospital_id,
                document_type.clone(),
                &original_filename,
                &mime_type,
                &contents,
            )
            .await
            .map_err(|_| ApiError::Internal("failed to store document".to_owned()))?;

        let document = state
            .hospital_repository
            .save_hospital_document(NewHospitalDocument {
                hospital_id: authenticated.hospital_id,
                document_type,
                storage_provider: stored.storage_provider,
                storage_key: stored.storage_key,
                original_filename: stored.original_filename,
                mime_type: stored.mime_type,
                file_size_bytes: stored.file_size_bytes,
            })
            .await?;

        return Ok(Json(HospitalDocumentResponse::from(document)));
    }

    Err(ApiError::BadRequest(
        "missing multipart file field".to_owned(),
    ))
}

fn validate_registration(request: &RegisterHospitalRequest) -> Result<(), ApiError> {
    if request.name.trim().is_empty()
        || request.email.trim().is_empty()
        || request.corporate_account_name.trim().is_empty()
        || request.corporate_account_number.trim().is_empty()
        || request.bank_name.trim().is_empty()
    {
        return Err(ApiError::BadRequest(
            "required fields are missing".to_owned(),
        ));
    }

    if !request.email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    if request.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters".to_owned(),
        ));
    }

    Ok(())
}

fn validate_mime_type(mime_type: &str) -> Result<(), ApiError> {
    match mime_type {
        "application/pdf" | "image/jpeg" | "image/png" => Ok(()),
        _ => Err(ApiError::UnsupportedMediaType(
            "only PDF, JPEG, and PNG files are supported".to_owned(),
        )),
    }
}

fn invalid_credentials() -> ApiError {
    ApiError::Unauthorized("invalid email or password".to_owned())
}

impl From<Hospital> for HospitalResponse {
    fn from(hospital: Hospital) -> Self {
        Self {
            id: hospital.id,
            name: hospital.name,
            email: hospital.email,
            phone_number: hospital.phone_number,
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

impl From<&Hospital> for HospitalSummaryResponse {
    fn from(hospital: &Hospital) -> Self {
        Self {
            id: hospital.id,
            name: hospital.name.clone(),
            email: hospital.email.clone(),
            verification_status: hospital.verification_status.clone(),
        }
    }
}

impl From<HospitalDocument> for HospitalDocumentResponse {
    fn from(document: HospitalDocument) -> Self {
        Self {
            id: document.id,
            document_type: document.document_type.as_str().to_owned(),
            status: document.status.as_str().to_owned(),
            original_filename: document.original_filename,
            mime_type: document.mime_type,
            file_size_bytes: document.file_size_bytes,
            uploaded_at: document.uploaded_at,
            reviewed_at: document.reviewed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_registration_request() -> RegisterHospitalRequest {
        RegisterHospitalRequest {
            name: "Lagoon Hospital".to_owned(),
            email: "admin@lagoon.example".to_owned(),
            password: "strong-password".to_owned(),
            phone_number: Some("+2348012345678".to_owned()),
            cac_registration_number: Some("RC123456".to_owned()),
            medical_license_number: Some("ML123456".to_owned()),
            corporate_account_name: "Lagoon Hospital Ltd".to_owned(),
            corporate_account_number: "0123456789".to_owned(),
            bank_name: "Wema Bank".to_owned(),
        }
    }

    #[test]
    fn registration_validation_accepts_valid_request() {
        assert!(validate_registration(&valid_registration_request()).is_ok());
    }

    #[test]
    fn registration_validation_rejects_short_password() {
        let mut request = valid_registration_request();
        request.password = "short".to_owned();

        assert!(matches!(
            validate_registration(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn registration_validation_rejects_invalid_email() {
        let mut request = valid_registration_request();
        request.email = "not-an-email".to_owned();

        assert!(matches!(
            validate_registration(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn mime_validation_accepts_supported_types() {
        assert!(validate_mime_type("application/pdf").is_ok());
        assert!(validate_mime_type("image/jpeg").is_ok());
        assert!(validate_mime_type("image/png").is_ok());
    }

    #[test]
    fn mime_validation_rejects_unsupported_types() {
        assert!(matches!(
            validate_mime_type("text/plain"),
            Err(ApiError::UnsupportedMediaType(_))
        ));
    }
}
