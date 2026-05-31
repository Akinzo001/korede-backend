use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::refresh_token::RefreshToken;

#[derive(Debug, Clone)]
pub struct NewRefreshToken {
    pub token_hash: String,
    pub subject_id: String,
    pub email: String,
    pub role: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum RefreshTokenRepositoryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    async fn create_refresh_token(
        &self,
        token: NewRefreshToken,
    ) -> Result<RefreshToken, RefreshTokenRepositoryError>;

    async fn find_refresh_token_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RefreshTokenRepositoryError>;

    async fn revoke_refresh_token(&self, id: Uuid) -> Result<bool, RefreshTokenRepositoryError>;
}
