use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose};
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
        email::EmailMessage,
        hospital::{HospitalRepositoryError, NewHospital, NewHospitalDocument},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register_hospital))
        .route("/login", post(login_hospital))
        .route("/me", get(current_hospital))
        .route("/documents", get(list_documents))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterHospitalRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub phone_number: Option<String>,
    pub official_address: String,
    pub administrator_name: String,
    pub cac_registration_number: Option<String>,
    pub medical_license_number: Option<String>,
    pub corporate_account_name: String,
    pub corporate_account_number: String,
    pub bank_name: String,
    pub terms_accepted: bool,
    pub cac_document: Base64DocumentRequest,
    pub medical_license_document: Base64DocumentRequest,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Base64DocumentRequest {
    pub original_filename: String,
    pub mime_type: String,
    pub content_base64: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterHospitalResponse {
    pub hospital: HospitalResponse,
    pub documents: Vec<HospitalDocumentResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
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
        (status = 200, description = "Hospital registered successfully with KYC documents.", body = RegisterHospitalResponse),
        (status = 400, description = "Invalid registration request."),
        (status = 413, description = "Uploaded document is too large."),
        (status = 415, description = "Unsupported document type."),
        (status = 409, description = "Hospital email already exists.")
    )
)]
pub async fn register_hospital(
    State(state): State<AppState>,
    Json(request): Json<RegisterHospitalRequest>,
) -> Result<Json<RegisterHospitalResponse>, ApiError> {
    validate_registration(&request)?;

    let cac_document = decode_base64_document(&request.cac_document, state.max_upload_bytes)?;
    let medical_license_document =
        decode_base64_document(&request.medical_license_document, state.max_upload_bytes)?;

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
            official_address: request.official_address.trim().to_owned(),
            administrator_name: request.administrator_name.trim().to_owned(),
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

    let cac_document = store_registration_document(
        &state,
        hospital.id,
        HospitalDocumentType::CacCertificate,
        &request.cac_document,
        &cac_document,
    )
    .await?;

    let medical_license_document = store_registration_document(
        &state,
        hospital.id,
        HospitalDocumentType::MedicalLicense,
        &request.medical_license_document,
        &medical_license_document,
    )
    .await?;

    send_registration_acknowledgement(&state, &hospital).await;

    Ok(Json(RegisterHospitalResponse {
        hospital: HospitalResponse::from(hospital),
        documents: vec![
            HospitalDocumentResponse::from(cac_document),
            HospitalDocumentResponse::from(medical_license_document),
        ],
    }))
}

async fn send_registration_acknowledgement(state: &AppState, hospital: &Hospital) {
    let subject = "Korede Health verification request received".to_owned();
    let text_body = format!(
        "Hello {},\n\nYour hospital registration and verification documents have been received.\n\nOur team will review your CAC certificate, medical license, and hospital details. Once your credentials are verified, you will be notified by email.\n\nThank you,\nKorede Health",
        hospital.name
    );
    let html_body = format!(
        "<p>Hello {},</p><p>Your hospital registration and verification documents have been received.</p><p>Our team will review your CAC certificate, medical license, and hospital details. Once your credentials are verified, you will be notified by email.</p><p>Thank you,<br>Korede Health</p>",
        hospital.name
    );

    if let Err(error) = state
        .email_service
        .send(EmailMessage {
            to_email: hospital.email.clone(),
            to_name: hospital.administrator_name.clone(),
            subject,
            text_body,
            html_body: Some(html_body),
        })
        .await
    {
        tracing::error!(%error, hospital_id = %hospital.id, "failed to send hospital registration acknowledgement email");
    }
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

fn validate_registration(request: &RegisterHospitalRequest) -> Result<(), ApiError> {
    if request.name.trim().is_empty()
        || request.email.trim().is_empty()
        || request.official_address.trim().is_empty()
        || request.administrator_name.trim().is_empty()
        || request.corporate_account_name.trim().is_empty()
        || request.corporate_account_number.trim().is_empty()
        || request.bank_name.trim().is_empty()
        || request.cac_document.original_filename.trim().is_empty()
        || request.cac_document.mime_type.trim().is_empty()
        || request.cac_document.content_base64.trim().is_empty()
        || request
            .medical_license_document
            .original_filename
            .trim()
            .is_empty()
        || request.medical_license_document.mime_type.trim().is_empty()
        || request
            .medical_license_document
            .content_base64
            .trim()
            .is_empty()
    {
        return Err(ApiError::BadRequest(
            "required fields are missing".to_owned(),
        ));
    }

    if !request.terms_accepted {
        return Err(ApiError::BadRequest(
            "terms must be accepted before registration".to_owned(),
        ));
    }

    if !request.email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    if request.password.len() < 12 {
        return Err(ApiError::BadRequest(
            "password must be at least 12 characters".to_owned(),
        ));
    }

    validate_mime_type(request.cac_document.mime_type.trim())?;
    validate_mime_type(request.medical_license_document.mime_type.trim())?;

    Ok(())
}

fn decode_base64_document(
    document: &Base64DocumentRequest,
    max_upload_bytes: usize,
) -> Result<Vec<u8>, ApiError> {
    let encoded = document.content_base64.trim();
    let encoded = encoded
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(encoded);

    let contents = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| ApiError::BadRequest("document base64 is invalid".to_owned()))?;

    if contents.is_empty() {
        return Err(ApiError::BadRequest(
            "document content cannot be empty".to_owned(),
        ));
    }

    if contents.len() > max_upload_bytes {
        return Err(ApiError::PayloadTooLarge(
            "uploaded document is too large".to_owned(),
        ));
    }

    Ok(contents)
}

