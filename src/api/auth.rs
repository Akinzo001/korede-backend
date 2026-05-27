use axum::{
    Json, Router, async_trait,
    extract::{FromRequestParts, State},
    http::{HeaderMap, StatusCode, request::Parts},
    routing::post,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{
        AppState,
        hospitals::{self, LoginHospitalRequest, VerifyLoginOtpRequest, VerifyLoginOtpResponse},
    },
    port::auth::{AuthenticatedAdmin, AuthenticatedHospital},
};

use super::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/login/verify-otp", post(verify_login_otp))
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
    pub expires_in: Option<i64>,
    pub otp_required: bool,
    pub login_challenge_id: Option<Uuid>,
    pub email: Option<String>,
    pub otp_expires_in_seconds: Option<i64>,
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

        return Ok(Json(LoginResponse {
            role: "admin".to_owned(),
            token_type: Some("Bearer".to_owned()),
            access_token: Some(access_token),
            expires_in: Some(state.jwt_expires_in_seconds),
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
        expires_in: None,
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
