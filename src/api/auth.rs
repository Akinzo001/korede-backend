use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};

use crate::{
    api::AppState,
    port::auth::{AuthenticatedAdmin, AuthenticatedHospital},
};

use super::error::ApiError;

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
