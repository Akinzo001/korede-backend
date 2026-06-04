use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::port::{hospital::HospitalRepositoryError, patient::PatientRepositoryError};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    PayloadTooLarge(String),
    UnsupportedMediaType(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::PayloadTooLarge(message) => (StatusCode::PAYLOAD_TOO_LARGE, message),
            Self::UnsupportedMediaType(message) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, message),
            Self::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<HospitalRepositoryError> for ApiError {
    fn from(error: HospitalRepositoryError) -> Self {
        match error {
            HospitalRepositoryError::DuplicateEmail => {
                Self::Conflict("hospital email already exists".to_owned())
            }
            HospitalRepositoryError::NotFound => Self::NotFound("hospital not found".to_owned()),
            HospitalRepositoryError::Database(error) => {
                tracing::error!(%error, "database operation failed");
                Self::Internal("internal server error".to_owned())
            }
        }
    }
}

impl From<PatientRepositoryError> for ApiError {
    fn from(error: PatientRepositoryError) -> Self {
        match error {
            PatientRepositoryError::DuplicateUsername => {
                Self::Conflict("patient username already exists".to_owned())
            }
            PatientRepositoryError::DuplicateEmail => {
                Self::Conflict("patient email already exists".to_owned())
            }
            PatientRepositoryError::NotFound => Self::NotFound("patient not found".to_owned()),
            PatientRepositoryError::Database(error) => {
                tracing::error!(%error, "database operation failed");
                Self::Internal("internal server error".to_owned())
            }
        }
    }
}
