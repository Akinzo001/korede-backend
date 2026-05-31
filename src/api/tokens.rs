use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{api::AppState, port::refresh_token::NewRefreshToken};

use super::error::ApiError;

pub async fn issue_refresh_token(
    state: &AppState,
    subject_id: String,
    email: &str,
    role: &str,
) -> Result<String, ApiError> {
    let refresh_token = generate_refresh_token();
    let token_hash = hash_refresh_token(&refresh_token);

    state
        .refresh_token_repository
        .create_refresh_token(NewRefreshToken {
            token_hash,
            subject_id,
            email: email.to_owned(),
            role: role.to_owned(),
            expires_at: Utc::now() + Duration::seconds(state.refresh_token_expires_in_seconds),
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "failed to create refresh token");
            ApiError::Internal("failed to create refresh token".to_owned())
        })?;

    Ok(refresh_token)
}

pub fn hash_refresh_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn generate_refresh_token() -> String {
    format!(
        "rt_{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_refresh_token_deterministically() {
        assert_eq!(hash_refresh_token("token"), hash_refresh_token("token"));
        assert_ne!(hash_refresh_token("token"), hash_refresh_token("other"));
    }

    #[test]
    fn generated_refresh_token_has_expected_shape() {
        let token = generate_refresh_token();

        assert!(token.starts_with("rt_"));
        assert_eq!(token.len(), 99);
    }
}
