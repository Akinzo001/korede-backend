use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthenticatedHospital {
    pub hospital_id: Uuid,
    pub email: String,
    pub role: String,
}

#[derive(Debug, Error)]
pub enum PasswordHashError {
    #[error("failed to hash password")]
    HashFailed,

    #[error("failed to verify password")]
    VerifyFailed,
}

pub trait PasswordHasher: Send + Sync {
    fn hash_password(&self, password: &str) -> Result<String, PasswordHashError>;

    fn verify_password(
        &self,
        password: &str,
        password_hash: &str,
    ) -> Result<bool, PasswordHashError>;
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("failed to create token")]
    CreateFailed,

    #[error("invalid token")]
    Invalid,
}

pub trait TokenService: Send + Sync {
    fn create_access_token(&self, hospital_id: Uuid, email: &str) -> Result<String, TokenError>;

    fn verify_access_token(&self, token: &str) -> Result<AuthenticatedHospital, TokenError>;
}
