use axum::{
    Json, Router, async_trait,
    extract::{FromRequestParts, State},
    http::{HeaderMap, StatusCode, request::Parts},
    routing::post,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{
        AppState,
        hospitals::{self, LoginHospitalRequest, VerifyLoginOtpRequest, VerifyLoginOtpResponse},
        tokens::{hash_refresh_token, issue_refresh_token},
    },
    domain::hospital::HospitalVerificationStatus,
    port::{
        auth::{AuthenticatedAdmin, AuthenticatedHospital, AuthenticatedPatient},
        email::EmailMessage,
        hospital::NewHospitalPasswordResetOtp,
        patient::NewPatientPasswordResetOtp,
    },
};

use super::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/login/verify-otp", post(verify_login_otp))
        .route("/refresh", post(refresh_token))
        .route("/forgot-password", post(request_password_reset))
        .route("/reset-password", post(reset_password))
}

const PASSWORD_RESET_OTP_LENGTH: usize = 6;
const PASSWORD_RESET_OTP_EXPIRES_IN_SECONDS: i64 = 300;
const PASSWORD_RESET_OTP_MAX_ATTEMPTS: i32 = 5;
const PASSWORD_RESET_OTP_RESEND_COOLDOWN_SECONDS: i64 = 60;

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub role: String,
    pub token_type: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub refresh_expires_in: Option<i64>,
    pub otp_required: bool,
    pub login_challenge_id: Option<Uuid>,
    pub email: Option<String>,
    pub otp_expires_in_seconds: Option<i64>,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshTokenResponse {
    pub role: String,
    pub token_type: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub refresh_expires_in: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub role: String,
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ForgotPasswordResponse {
    pub role: String,
    pub email: String,
    pub otp_expires_in_seconds: i64,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub role: String,
    pub email: String,
    pub otp: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetPasswordResponse {
    pub role: String,
    pub email: String,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Admin authenticated or hospital password accepted.", body = LoginResponse),
        (status = 400, description = "Invalid login request."),
        (status = 401, description = "Invalid email or password.")
    )
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    validate_login_request(&request)?;

    let email = request.email.trim().to_lowercase();
    let password = request.password.trim();

    if email == state.super_admin_email {
        if !constant_time_eq(password.as_bytes(), state.super_admin_password.as_bytes()) {
            return Err(invalid_credentials());
        }

        let access_token = state
            .token_service
            .create_admin_access_token(&state.super_admin_email)
            .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;
        let refresh_token = issue_refresh_token(
            &state,
            "super_admin".to_owned(),
            &state.super_admin_email,
            "admin",
        )
        .await?;

        return Ok(Json(LoginResponse {
            role: "admin".to_owned(),
            token_type: Some("Bearer".to_owned()),
            access_token: Some(access_token),
            refresh_token: Some(refresh_token),
            expires_in: Some(state.jwt_expires_in_seconds),
            refresh_expires_in: Some(state.refresh_token_expires_in_seconds),
            otp_required: false,
            login_challenge_id: None,
            email: Some(state.super_admin_email.clone()),
            otp_expires_in_seconds: None,
            message: "Login successful.".to_owned(),
        }));
    }

    if state
        .hospital_repository
        .find_hospital_by_email(&email)
        .await?
        .is_some()
    {
        let hospital_login = hospitals::login_hospital(
            State(state),
            headers,
            Json(LoginHospitalRequest {
                email,
                password: request.password,
            }),
        )
        .await?;

        let hospital_login = hospital_login.0;
        return Ok(Json(LoginResponse {
            role: "hospital".to_owned(),
            token_type: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            refresh_expires_in: None,
            otp_required: hospital_login.otp_required,
            login_challenge_id: Some(hospital_login.login_challenge_id),
            email: Some(hospital_login.email),
            otp_expires_in_seconds: Some(hospital_login.otp_expires_in_seconds),
            message: hospital_login.message,
        }));
    }

    login_patient(&state, &email, &request.password).await
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login/verify-otp",
    tag = "Auth",
    request_body = VerifyLoginOtpRequest,
    responses(
        (status = 200, description = "Hospital logged in successfully.", body = VerifyLoginOtpResponse),
        (status = 400, description = "Invalid or expired OTP.")
    )
)]
pub async fn verify_login_otp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifyLoginOtpRequest>,
) -> Result<Json<VerifyLoginOtpResponse>, ApiError> {
    hospitals::verify_login_otp(State(state), headers, Json(request)).await
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "Auth",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, description = "Access token refreshed and refresh token rotated.", body = RefreshTokenResponse),
        (status = 400, description = "Invalid refresh request."),
        (status = 401, description = "Invalid or expired refresh token.")
    )
)]
pub async fn refresh_token(
    State(state): State<AppState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<Json<RefreshTokenResponse>, ApiError> {
    let raw_refresh_token = request.refresh_token.trim();
    if raw_refresh_token.is_empty() {
        return Err(ApiError::BadRequest("refresh token is required".to_owned()));
    }

    let token_hash = hash_refresh_token(raw_refresh_token);
    let stored_token = state
        .refresh_token_repository
        .find_refresh_token_by_hash(&token_hash)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to load refresh token");
            ApiError::Internal("internal server error".to_owned())
        })?
        .ok_or_else(invalid_refresh_token)?;

    if stored_token.revoked_at.is_some() || stored_token.expires_at <= Utc::now() {
        return Err(invalid_refresh_token());
    }

    let (access_token, subject_id, email, role) = match stored_token.role.as_str() {
        "admin" => {
            if stored_token.email != state.super_admin_email
                || stored_token.subject_id != "super_admin"
            {
                return Err(invalid_refresh_token());
            }

            let access_token = state
                .token_service
                .create_admin_access_token(&state.super_admin_email)
                .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;

            (
                access_token,
                "super_admin".to_owned(),
                state.super_admin_email.clone(),
                "admin".to_owned(),
            )
        }
        "hospital" => {
            let hospital_id =
                Uuid::parse_str(&stored_token.subject_id).map_err(|_| invalid_refresh_token())?;
            let hospital = state
                .hospital_repository
                .find_hospital_by_id(hospital_id)
                .await?
                .ok_or_else(invalid_refresh_token)?;

            if !hospital.email_verified {
                return Err(invalid_refresh_token());
            }

            match hospital.verification_status {
                HospitalVerificationStatus::Rejected | HospitalVerificationStatus::Suspended => {
                    return Err(invalid_refresh_token());
                }
                HospitalVerificationStatus::Pending | HospitalVerificationStatus::Verified => {}
            }

            let access_token = state
                .token_service
                .create_access_token(hospital.id, &hospital.email)
                .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;

            (
                access_token,
                hospital.id.to_string(),
                hospital.email,
                "hospital".to_owned(),
            )
        }
        "patient" => {
            let patient_id =
                Uuid::parse_str(&stored_token.subject_id).map_err(|_| invalid_refresh_token())?;
            let patient = state
                .patient_repository
                .find_patient_by_id(patient_id)
                .await?
                .ok_or_else(invalid_refresh_token)?;

            if !patient.email_verified {
                return Err(invalid_refresh_token());
            }

            let email = patient.email.ok_or_else(invalid_refresh_token)?;
            if email != stored_token.email {
                return Err(invalid_refresh_token());
            }

            let access_token = state
                .token_service
                .create_patient_access_token(patient.id, &email)
                .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;

            (
                access_token,
                patient.id.to_string(),
                email,
                "patient".to_owned(),
            )
        }
        _ => return Err(invalid_refresh_token()),
    };

    let revoked = state
        .refresh_token_repository
        .revoke_refresh_token(stored_token.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to revoke refresh token");
            ApiError::Internal("internal server error".to_owned())
        })?;
    if !revoked {
        return Err(invalid_refresh_token());
    }

    let new_refresh_token = issue_refresh_token(&state, subject_id, &email, &role).await?;

    Ok(Json(RefreshTokenResponse {
        role,
        token_type: "Bearer".to_owned(),
        access_token,
        refresh_token: new_refresh_token,
        expires_in: state.jwt_expires_in_seconds,
        refresh_expires_in: state.refresh_token_expires_in_seconds,
    }))
}

