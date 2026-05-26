use axum::{Json, Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::{AppState, error::ApiError};

pub fn routes() -> Router<AppState> {
    Router::new().route("/login", post(login_admin))
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

#[utoipa::path(
    post,
    path = "/api/v1/admin/login",
    tag = "Admin",
    request_body = AdminLoginRequest,
    responses(
        (status = 200, description = "Super-admin authenticated successfully.", body = AdminLoginResponse),
        (status = 400, description = "Invalid login request."),
        (status = 401, description = "Invalid admin credentials.")
    )
)]
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