async fn store_registration_document(
    state: &AppState,
    hospital_id: Uuid,
    document_type: HospitalDocumentType,
    request: &Base64DocumentRequest,
    contents: &[u8],
) -> Result<HospitalDocument, ApiError> {
    let stored = state
        .document_storage
        .save_document(
            hospital_id,
            document_type.clone(),
            request.original_filename.trim(),
            request.mime_type.trim(),
            contents,
        )
        .await
        .map_err(|_| ApiError::Internal("failed to store document".to_owned()))?;

    state
        .hospital_repository
        .save_hospital_document(NewHospitalDocument {
            hospital_id,
            document_type,
            storage_provider: stored.storage_provider,
            storage_key: stored.storage_key,
            original_filename: stored.original_filename,
            mime_type: stored.mime_type,
            file_size_bytes: stored.file_size_bytes,
        })
        .await
        .map_err(ApiError::from)
}

fn validate_mime_type(mime_type: &str) -> Result<(), ApiError> {
    match mime_type {
        "application/pdf" => Ok(()),
        _ => Err(ApiError::UnsupportedMediaType(
            "only PDF files are supported".to_owned(),
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
            official_address: "1 Hospital Road, Lagos".to_owned(),
            administrator_name: "Dr Jane Doe".to_owned(),
            cac_registration_number: Some("RC123456".to_owned()),
            medical_license_number: Some("ML123456".to_owned()),
            corporate_account_name: "Lagoon Hospital Ltd".to_owned(),
            corporate_account_number: "0123456789".to_owned(),
            bank_name: "Wema Bank".to_owned(),
            terms_accepted: true,
            cac_document: Base64DocumentRequest {
                original_filename: "cac.pdf".to_owned(),
                mime_type: "application/pdf".to_owned(),
                content_base64: "aGVsbG8=".to_owned(),
            },
            medical_license_document: Base64DocumentRequest {
                original_filename: "license.pdf".to_owned(),
                mime_type: "application/pdf".to_owned(),
                content_base64: "aGVsbG8=".to_owned(),
            },
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
    fn registration_validation_rejects_missing_terms_acceptance() {
        let mut request = valid_registration_request();
        request.terms_accepted = false;

        assert!(matches!(
            validate_registration(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn base64_document_decoding_accepts_plain_base64() {
        let request = Base64DocumentRequest {
            original_filename: "cac.pdf".to_owned(),
            mime_type: "application/pdf".to_owned(),
            content_base64: "aGVsbG8=".to_owned(),
        };

        assert_eq!(decode_base64_document(&request, 10).unwrap(), b"hello");
    }

    #[test]
    fn base64_document_decoding_accepts_data_urls() {
        let request = Base64DocumentRequest {
            original_filename: "cac.pdf".to_owned(),
            mime_type: "application/pdf".to_owned(),
            content_base64: "data:application/pdf;base64,aGVsbG8=".to_owned(),
        };

        assert_eq!(decode_base64_document(&request, 10).unwrap(), b"hello");
    }

    #[test]
    fn mime_validation_accepts_pdf() {
        assert!(validate_mime_type("application/pdf").is_ok());
    }

    #[test]
    fn mime_validation_rejects_unsupported_types() {
        assert!(matches!(
            validate_mime_type("image/jpeg"),
            Err(ApiError::UnsupportedMediaType(_))
        ));
        assert!(matches!(
            validate_mime_type("image/png"),
            Err(ApiError::UnsupportedMediaType(_))
        ));
        assert!(matches!(
            validate_mime_type("text/plain"),
            Err(ApiError::UnsupportedMediaType(_))
        ));
    }
}
