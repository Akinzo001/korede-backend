use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{error::ApiError, tokens::issue_refresh_token, AppState},
    domain::{
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::{HospitalDocument, HospitalDocumentType},
        medical_case::MedicalCase,
        medical_case_billing_item::MedicalCaseBillingItem,
        medical_case_document::MedicalCaseDocument,
        patient::Patient,
        patient_declaration::PatientDeclaration,
    },
    port::{
        auth::AuthenticatedHospital,
        email::EmailMessage,
        hospital::{
            HospitalRepositoryError, NewHospital, NewHospitalAuditLog, NewHospitalDocument,
            NewHospitalEmailOtp, NewHospitalLoginOtp,
        },
        medical_case::{NewMedicalCase, NewMedicalCaseBillingItem, NewMedicalCaseDocument},
    },
};

const OTP_LENGTH: usize = 6;
const OTP_EXPIRES_IN_SECONDS: i64 = 300;
const OTP_MAX_ATTEMPTS: i32 = 5;
const OTP_RESEND_COOLDOWN_SECONDS: i64 = 60;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register_hospital))
        .route("/verify-email", post(verify_hospital_email))
        .route("/resend-otp", post(resend_hospital_email_otp))
        .route("/me", get(current_hospital))
        .route("/documents", get(list_documents))
        .route("/patients/:username", get(find_patient))
        .route(
            "/patients/:username/declaration",
            get(get_patient_declaration),
        )
        .route("/cases", post(create_case))
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
    pub hospital_id: Uuid,
    pub email: String,
    pub email_verification_required: bool,
    pub otp_expires_in_seconds: i64,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyHospitalEmailRequest {
    pub email: String,
    pub otp: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyHospitalEmailResponse {
    pub hospital_id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub verification_status: HospitalVerificationStatus,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResendHospitalEmailOtpRequest {
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResendHospitalEmailOtpResponse {
    pub email: String,
    pub otp_expires_in_seconds: i64,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalResponse {
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginHospitalRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginHospitalResponse {
    pub login_challenge_id: Uuid,
    pub email: String,
    pub otp_required: bool,
    pub otp_expires_in_seconds: i64,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyLoginOtpRequest {
    pub login_challenge_id: Uuid,
    pub otp: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyLoginOtpResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_expires_in: i64,
    pub dashboard_access: String,
    pub hospital: HospitalSummaryResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalSummaryResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub email_verified: bool,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalPatientDeclarationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub statement: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalPatientLookupResponse {
    pub patient: HospitalPatientLookupPatientResponse,
    pub declaration: HospitalPatientLookupDeclarationResponse,
    pub can_create_case: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalPatientLookupPatientResponse {
    pub id: Uuid,
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email_verified: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalPatientLookupDeclarationResponse {
    pub exists: bool,
    pub statement: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateHospitalCaseRequest {
    pub patient_username: String,
    pub title: String,
    pub diagnosis_summary: String,
    pub admitted_at: Option<NaiveDate>,
    pub billing_items: Vec<CreateHospitalCaseBillingItemRequest>,
    #[serde(default)]
    pub documents: Vec<CreateHospitalCaseDocumentRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateHospitalCaseBillingItemRequest {
    pub description: String,
    pub amount_kobo: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateHospitalCaseDocumentRequest {
    pub document_type: String,
    pub original_filename: String,
    pub mime_type: String,
    pub content_base64: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateHospitalCaseResponse {
    pub case: HospitalCaseResponse,
    pub patient: HospitalPatientLookupPatientResponse,
    pub patient_declaration: HospitalPatientDeclarationResponse,
    pub billing_items: Vec<HospitalCaseBillingItemResponse>,
    pub documents: Vec<HospitalCaseDocumentResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalCaseResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub patient_id: Uuid,
    pub title: String,
    pub public_slug: String,
    pub public_link: String,
    pub diagnosis_summary: String,
    pub bill_amount_kobo: i64,
    pub amount_raised_kobo: i64,
    pub status: String,
    pub admitted_at: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalCaseBillingItemResponse {
    pub id: Uuid,
    pub medical_case_id: Uuid,
    pub description: String,
    pub amount_kobo: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalCaseDocumentResponse {
    pub id: Uuid,
    pub medical_case_id: Uuid,
    pub hospital_id: Uuid,
    pub document_type: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
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
    let email = request.email.trim().to_lowercase();

    if state
        .hospital_repository
        .find_hospital_by_email(&email)
        .await?
        .is_some()
    {
        return Err(HospitalRepositoryError::DuplicateEmail.into());
    }

    let otp = generate_otp();
    send_email_verification_otp(&state, &email, &request.administrator_name, &otp).await?;

    let password_hash = state
        .password_hasher
        .hash_password(&request.password)
        .map_err(|_| ApiError::Internal("failed to hash password".to_owned()))?;

    let hospital = state
        .hospital_repository
        .create_hospital(NewHospital {
            name: request.name.trim().to_owned(),
            email: email.clone(),
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
        normalized_document_mime_type(&request.cac_document.mime_type)?,
        &cac_document,
    )
    .await?;

    let medical_license_document = store_registration_document(
        &state,
        hospital.id,
        HospitalDocumentType::MedicalLicense,
        &request.medical_license_document,
        normalized_document_mime_type(&request.medical_license_document.mime_type)?,
        &medical_license_document,
    )
    .await?;

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    state
        .hospital_repository
        .create_email_otp(NewHospitalEmailOtp {
            hospital_id: hospital.id,
            email: email.clone(),
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    let _ = (cac_document, medical_license_document);

    Ok(Json(RegisterHospitalResponse {
        hospital_id: hospital.id,
        email,
        email_verification_required: true,
        otp_expires_in_seconds: OTP_EXPIRES_IN_SECONDS,
        message: "Registration received. Please verify your email with the OTP sent to your inbox."
            .to_owned(),
    }))
}

async fn send_email_verification_otp(
    state: &AppState,
    email: &str,
    administrator_name: &str,
    otp: &str,
) -> Result<(), ApiError> {
    let subject = "Verify your Korede Health email".to_owned();
    let text_body = format!(
        "Hello {},\n\nYour Korede Health verification code is {}.\n\nThis code expires in 5 minutes.\n\nIf you did not start this registration, you can ignore this email.\n\nThank you,\nKorede Health",
        administrator_name.trim(),
        otp
    );
    let html_body = format!(
        "<p>Hello {},</p><p>Your Korede Health verification code is <strong>{}</strong>.</p><p>This code expires in 5 minutes.</p><p>If you did not start this registration, you can ignore this email.</p><p>Thank you,<br>Korede Health</p>",
        administrator_name.trim(),
        otp
    );

    state
        .email_service
        .send(EmailMessage {
            to_email: email.to_owned(),
            to_name: Some(administrator_name.trim().to_owned()),
            subject,
            text_body,
            html_body: Some(html_body),
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to send hospital email verification OTP");
            ApiError::Internal("failed to send email verification OTP".to_owned())
        })
}

async fn send_login_otp(
    state: &AppState,
    email: &str,
    administrator_name: &str,
    otp: &str,
) -> Result<(), ApiError> {
    let subject = "Your Korede Health login code".to_owned();
    let text_body = format!(
        "Hello {},\n\nYour Korede Health login code is {}.\n\nThis code expires in 5 minutes.\n\nIf you did not try to log in, please secure your account immediately.\n\nThank you,\nKorede Health",
        administrator_name.trim(),
        otp
    );
    let html_body = format!(
        "<p>Hello {},</p><p>Your Korede Health login code is <strong>{}</strong>.</p><p>This code expires in 5 minutes.</p><p>If you did not try to log in, please secure your account immediately.</p><p>Thank you,<br>Korede Health</p>",
        administrator_name.trim(),
        otp
    );

    state
        .email_service
        .send(EmailMessage {
            to_email: email.to_owned(),
            to_name: Some(administrator_name.trim().to_owned()),
            subject,
            text_body,
            html_body: Some(html_body),
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to send hospital login OTP");
            ApiError::Internal("failed to send login OTP".to_owned())
        })
}

async fn send_registration_acknowledgement(
    state: &AppState,
    hospital: &Hospital,
) -> Result<(), ApiError> {
    let subject = "Korede Health verification request received".to_owned();
    let text_body = format!(
        "Hello {},\n\nYour hospital registration and verification documents have been received.\n\nOur team will review your CAC certificate, medical license, and hospital details. Once your credentials are verified, you will be notified by email.\n\nThank you,\nKorede Health",
        hospital.name
    );
    let html_body = format!(
        "<p>Hello {},</p><p>Your hospital registration and verification documents have been received.</p><p>Our team will review your CAC certificate, medical license, and hospital details. Once your credentials are verified, you will be notified by email.</p><p>Thank you,<br>Korede Health</p>",
        hospital.name
    );

    state
        .email_service
        .send(EmailMessage {
            to_email: hospital.email.clone(),
            to_name: hospital.administrator_name.clone(),
            subject,
            text_body,
            html_body: Some(html_body),
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, hospital_id = %hospital.id, "failed to send hospital registration acknowledgement email");
            ApiError::Internal("failed to send registration acknowledgement email".to_owned())
        })
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/verify-email",
    tag = "Hospitals",
    request_body = VerifyHospitalEmailRequest,
    responses(
        (status = 200, description = "Hospital email verified successfully.", body = VerifyHospitalEmailResponse),
        (status = 400, description = "Invalid or expired OTP."),
        (status = 404, description = "Hospital was not found.")
    )
)]
pub async fn verify_hospital_email(
    State(state): State<AppState>,
    Json(request): Json<VerifyHospitalEmailRequest>,
) -> Result<Json<VerifyHospitalEmailResponse>, ApiError> {
    validate_email_otp_request(&request.email, &request.otp)?;

    let email = request.email.trim().to_lowercase();
    let otp = state
        .hospital_repository
        .find_latest_email_otp(&email)
        .await?
        .ok_or_else(|| ApiError::BadRequest("invalid or expired OTP".to_owned()))?;

    if otp.used_at.is_some() {
        return Err(ApiError::BadRequest("OTP has already been used".to_owned()));
    }

    if otp.expires_at <= Utc::now() {
        return Err(ApiError::BadRequest("OTP has expired".to_owned()));
    }

    if otp.attempt_count >= OTP_MAX_ATTEMPTS {
        return Err(ApiError::BadRequest(
            "maximum OTP verification attempts exceeded".to_owned(),
        ));
    }

    let otp_matches = state
        .password_hasher
        .verify_password(request.otp.trim(), &otp.otp_hash)
        .map_err(|_| ApiError::BadRequest("invalid OTP".to_owned()))?;

    if !otp_matches {
        state
            .hospital_repository
            .increment_email_otp_attempts(otp.id)
            .await?;

        return Err(ApiError::BadRequest("invalid OTP".to_owned()));
    }

    state
        .hospital_repository
        .mark_email_otp_used(otp.id)
        .await?;
    let hospital = state
        .hospital_repository
        .mark_hospital_email_verified(otp.hospital_id)
        .await?;

    send_registration_acknowledgement(&state, &hospital).await?;

    Ok(Json(VerifyHospitalEmailResponse {
        hospital_id: hospital.id,
        email: hospital.email,
        email_verified: hospital.email_verified,
        verification_status: hospital.verification_status,
        message: "Email verified successfully. Your credentials are now pending admin review."
            .to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/resend-otp",
    tag = "Hospitals",
    request_body = ResendHospitalEmailOtpRequest,
    responses(
        (status = 200, description = "A new OTP was sent.", body = ResendHospitalEmailOtpResponse),
        (status = 400, description = "Email is already verified or resend cooldown is active."),
        (status = 404, description = "Hospital was not found.")
    )
)]
pub async fn resend_hospital_email_otp(
    State(state): State<AppState>,
    Json(request): Json<ResendHospitalEmailOtpRequest>,
) -> Result<Json<ResendHospitalEmailOtpResponse>, ApiError> {
    if request.email.trim().is_empty() || !request.email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    let email = request.email.trim().to_lowercase();
    let hospital = state
        .hospital_repository
        .find_hospital_by_email(&email)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    if hospital.email_verified {
        return Err(ApiError::BadRequest("email is already verified".to_owned()));
    }

    if let Some(created_at) = state
        .hospital_repository
        .latest_email_otp_created_at(hospital.id)
        .await?
    {
        let next_allowed_at = created_at + Duration::seconds(OTP_RESEND_COOLDOWN_SECONDS);
        if next_allowed_at > Utc::now() {
            return Err(ApiError::BadRequest(
                "please wait before requesting another OTP".to_owned(),
            ));
        }
    }

    let otp = generate_otp();
    send_email_verification_otp(
        &state,
        &hospital.email,
        hospital
            .administrator_name
            .as_deref()
            .unwrap_or(&hospital.name),
        &otp,
    )
    .await?;

    state
        .hospital_repository
        .invalidate_active_email_otps(hospital.id)
        .await?;

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    state
        .hospital_repository
        .create_email_otp(NewHospitalEmailOtp {
            hospital_id: hospital.id,
            email: hospital.email.clone(),
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    Ok(Json(ResendHospitalEmailOtpResponse {
        email: hospital.email,
        otp_expires_in_seconds: OTP_EXPIRES_IN_SECONDS,
        message: "A new OTP has been sent to your email.".to_owned(),
    }))
}

pub async fn login_hospital(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginHospitalRequest>,
) -> Result<Json<LoginHospitalResponse>, ApiError> {
    let audit_context = AuditContext::from_headers(&headers);
    let email = request.email.trim().to_lowercase();
    let hospital = state
        .hospital_repository
        .find_hospital_by_email(&email)
        .await?;

    let Some(hospital) = hospital else {
        audit_hospital_event(
            &state,
            None,
            Some(email),
            "login_failure",
            false,
            Some("invalid_credentials"),
            &audit_context,
            serde_json::json!({ "stage": "password" }),
        )
        .await;
        return Err(invalid_credentials());
    };

    let password_matches = state
        .password_hasher
        .verify_password(&request.password, &hospital.password_hash)
        .unwrap_or(false);

    if !password_matches {
        audit_hospital_event(
            &state,
            Some(hospital.id),
            Some(hospital.email.clone()),
            "login_failure",
            false,
            Some("invalid_credentials"),
            &audit_context,
            serde_json::json!({ "stage": "password" }),
        )
        .await;
        return Err(invalid_credentials());
    }

    if !hospital.email_verified {
        audit_hospital_event(
            &state,
            Some(hospital.id),
            Some(hospital.email.clone()),
            "login_failure",
            false,
            Some("email_verification_required"),
            &audit_context,
            serde_json::json!({ "stage": "password" }),
        )
        .await;
        return Err(ApiError::Forbidden(
            "email verification required".to_owned(),
        ));
    }

    match hospital.verification_status {
        HospitalVerificationStatus::Rejected => {
            audit_hospital_event(
                &state,
                Some(hospital.id),
                Some(hospital.email.clone()),
                "login_failure",
                false,
                Some("hospital_verification_rejected"),
                &audit_context,
                serde_json::json!({ "stage": "password" }),
            )
            .await;
            return Err(ApiError::Forbidden(
                "hospital verification was rejected".to_owned(),
            ));
        }
        HospitalVerificationStatus::Suspended => {
            audit_hospital_event(
                &state,
                Some(hospital.id),
                Some(hospital.email.clone()),
                "login_failure",
                false,
                Some("hospital_account_suspended"),
                &audit_context,
                serde_json::json!({ "stage": "password" }),
            )
            .await;
            return Err(ApiError::Forbidden(
                "hospital account is suspended".to_owned(),
            ));
        }
        HospitalVerificationStatus::Pending | HospitalVerificationStatus::Verified => {}
    }

    state
        .hospital_repository
        .invalidate_active_login_otps(hospital.id)
        .await?;

    let otp = generate_otp();
    send_login_otp(
        &state,
        &hospital.email,
        hospital
            .administrator_name
            .as_deref()
            .unwrap_or(&hospital.name),
        &otp,
    )
    .await?;

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    let login_otp = state
        .hospital_repository
        .create_login_otp(NewHospitalLoginOtp {
            hospital_id: hospital.id,
            email: hospital.email.clone(),
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    audit_hospital_event(
        &state,
        Some(hospital.id),
        Some(hospital.email.clone()),
        "otp_sent",
        true,
        None,
        &audit_context,
        serde_json::json!({ "purpose": "login", "challenge_id": login_otp.id }),
    )
    .await;

    Ok(Json(LoginHospitalResponse {
        login_challenge_id: login_otp.id,
        email: hospital.email,
        otp_required: true,
        otp_expires_in_seconds: OTP_EXPIRES_IN_SECONDS,
        message: "Password accepted. Please verify the OTP sent to your email.".to_owned(),
    }))
}

pub async fn verify_login_otp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifyLoginOtpRequest>,
) -> Result<Json<VerifyLoginOtpResponse>, ApiError> {
    validate_login_otp_request(&request.otp)?;

    let audit_context = AuditContext::from_headers(&headers);
    let login_otp = state
        .hospital_repository
        .find_login_otp_by_id(request.login_challenge_id)
        .await?
        .ok_or_else(|| ApiError::BadRequest("invalid or expired OTP".to_owned()))?;

    let hospital = state
        .hospital_repository
        .find_hospital_by_id(login_otp.hospital_id)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    if login_otp.used_at.is_some() {
        audit_hospital_event(
            &state,
            Some(login_otp.hospital_id),
            Some(login_otp.email.clone()),
            "otp_failed",
            false,
            Some("otp_already_used"),
            &audit_context,
            serde_json::json!({ "purpose": "login", "challenge_id": login_otp.id }),
        )
        .await;
        return Err(ApiError::BadRequest("OTP has already been used".to_owned()));
    }

    if login_otp.expires_at <= Utc::now() {
        audit_hospital_event(
            &state,
            Some(login_otp.hospital_id),
            Some(login_otp.email.clone()),
            "otp_failed",
            false,
            Some("otp_expired"),
            &audit_context,
            serde_json::json!({ "purpose": "login", "challenge_id": login_otp.id }),
        )
        .await;
        return Err(ApiError::BadRequest("OTP has expired".to_owned()));
    }

    if login_otp.attempt_count >= OTP_MAX_ATTEMPTS {
        audit_hospital_event(
            &state,
            Some(login_otp.hospital_id),
            Some(login_otp.email.clone()),
            "otp_failed",
            false,
            Some("maximum_attempts_exceeded"),
            &audit_context,
            serde_json::json!({ "purpose": "login", "challenge_id": login_otp.id }),
        )
        .await;
        return Err(ApiError::BadRequest(
            "maximum OTP verification attempts exceeded".to_owned(),
        ));
    }

    let otp_matches = state
        .password_hasher
        .verify_password(request.otp.trim(), &login_otp.otp_hash)
        .unwrap_or(false);

    if !otp_matches {
        state
            .hospital_repository
            .increment_login_otp_attempts(login_otp.id)
            .await?;

        audit_hospital_event(
            &state,
            Some(login_otp.hospital_id),
            Some(login_otp.email.clone()),
            "otp_failed",
            false,
            Some("invalid_otp"),
            &audit_context,
            serde_json::json!({ "purpose": "login", "challenge_id": login_otp.id }),
        )
        .await;
        return Err(ApiError::BadRequest("invalid OTP".to_owned()));
    }

    state
        .hospital_repository
        .mark_login_otp_used(login_otp.id)
        .await?;

    let access_token = state
        .token_service
        .create_access_token(hospital.id, &hospital.email)
        .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;
    let refresh_token =
        issue_refresh_token(&state, hospital.id.to_string(), &hospital.email, "hospital").await?;

    audit_hospital_event(
        &state,
        Some(hospital.id),
        Some(hospital.email.clone()),
        "login_success",
        true,
        None,
        &audit_context,
        serde_json::json!({ "challenge_id": login_otp.id }),
    )
    .await;

    Ok(Json(VerifyLoginOtpResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_owned(),
        expires_in: state.jwt_expires_in_seconds,
        refresh_expires_in: state.refresh_token_expires_in_seconds,
        dashboard_access: dashboard_access_for(&hospital).to_owned(),
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

#[utoipa::path(
    get,
    path = "/api/v1/hospitals/patients/{username}",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    params(
        ("username" = String, Path, description = "Patient username")
    ),
    responses(
        (status = 200, description = "Patient lookup result for hospital case creation.", body = HospitalPatientLookupResponse),
        (status = 401, description = "Missing or invalid hospital bearer token."),
        (status = 404, description = "Patient was not found.")
    )
)]
pub async fn find_patient(
    _authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<HospitalPatientLookupResponse>, ApiError> {
    let patient = state
        .patient_repository
        .find_patient_by_username(&username)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient not found".to_owned()))?;

    let declaration = state
        .patient_declaration_repository
        .find_patient_declaration(patient.id)
        .await?;

    Ok(Json(HospitalPatientLookupResponse::from((
        patient,
        declaration,
    ))))
}

#[utoipa::path(
    post,
    path = "/api/v1/hospitals/cases",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    request_body = CreateHospitalCaseRequest,
    responses(
        (status = 200, description = "Case created and published successfully.", body = CreateHospitalCaseResponse),
        (status = 400, description = "Invalid case creation request."),
        (status = 401, description = "Missing or invalid hospital bearer token."),
        (status = 404, description = "Patient was not found."),
        (status = 413, description = "Uploaded document is too large."),
        (status = 415, description = "Unsupported document type.")
    )
)]
pub async fn create_case(
    authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
    Json(request): Json<CreateHospitalCaseRequest>,
) -> Result<Json<CreateHospitalCaseResponse>, ApiError> {
    validate_create_case_request(&request)?;

    let patient = state
        .patient_repository
        .find_patient_by_username(&request.patient_username)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient not found".to_owned()))?;

    let declaration = state
        .patient_declaration_repository
        .find_patient_declaration(patient.id)
        .await?
        .ok_or_else(|| {
            ApiError::BadRequest("patient declaration is required before case creation".to_owned())
        })?;

    let case_id = Uuid::new_v4();
    let public_slug =
        generate_case_public_slug(&request.patient_username, &request.title, case_id);
    let mut stored_documents = Vec::with_capacity(request.documents.len());

    for document in &request.documents {
        let contents = decode_base64_document(
            &Base64DocumentRequest {
                original_filename: document.original_filename.clone(),
                mime_type: document.mime_type.clone(),
                content_base64: document.content_base64.clone(),
            },
            state.max_upload_bytes,
        )?;
        let mime_type = normalized_document_mime_type(&document.mime_type)?;
        let stored = state
            .document_storage
            .save_case_document(
                authenticated.hospital_id,
                case_id,
                document.document_type.trim(),
                document.original_filename.trim(),
                mime_type,
                &contents,
            )
            .await
            .map_err(|_| ApiError::Internal("failed to store document".to_owned()))?;

        stored_documents.push(NewMedicalCaseDocument {
            document_type: document.document_type.trim().to_owned(),
            storage_provider: stored.storage_provider,
            storage_key: stored.storage_key,
            original_filename: stored.original_filename,
            mime_type: stored.mime_type,
            file_size_bytes: stored.file_size_bytes,
        });
    }

    let billing_items = request
        .billing_items
        .iter()
        .map(|item| NewMedicalCaseBillingItem {
            description: item.description.trim().to_owned(),
            amount_kobo: item.amount_kobo,
        })
        .collect::<Vec<_>>();
    let bill_amount_kobo = billing_items.iter().try_fold(0_i64, |total, item| {
        total
            .checked_add(item.amount_kobo)
            .ok_or_else(|| ApiError::BadRequest("billing total is too large".to_owned()))
    })?;

    let created = state
        .medical_case_repository
        .create_published_case(
            NewMedicalCase {
                id: case_id,
                hospital_id: authenticated.hospital_id,
                patient_id: patient.id,
                title: request.title.trim().to_owned(),
                public_slug,
                diagnosis_summary: request.diagnosis_summary.trim().to_owned(),
                bill_amount_kobo,
                admitted_at: request.admitted_at,
            },
            billing_items,
            stored_documents,
        )
        .await?;

    Ok(Json(CreateHospitalCaseResponse {
        case: HospitalCaseResponse::from(created.case),
        patient: HospitalPatientLookupPatientResponse::from(patient),
        patient_declaration: HospitalPatientDeclarationResponse::from(declaration),
        billing_items: created
            .billing_items
            .into_iter()
            .map(HospitalCaseBillingItemResponse::from)
            .collect(),
        documents: created
            .documents
            .into_iter()
            .map(HospitalCaseDocumentResponse::from)
            .collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/hospitals/patients/{username}/declaration",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    params(
        ("username" = String, Path, description = "Patient username")
    ),
    responses(
        (status = 200, description = "Patient declaration by username.", body = HospitalPatientDeclarationResponse),
        (status = 401, description = "Missing or invalid hospital bearer token."),
        (status = 404, description = "Patient declaration was not found.")
    )
)]
pub async fn get_patient_declaration(
    _authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<HospitalPatientDeclarationResponse>, ApiError> {
    let declaration = state
        .patient_declaration_repository
        .find_patient_declaration_by_username(&username)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    Ok(Json(HospitalPatientDeclarationResponse::from(declaration)))
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

fn validate_create_case_request(request: &CreateHospitalCaseRequest) -> Result<(), ApiError> {
    if request.patient_username.trim().is_empty()
        || request.title.trim().is_empty()
        || request.diagnosis_summary.trim().is_empty()
    {
        return Err(ApiError::BadRequest(
            "required fields are missing".to_owned(),
        ));
    }

    if request.billing_items.is_empty() {
        return Err(ApiError::BadRequest(
            "at least one billing item is required".to_owned(),
        ));
    }

    for item in &request.billing_items {
        if item.description.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "billing item description is required".to_owned(),
            ));
        }

        if item.amount_kobo <= 0 {
            return Err(ApiError::BadRequest(
                "billing item amount must be greater than zero".to_owned(),
            ));
        }
    }

    for document in &request.documents {
        if document.document_type.trim().is_empty()
            || document.original_filename.trim().is_empty()
            || document.mime_type.trim().is_empty()
            || document.content_base64.trim().is_empty()
        {
            return Err(ApiError::BadRequest(
                "document fields are required".to_owned(),
            ));
        }

        validate_mime_type(document.mime_type.trim())?;
    }

    Ok(())
}

fn validate_email_otp_request(email: &str, otp: &str) -> Result<(), ApiError> {
    if email.trim().is_empty() || !email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    validate_login_otp_request(otp)
}

fn validate_login_otp_request(otp: &str) -> Result<(), ApiError> {
    let otp = otp.trim();
    if otp.len() != OTP_LENGTH || !otp.chars().all(|character| character.is_ascii_digit()) {
        return Err(ApiError::BadRequest(
            "OTP must be a 6-digit code".to_owned(),
        ));
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct AuditContext {
    ip_address: Option<String>,
    user_agent: Option<String>,
}

impl AuditContext {
    fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            ip_address: header_value(headers, "x-forwarded-for")
                .and_then(|value| value.split(',').next().map(str::trim).map(str::to_owned))
                .filter(|value| !value.is_empty())
                .or_else(|| header_value(headers, "x-real-ip")),
            user_agent: header_value(headers, "user-agent"),
        }
    }
}

fn header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

async fn audit_hospital_event(
    state: &AppState,
    hospital_id: Option<Uuid>,
    email: Option<String>,
    event_type: &str,
    success: bool,
    reason: Option<&str>,
    context: &AuditContext,
    metadata: serde_json::Value,
) {
    if let Err(error) = state
        .hospital_repository
        .save_audit_log(NewHospitalAuditLog {
            hospital_id,
            email,
            event_type: event_type.to_owned(),
            success,
            reason: reason.map(str::to_owned),
            ip_address: context.ip_address.clone(),
            user_agent: context.user_agent.clone(),
            metadata,
        })
        .await
    {
        tracing::error!(%error, %event_type, "failed to save hospital audit log");
    }
}

fn generate_otp() -> String {
    let value = Uuid::new_v4().as_u128() % 1_000_000;
    format!("{value:06}")
}

fn generate_case_public_slug(patient_username: &str, title: &str, case_id: Uuid) -> String {
    let prefix_source = format!("{} {}", patient_username.trim(), title.trim());
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in prefix_source.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if (character.is_whitespace() || character == '-' || character == '_')
            && !slug.is_empty()
            && !last_was_separator
        {
            slug.push('-');
            last_was_separator = true;
        }

        if slug.len() >= 80 {
            break;
        }
    }

    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "case" } else { slug };
    let unique_suffix = case_id.simple().to_string();

    format!("{}-{}", slug, &unique_suffix[..8])
}

fn public_case_link(public_slug: &str) -> String {
    format!("/cases/{public_slug}")
}

fn dashboard_access_for(hospital: &Hospital) -> &'static str {
    match hospital.verification_status {
        HospitalVerificationStatus::Verified => "full",
        _ => "pending_review",
    }
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
    mime_type: &str,
    contents: &[u8],
) -> Result<HospitalDocument, ApiError> {
    let stored = state
        .document_storage
        .save_document(
            hospital_id,
            document_type.clone(),
            request.original_filename.trim(),
            mime_type,
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
    normalized_document_mime_type(mime_type).map(|_| ())
}

fn normalized_document_mime_type(mime_type: &str) -> Result<&'static str, ApiError> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "application/pdf" | "application/x-pdf" | "pdf" => Ok("application/pdf"),
        "image/jpeg" | "image/jpg" | "jpeg" | "jpg" => Ok("image/jpeg"),
        "image/png" | "png" => Ok("image/png"),
        "image/webp" | "webp" => Ok("image/webp"),
        _ => Err(ApiError::UnsupportedMediaType(
            "only PDF, JPEG, PNG, and WebP files are supported".to_owned(),
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

impl From<&Hospital> for HospitalSummaryResponse {
    fn from(hospital: &Hospital) -> Self {
        Self {
            id: hospital.id,
            name: hospital.name.clone(),
            email: hospital.email.clone(),
            email_verified: hospital.email_verified,
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

impl From<PatientDeclaration> for HospitalPatientDeclarationResponse {
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

impl From<Patient> for HospitalPatientLookupPatientResponse {
    fn from(patient: Patient) -> Self {
        Self {
            id: patient.id,
            username: patient.username,
            first_name: patient.first_name,
            last_name: patient.last_name,
            email_verified: patient.email_verified,
        }
    }
}

impl From<(Patient, Option<PatientDeclaration>)> for HospitalPatientLookupResponse {
    fn from((patient, declaration): (Patient, Option<PatientDeclaration>)) -> Self {
        let declaration = declaration.map(HospitalPatientLookupDeclarationResponse::from);
        let can_create_case = declaration.is_some();

        Self {
            patient: HospitalPatientLookupPatientResponse::from(patient),
            declaration: declaration.unwrap_or(HospitalPatientLookupDeclarationResponse {
                exists: false,
                statement: None,
                created_at: None,
            }),
            can_create_case,
        }
    }
}

impl From<PatientDeclaration> for HospitalPatientLookupDeclarationResponse {
    fn from(declaration: PatientDeclaration) -> Self {
        Self {
            exists: true,
            statement: Some(declaration.statement),
            created_at: Some(declaration.created_at),
        }
    }
}

impl From<MedicalCase> for HospitalCaseResponse {
    fn from(medical_case: MedicalCase) -> Self {
        Self {
            id: medical_case.id,
            hospital_id: medical_case.hospital_id,
            patient_id: medical_case.patient_id,
            title: medical_case.title,
            public_slug: medical_case.public_slug.clone().unwrap_or_default(),
            public_link: public_case_link(medical_case.public_slug.as_deref().unwrap_or_default()),
            diagnosis_summary: medical_case.diagnosis_summary,
            bill_amount_kobo: medical_case.bill_amount_kobo,
            amount_raised_kobo: medical_case.amount_raised_kobo,
            status: medical_case.status.as_str().to_owned(),
            admitted_at: medical_case.admitted_at,
            created_at: medical_case.created_at,
            updated_at: medical_case.updated_at,
        }
    }
}

impl From<MedicalCaseBillingItem> for HospitalCaseBillingItemResponse {
    fn from(item: MedicalCaseBillingItem) -> Self {
        Self {
            id: item.id,
            medical_case_id: item.medical_case_id,
            description: item.description,
            amount_kobo: item.amount_kobo,
            created_at: item.created_at,
        }
    }
}

impl From<MedicalCaseDocument> for HospitalCaseDocumentResponse {
    fn from(document: MedicalCaseDocument) -> Self {
        Self {
            id: document.id,
            medical_case_id: document.medical_case_id,
            hospital_id: document.hospital_id,
            document_type: document.document_type,
            original_filename: document.original_filename,
            mime_type: document.mime_type,
            file_size_bytes: document.file_size_bytes,
            uploaded_at: document.uploaded_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hospital_with_status(status: HospitalVerificationStatus) -> Hospital {
        Hospital {
            id: Uuid::new_v4(),
            name: "Lagoon Hospital".to_owned(),
            email: "admin@lagoon.example".to_owned(),
            email_verified: true,
            email_verified_at: Some(Utc::now()),
            password_hash: "hash".to_owned(),
            phone_number: Some("+2348012345678".to_owned()),
            official_address: Some("1 Hospital Road, Lagos".to_owned()),
            administrator_name: Some("Dr Jane Doe".to_owned()),
            cac_registration_number: Some("RC123456".to_owned()),
            medical_license_number: Some("ML123456".to_owned()),
            corporate_account_name: "Lagoon Hospital Ltd".to_owned(),
            corporate_account_number: "0123456789".to_owned(),
            bank_name: "Wema Bank".to_owned(),
            verification_status: status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

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

    fn valid_case_request() -> CreateHospitalCaseRequest {
        CreateHospitalCaseRequest {
            patient_username: "oluwaseun34".to_owned(),
            title: "Right Femur Fracture Surgery".to_owned(),
            diagnosis_summary: "Patient requires urgent ORIF surgery.".to_owned(),
            admitted_at: None,
            billing_items: vec![CreateHospitalCaseBillingItemRequest {
                description: "Surgery".to_owned(),
                amount_kobo: 150_000_000,
            }],
            documents: vec![],
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
    fn mime_validation_accepts_supported_document_types() {
        assert!(validate_mime_type("application/pdf").is_ok());
        assert!(validate_mime_type("Pdf").is_ok());
        assert!(validate_mime_type("image/jpeg").is_ok());
        assert!(validate_mime_type("JPG").is_ok());
        assert!(validate_mime_type("image/png").is_ok());
        assert!(validate_mime_type("webp").is_ok());
        assert_eq!(
            normalized_document_mime_type("Pdf").unwrap(),
            "application/pdf"
        );
        assert_eq!(normalized_document_mime_type("JPG").unwrap(), "image/jpeg");
    }

    #[test]
    fn mime_validation_rejects_unsupported_types() {
        assert!(matches!(
            validate_mime_type("text/plain"),
            Err(ApiError::UnsupportedMediaType(_))
        ));
    }

    #[test]
    fn case_creation_validation_accepts_valid_request() {
        assert!(validate_create_case_request(&valid_case_request()).is_ok());
    }

    #[test]
    fn case_creation_validation_rejects_empty_billing_items() {
        let mut request = valid_case_request();
        request.billing_items.clear();

        assert!(matches!(
            validate_create_case_request(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn case_creation_validation_rejects_non_positive_billing_amount() {
        let mut request = valid_case_request();
        request.billing_items[0].amount_kobo = 0;

        assert!(matches!(
            validate_create_case_request(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn public_slug_generation_uses_patient_title_and_uuid_suffix() {
        let case_id = Uuid::parse_str("12345678-90ab-cdef-1234-567890abcdef").unwrap();
        let slug = generate_case_public_slug(
            "Oluwaseun34",
            "Right Femur Fracture Surgery",
            case_id,
        );

        assert_eq!(slug, "oluwaseun34-right-femur-fracture-surgery-12345678");
    }

    #[test]
    fn public_slug_generation_falls_back_when_text_has_no_slug_content() {
        let case_id = Uuid::parse_str("abcdef12-3456-7890-abcd-ef1234567890").unwrap();
        let slug = generate_case_public_slug("!!!", "___", case_id);

        assert_eq!(slug, "case-abcdef12");
    }

    #[test]
    fn public_case_link_uses_cases_path() {
        assert_eq!(
            public_case_link("oluwaseun34-case-12345678"),
            "/cases/oluwaseun34-case-12345678"
        );
    }

    #[test]
    fn otp_generation_returns_six_digits() {
        let otp = generate_otp();

        assert_eq!(otp.len(), 6);
        assert!(otp.chars().all(|character| character.is_ascii_digit()));
    }

    #[test]
    fn email_otp_validation_accepts_valid_input() {
        assert!(validate_email_otp_request("admin@hospital.com", "123456").is_ok());
    }

    #[test]
    fn email_otp_validation_rejects_invalid_otp_shape() {
        assert!(matches!(
            validate_email_otp_request("admin@hospital.com", "12345"),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            validate_email_otp_request("admin@hospital.com", "abcdef"),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn dashboard_access_is_pending_review_for_pending_hospital() {
        let hospital = hospital_with_status(HospitalVerificationStatus::Pending);

        assert_eq!(dashboard_access_for(&hospital), "pending_review");
    }

    #[test]
    fn dashboard_access_is_full_for_verified_hospital() {
        let hospital = hospital_with_status(HospitalVerificationStatus::Verified);

        assert_eq!(dashboard_access_for(&hospital), "full");
    }
}
