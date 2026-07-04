use axum::{extract::State, routing::post, Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{error::ApiError, AppState},
    domain::{patient::Patient, patient_declaration::PatientDeclaration},
    port::{
        auth::AuthenticatedPatient,
        email::EmailMessage,
        patient::{NewPatient, NewPatientEmailOtp, PatientRepositoryError},
        patient_declaration::UpsertPatientDeclaration,
    },
};

const OTP_LENGTH: usize = 6;
const OTP_EXPIRES_IN_SECONDS: i64 = 300;
const OTP_MAX_ATTEMPTS: i32 = 5;
const OTP_RESEND_COOLDOWN_SECONDS: i64 = 60;
const DECLARATION_MIN_CHARACTERS: usize = 20;
const DECLARATION_MAX_CHARACTERS: usize = 5000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register_patient))
        .route("/verify-email", post(verify_patient_email))
        .route("/resend-otp", post(resend_patient_email_otp))
        .route(
            "/declaration",
            post(upsert_declaration)
                .put(upsert_declaration)
                .get(get_declaration),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterPatientRequest {
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub password: String,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub phone_number: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterPatientResponse {
    pub patient: PatientResponse,
    pub email_verification_required: bool,
    pub otp_expires_in_seconds: i64,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyPatientEmailRequest {
    pub email: String,
    pub otp: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyPatientEmailResponse {
    pub patient_id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResendPatientEmailOtpRequest {
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResendPatientEmailOtpResponse {
    pub email: String,
    pub otp_expires_in_seconds: i64,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PatientResponse {
    pub id: Uuid,
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub full_name: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub phone_number: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertPatientDeclarationRequest {
    pub statement: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PatientDeclarationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub statement: String,
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
    let email = normalize_email(&request.email)?;

    if state
        .patient_repository
        .find_patient_by_username(&username)
        .await?
        .is_some()
    {
        return Err(PatientRepositoryError::DuplicateUsername.into());
    }

    if state
        .patient_repository
        .find_patient_by_email(&email)
        .await?
        .is_some()
    {
        return Err(PatientRepositoryError::DuplicateEmail.into());
    }

    let otp = generate_otp();
    send_patient_email_verification_otp(&state, &email, &request.first_name, &otp).await?;

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
            email: Some(email.clone()),
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

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    state
        .patient_repository
        .create_email_otp(NewPatientEmailOtp {
            patient_id: patient.id,
            email: email.clone(),
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    Ok(Json(RegisterPatientResponse {
        patient: PatientResponse::from(patient),
        email_verification_required: true,
        otp_expires_in_seconds: OTP_EXPIRES_IN_SECONDS,
        message: "Patient registered. Please verify your email with the OTP sent to your inbox."
            .to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/patients/verify-email",
    tag = "Patients",
    request_body = VerifyPatientEmailRequest,
    responses(
        (status = 200, description = "Patient email verified successfully.", body = VerifyPatientEmailResponse),
        (status = 400, description = "Invalid or expired OTP."),
        (status = 404, description = "Patient was not found.")
    )
)]
pub async fn verify_patient_email(
    State(state): State<AppState>,
    Json(request): Json<VerifyPatientEmailRequest>,
) -> Result<Json<VerifyPatientEmailResponse>, ApiError> {
    validate_email_otp_request(&request.email, &request.otp)?;

    let email = normalize_email(&request.email)?;
    let otp = state
        .patient_repository
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
            .patient_repository
            .increment_email_otp_attempts(otp.id)
            .await?;

        return Err(ApiError::BadRequest("invalid OTP".to_owned()));
    }

    state.patient_repository.mark_email_otp_used(otp.id).await?;
    let patient = state
        .patient_repository
        .mark_patient_email_verified(otp.patient_id)
        .await?;

    Ok(Json(VerifyPatientEmailResponse {
        patient_id: patient.id,
        username: patient.username,
        email: patient.email.unwrap_or(email),
        email_verified: patient.email_verified,
        message: "Email verified successfully.".to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/patients/resend-otp",
    tag = "Patients",
    request_body = ResendPatientEmailOtpRequest,
    responses(
        (status = 200, description = "A new OTP was sent.", body = ResendPatientEmailOtpResponse),
        (status = 400, description = "Email is already verified or resend cooldown is active."),
        (status = 404, description = "Patient was not found.")
    )
)]
pub async fn resend_patient_email_otp(
    State(state): State<AppState>,
    Json(request): Json<ResendPatientEmailOtpRequest>,
) -> Result<Json<ResendPatientEmailOtpResponse>, ApiError> {
    let email = normalize_email(&request.email)?;
    let patient = state
        .patient_repository
        .find_patient_by_email(&email)
        .await?
        .ok_or(PatientRepositoryError::NotFound)?;

    if patient.email_verified {
        return Err(ApiError::BadRequest("email is already verified".to_owned()));
    }

    if let Some(created_at) = state
        .patient_repository
        .latest_email_otp_created_at(patient.id)
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
    send_patient_email_verification_otp(&state, &email, patient_display_name(&patient), &otp)
        .await?;

    state
        .patient_repository
        .invalidate_active_email_otps(patient.id)
        .await?;

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    state
        .patient_repository
        .create_email_otp(NewPatientEmailOtp {
            patient_id: patient.id,
            email: email.clone(),
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    Ok(Json(ResendPatientEmailOtpResponse {
        email,
        otp_expires_in_seconds: OTP_EXPIRES_IN_SECONDS,
        message: "A new OTP has been sent to your email.".to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/patients/declaration",
    tag = "Patients",
    security(("bearer_auth" = [])),
    request_body = UpsertPatientDeclarationRequest,
    responses(
        (status = 200, description = "Patient declaration saved.", body = PatientDeclarationResponse),
        (status = 400, description = "Invalid declaration statement."),
        (status = 401, description = "Missing or invalid patient bearer token."),
        (status = 409, description = "Patient declaration is locked by an open medical case.")
    )
)]
pub async fn upsert_declaration(
    authenticated: AuthenticatedPatient,
    State(state): State<AppState>,
    Json(request): Json<UpsertPatientDeclarationRequest>,
) -> Result<Json<PatientDeclarationResponse>, ApiError> {
    let statement = validate_declaration_statement(&request.statement)?;

    if state
        .medical_case_repository
        .patient_has_open_case(authenticated.patient_id)
        .await?
    {
        return Err(ApiError::Conflict(
            "patient declaration is locked because an open medical case already exists".to_owned(),
        ));
    }

    let declaration = state
        .patient_declaration_repository
        .upsert_patient_declaration(UpsertPatientDeclaration {
            patient_id: authenticated.patient_id,
            statement,
        })
        .await?;

    Ok(Json(PatientDeclarationResponse::from(declaration)))
}

#[utoipa::path(
    get,
    path = "/api/v1/patients/declaration",
    tag = "Patients",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Patient declaration.", body = PatientDeclarationResponse),
        (status = 401, description = "Missing or invalid patient bearer token."),
        (status = 404, description = "Patient declaration was not found.")
    )
)]
pub async fn get_declaration(
    authenticated: AuthenticatedPatient,
    State(state): State<AppState>,
) -> Result<Json<PatientDeclarationResponse>, ApiError> {
    let declaration = state
        .patient_declaration_repository
        .find_patient_declaration(authenticated.patient_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    Ok(Json(PatientDeclarationResponse::from(declaration)))
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

    normalize_email(&request.email)?;

    Ok(())
}

fn validate_declaration_statement(statement: &str) -> Result<String, ApiError> {
    let statement = statement.trim();
    let character_count = statement.chars().count();

    if character_count < DECLARATION_MIN_CHARACTERS {
        return Err(ApiError::BadRequest(format!(
            "patient declaration statement is too short: minimum is {} characters, received {} characters",
            DECLARATION_MIN_CHARACTERS, character_count
        )));
    }

    if character_count > DECLARATION_MAX_CHARACTERS {
        return Err(ApiError::BadRequest(format!(
            "patient declaration statement is too long: maximum is {} characters, received {} characters",
            DECLARATION_MAX_CHARACTERS, character_count
        )));
    }

    Ok(statement.to_owned())
}

async fn send_patient_email_verification_otp(
    state: &AppState,
    email: &str,
    patient_name: &str,
    otp: &str,
) -> Result<(), ApiError> {
    let subject = "Verify your Korede Health email".to_owned();
    let text_body = format!(
        "Hello {},\n\nYour Korede Health verification code is {}.\n\nThis code expires in 5 minutes.\n\nIf you did not start this registration, you can ignore this email.\n\nThank you,\nKorede Health",
        patient_name.trim(),
        otp
    );
    let html_body = format!(
        "<p>Hello {},</p><p>Your Korede Health verification code is <strong>{}</strong>.</p><p>This code expires in 5 minutes.</p><p>If you did not start this registration, you can ignore this email.</p><p>Thank you,<br>Korede Health</p>",
        patient_name.trim(),
        otp
    );

    state
        .email_service
        .send(EmailMessage {
            to_email: email.to_owned(),
            to_name: Some(patient_name.trim().to_owned()),
            subject,
            text_body,
            html_body: Some(html_body),
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to send patient email verification OTP");
            ApiError::Internal("failed to send email verification OTP".to_owned())
        })
}

fn validate_email_otp_request(email: &str, otp: &str) -> Result<(), ApiError> {
    normalize_email(email)?;

    let otp = otp.trim();
    if otp.len() != OTP_LENGTH || !otp.chars().all(|character| character.is_ascii_digit()) {
        return Err(ApiError::BadRequest(
            "OTP must be a 6-digit code".to_owned(),
        ));
    }

    Ok(())
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();

    if email.is_empty() || !email.contains('@') || !email.contains('.') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    Ok(email)
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

fn generate_otp() -> String {
    let value = Uuid::new_v4().as_u128() % 1_000_000;
    format!("{value:06}")
}

fn patient_display_name(patient: &Patient) -> &str {
    patient
        .first_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&patient.full_name)
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
            email_verified: patient.email_verified,
            email_verified_at: patient.email_verified_at,
            date_of_birth: patient.date_of_birth,
            gender: patient.gender,
            phone_number: patient.phone_number,
            created_at: patient.created_at,
            updated_at: patient.updated_at,
        }
    }
}

impl From<PatientDeclaration> for PatientDeclarationResponse {
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

    fn valid_registration_request() -> RegisterPatientRequest {
        RegisterPatientRequest {
            username: "patient_one".to_owned(),
            first_name: "Femi".to_owned(),
            last_name: "Jacob".to_owned(),
            email: "femi@example.com".to_owned(),
            password: "strong-password".to_owned(),
            date_of_birth: None,
            gender: Some("male".to_owned()),
            phone_number: Some("09025540752".to_owned()),
        }
    }

    #[test]
    fn registration_validation_accepts_valid_request() {
        assert!(validate_patient_registration(&valid_registration_request()).is_ok());
    }

    #[test]
    fn registration_validation_requires_email() {
        let mut request = valid_registration_request();
        request.email = " ".to_owned();

        assert!(matches!(
            validate_patient_registration(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn registration_validation_rejects_invalid_email() {
        let mut request = valid_registration_request();
        request.email = "not-an-email".to_owned();

        assert!(matches!(
            validate_patient_registration(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn registration_validation_rejects_invalid_username() {
        let mut request = valid_registration_request();
        request.username = "no spaces".to_owned();

        assert!(matches!(
            validate_patient_registration(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn email_otp_validation_accepts_valid_input() {
        assert!(validate_email_otp_request("femi@example.com", "123456").is_ok());
    }

    #[test]
    fn email_otp_validation_rejects_invalid_otp_shape() {
        assert!(matches!(
            validate_email_otp_request("femi@example.com", "12345"),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            validate_email_otp_request("femi@example.com", "abcdef"),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn otp_generation_returns_six_digits() {
        let otp = generate_otp();

        assert_eq!(otp.len(), 6);
        assert!(otp.chars().all(|character| character.is_ascii_digit()));
    }

    #[test]
    fn declaration_validation_trims_and_accepts_valid_statement() {
        let statement = validate_declaration_statement(
            "  I started feeling severe symptoms after the accident.  ",
        )
        .unwrap();

        assert_eq!(
            statement,
            "I started feeling severe symptoms after the accident."
        );
    }

    #[test]
    fn declaration_validation_rejects_short_statement() {
        let error = validate_declaration_statement("I need money").unwrap_err();

        assert!(matches!(&error, ApiError::BadRequest(_)));
        assert_eq!(
            match error {
                ApiError::BadRequest(message) => message,
                _ => unreachable!(),
            },
            "patient declaration statement is too short: minimum is 20 characters, received 12 characters"
        );
    }
}
