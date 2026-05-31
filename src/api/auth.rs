use axum::{
    Json, Router, async_trait,
    extract::{FromRequestParts, State},
    http::{HeaderMap, StatusCode, request::Parts},
    routing::post,
};
use chrono::Utc;
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
    port::auth::{AuthenticatedAdmin, AuthenticatedHospital},
};

use super::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/login/verify-otp", post(verify_login_otp))
        .route("/refresh", post(refresh_token))
}

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
    Ok(Json(LoginResponse {
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
    }))
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

fn invalid_credentials() -> ApiError {
    ApiError::Unauthorized("invalid email or password".to_owned())
}

fn invalid_refresh_token() -> ApiError {
    ApiError::Unauthorized("invalid or expired refresh token".to_owned())
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
}