async fn login_patient(
    state: &AppState,
    email: &str,
    password: &str,
) -> Result<Json<LoginResponse>, ApiError> {
    let patient = state
        .patient_repository
        .find_patient_by_email(email)
        .await?
        .ok_or_else(invalid_credentials)?;

    let password_hash = patient
        .password_hash
        .as_deref()
        .ok_or_else(invalid_credentials)?;

    let password_matches = state
        .password_hasher
        .verify_password(password, password_hash)
        .unwrap_or(false);

    if !password_matches {
        return Err(invalid_credentials());
    }

    if !patient.email_verified {
        return Err(ApiError::Forbidden(
            "email must be verified before login".to_owned(),
        ));
    }

    let email = patient.email.ok_or_else(invalid_credentials)?;
    let access_token = state
        .token_service
        .create_patient_access_token(patient.id, &email)
        .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;
    let refresh_token =
        issue_refresh_token(state, patient.id.to_string(), &email, "patient").await?;

    Ok(Json(LoginResponse {
        role: "patient".to_owned(),
        token_type: Some("Bearer".to_owned()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in: Some(state.jwt_expires_in_seconds),
        refresh_expires_in: Some(state.refresh_token_expires_in_seconds),
        otp_required: false,
        login_challenge_id: None,
        email: Some(email),
        otp_expires_in_seconds: None,
        message: "Login successful.".to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/forgot-password",
    tag = "Auth",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Password reset OTP request accepted.", body = ForgotPasswordResponse),
        (status = 400, description = "Invalid role or email.")
    )
)]
pub async fn request_password_reset(
    State(state): State<AppState>,
    Json(request): Json<ForgotPasswordRequest>,
) -> Result<Json<ForgotPasswordResponse>, ApiError> {
    let role = normalize_password_reset_role(&request.role)?;
    let email = normalize_email(&request.email)?;

    match role.as_str() {
        "hospital" => request_hospital_password_reset(&state, &email).await?,
        "patient" => request_patient_password_reset(&state, &email).await?,
        _ => unreachable!(),
    }

    Ok(Json(ForgotPasswordResponse {
        role,
        email,
        otp_expires_in_seconds: PASSWORD_RESET_OTP_EXPIRES_IN_SECONDS,
        message: "If an account exists for this email, a password reset OTP has been sent."
            .to_owned(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/reset-password",
    tag = "Auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset successfully.", body = ResetPasswordResponse),
        (status = 400, description = "Invalid request, OTP, or password.")
    )
)]
pub async fn reset_password(
    State(state): State<AppState>,
    Json(request): Json<ResetPasswordRequest>,
) -> Result<Json<ResetPasswordResponse>, ApiError> {
    let role = normalize_password_reset_role(&request.role)?;
    let email = normalize_email(&request.email)?;
    validate_password_reset_otp(&request.otp)?;
    validate_new_password(&role, &request.new_password)?;

    match role.as_str() {
        "hospital" => reset_hospital_password(&state, &email, &request.otp, &request.new_password).await?,
        "patient" => reset_patient_password(&state, &email, &request.otp, &request.new_password).await?,
        _ => unreachable!(),
    }

    Ok(Json(ResetPasswordResponse {
        role,
        email,
        message: "Password reset successfully.".to_owned(),
    }))
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedHospital {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing bearer token".to_owned()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("invalid bearer token".to_owned()))?;

        state
            .token_service
            .verify_access_token(token)
            .map_err(|_| ApiError::Unauthorized("invalid or expired token".to_owned()))
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing bearer token".to_owned()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("invalid bearer token".to_owned()))?;

        state
            .token_service
            .verify_admin_access_token(token)
            .map_err(|_| ApiError::Unauthorized("invalid or expired token".to_owned()))
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedPatient {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing bearer token".to_owned()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("invalid bearer token".to_owned()))?;

        state
            .token_service
            .verify_patient_access_token(token)
            .map_err(|_| ApiError::Unauthorized("invalid or expired token".to_owned()))
    }
}

pub async fn unauthorized() -> StatusCode {
    StatusCode::UNAUTHORIZED
}

fn validate_login_request(request: &LoginRequest) -> Result<(), ApiError> {
    if request.email.trim().is_empty() || !request.email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    if request.password.trim().is_empty() {
        return Err(ApiError::BadRequest("password is required".to_owned()));
    }

    Ok(())
}

async fn request_hospital_password_reset(state: &AppState, email: &str) -> Result<(), ApiError> {
    let Some(hospital) = state.hospital_repository.find_hospital_by_email(email).await? else {
        return Ok(());
    };

    if let Some(created_at) = state
        .hospital_repository
        .latest_password_reset_otp_created_at(hospital.id)
        .await?
    {
        let next_allowed_at =
            created_at + Duration::seconds(PASSWORD_RESET_OTP_RESEND_COOLDOWN_SECONDS);
        if next_allowed_at > Utc::now() {
            return Ok(());
        }
    }

    let otp = generate_otp();
    send_password_reset_otp(
        state,
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
        .invalidate_active_password_reset_otps(hospital.id)
        .await?;

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    state
        .hospital_repository
        .create_password_reset_otp(NewHospitalPasswordResetOtp {
            hospital_id: hospital.id,
            email: hospital.email,
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(PASSWORD_RESET_OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    Ok(())
}

async fn request_patient_password_reset(state: &AppState, email: &str) -> Result<(), ApiError> {
    let Some(patient) = state.patient_repository.find_patient_by_email(email).await? else {
        return Ok(());
    };

    if let Some(created_at) = state
        .patient_repository
        .latest_password_reset_otp_created_at(patient.id)
        .await?
    {
        let next_allowed_at =
            created_at + Duration::seconds(PASSWORD_RESET_OTP_RESEND_COOLDOWN_SECONDS);
        if next_allowed_at > Utc::now() {
            return Ok(());
        }
    }

    let otp = generate_otp();
    send_password_reset_otp(state, email, &patient.full_name, &otp).await?;

    state
        .patient_repository
        .invalidate_active_password_reset_otps(patient.id)
        .await?;

    let otp_hash = state
        .password_hasher
        .hash_password(&otp)
        .map_err(|_| ApiError::Internal("failed to hash OTP".to_owned()))?;

    state
        .patient_repository
        .create_password_reset_otp(NewPatientPasswordResetOtp {
            patient_id: patient.id,
            email: email.to_owned(),
            otp_hash,
            expires_at: Utc::now() + Duration::seconds(PASSWORD_RESET_OTP_EXPIRES_IN_SECONDS),
        })
        .await?;

    Ok(())
}

async fn reset_hospital_password(
    state: &AppState,
    email: &str,
    otp: &str,
    new_password: &str,
) -> Result<(), ApiError> {
    let reset_otp = state
        .hospital_repository
        .find_latest_password_reset_otp(email)
        .await?
        .ok_or_else(invalid_password_reset_otp)?;

    if reset_otp.used_at.is_some() {
        return Err(ApiError::BadRequest("OTP has already been used".to_owned()));
    }

    if reset_otp.expires_at <= Utc::now() {
        return Err(ApiError::BadRequest("OTP has expired".to_owned()));
    }

    if reset_otp.attempt_count >= PASSWORD_RESET_OTP_MAX_ATTEMPTS {
        return Err(ApiError::BadRequest(
            "maximum OTP verification attempts exceeded".to_owned(),
        ));
    }

    let otp_matches = state
        .password_hasher
        .verify_password(otp.trim(), &reset_otp.otp_hash)
        .unwrap_or(false);

    if !otp_matches {
        state
            .hospital_repository
            .increment_password_reset_otp_attempts(reset_otp.id)
            .await?;

        return Err(invalid_password_reset_otp());
    }

    let password_hash = state
        .password_hasher
        .hash_password(new_password)
        .map_err(|_| ApiError::Internal("failed to hash password".to_owned()))?;

    state
        .hospital_repository
        .mark_password_reset_otp_used(reset_otp.id)
        .await?;
    state
        .hospital_repository
        .update_hospital_password(reset_otp.hospital_id, password_hash)
        .await?;

    Ok(())
}

async fn reset_patient_password(
    state: &AppState,
    email: &str,
    otp: &str,
    new_password: &str,
) -> Result<(), ApiError> {
    let reset_otp = state
        .patient_repository
        .find_latest_password_reset_otp(email)
        .await?
        .ok_or_else(invalid_password_reset_otp)?;

    if reset_otp.used_at.is_some() {
        return Err(ApiError::BadRequest("OTP has already been used".to_owned()));
    }

    if reset_otp.expires_at <= Utc::now() {
        return Err(ApiError::BadRequest("OTP has expired".to_owned()));
    }

    if reset_otp.attempt_count >= PASSWORD_RESET_OTP_MAX_ATTEMPTS {
        return Err(ApiError::BadRequest(
            "maximum OTP verification attempts exceeded".to_owned(),
        ));
    }

    let otp_matches = state
        .password_hasher
        .verify_password(otp.trim(), &reset_otp.otp_hash)
        .unwrap_or(false);

    if !otp_matches {
        state
            .patient_repository
            .increment_password_reset_otp_attempts(reset_otp.id)
            .await?;

        return Err(invalid_password_reset_otp());
    }

    let password_hash = state
        .password_hasher
        .hash_password(new_password)
        .map_err(|_| ApiError::Internal("failed to hash password".to_owned()))?;

    state
        .patient_repository
        .mark_password_reset_otp_used(reset_otp.id)
        .await?;
    state
        .patient_repository
        .update_patient_password(reset_otp.patient_id, password_hash)
        .await?;

    Ok(())
}

async fn send_password_reset_otp(
    state: &AppState,
    email: &str,
    name: &str,
    otp: &str,
) -> Result<(), ApiError> {
    let subject = "Reset your Korede Health password".to_owned();
    let text_body = format!(
        "Hello {},\n\nYour Korede Health password reset code is {}.\n\nThis code expires in 5 minutes.\n\nIf you did not request a password reset, you can ignore this email.\n\nThank you,\nKorede Health",
        name.trim(),
        otp
    );
    let html_body = format!(
        "<p>Hello {},</p><p>Your Korede Health password reset code is <strong>{}</strong>.</p><p>This code expires in 5 minutes.</p><p>If you did not request a password reset, you can ignore this email.</p><p>Thank you,<br>Korede Health</p>",
        name.trim(),
        otp
    );

    state
        .email_service
        .send(EmailMessage {
            to_email: email.to_owned(),
            to_name: Some(name.trim().to_owned()),
            subject,
            text_body,
            html_body: Some(html_body),
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to send password reset OTP");
            ApiError::Internal("failed to send password reset OTP".to_owned())
        })
}

fn normalize_password_reset_role(role: &str) -> Result<String, ApiError> {
    match role.trim().to_lowercase().as_str() {
        "hospital" => Ok("hospital".to_owned()),
        "patient" => Ok("patient".to_owned()),
        _ => Err(ApiError::BadRequest(
            "role must be either hospital or patient".to_owned(),
        )),
    }
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();

    if email.is_empty() || !email.contains('@') || !email.contains('.') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    Ok(email)
}

fn validate_password_reset_otp(otp: &str) -> Result<(), ApiError> {
    let otp = otp.trim();
    if otp.len() != PASSWORD_RESET_OTP_LENGTH
        || !otp.chars().all(|character| character.is_ascii_digit())
    {
        return Err(ApiError::BadRequest(
            "OTP must be a 6-digit code".to_owned(),
        ));
    }

    Ok(())
}

fn validate_new_password(role: &str, password: &str) -> Result<(), ApiError> {
    let minimum_length = match role {
        "hospital" => 12,
        "patient" => 8,
        _ => unreachable!(),
    };

    if password.len() < minimum_length {
        return Err(ApiError::BadRequest(format!(
            "password must be at least {minimum_length} characters"
        )));
    }

    Ok(())
}

fn generate_otp() -> String {
    let value = Uuid::new_v4().as_u128() % 1_000_000;
    format!("{value:06}")
}

fn invalid_credentials() -> ApiError {
    ApiError::Unauthorized("invalid email or password".to_owned())
}

fn invalid_refresh_token() -> ApiError {
    ApiError::Unauthorized("invalid or expired refresh token".to_owned())
}

fn invalid_password_reset_otp() -> ApiError {
    ApiError::BadRequest("invalid or expired OTP".to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_login_request() {
        let request = LoginRequest {
            email: "admin@example.com".to_owned(),
            password: "password".to_owned(),
        };

        assert!(validate_login_request(&request).is_ok());
    }

    #[test]
    fn rejects_invalid_login_email() {
        let request = LoginRequest {
            email: "admin".to_owned(),
            password: "password".to_owned(),
        };

        assert!(matches!(
            validate_login_request(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn constant_time_eq_matches_equal_values() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"wrong"));
        assert!(!constant_time_eq(b"secret", b"secret1"));
    }

    #[test]
    fn password_reset_role_validation_accepts_supported_roles() {
        assert_eq!(
            normalize_password_reset_role("hospital").unwrap(),
            "hospital"
        );
        assert_eq!(normalize_password_reset_role("PATIENT").unwrap(), "patient");
    }

    #[test]
    fn password_reset_role_validation_rejects_unsupported_role() {
        assert!(matches!(
            normalize_password_reset_role("admin"),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn password_reset_otp_validation_accepts_six_digits() {
        assert!(validate_password_reset_otp("123456").is_ok());
    }

    #[test]
    fn password_reset_otp_validation_rejects_invalid_shape() {
        assert!(matches!(
            validate_password_reset_otp("12345"),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            validate_password_reset_otp("abcdef"),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn new_password_validation_uses_role_specific_minimums() {
        assert!(validate_new_password("patient", "12345678").is_ok());
        assert!(validate_new_password("hospital", "123456789012").is_ok());
        assert!(matches!(
            validate_new_password("patient", "1234567"),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            validate_new_password("hospital", "12345678901"),
            Err(ApiError::BadRequest(_))
        ));
    }
}
