use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::refresh_token::RefreshToken,
    port::refresh_token::{NewRefreshToken, RefreshTokenRepository, RefreshTokenRepositoryError},
};

#[derive(Debug, Clone)]
pub struct PostgresRefreshTokenRepository {
    pool: PgPool,
}

impl PostgresRefreshTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RefreshTokenRepository for PostgresRefreshTokenRepository {
    async fn create_refresh_token(
        &self,
        token: NewRefreshToken,
    ) -> Result<RefreshToken, RefreshTokenRepositoryError> {
        let id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO auth_refresh_tokens (
                id,
                token_hash,
                subject_id,
                email,
                role,
                expires_at,
                created_at
            )
            VALUES ($1, $2, $3, LOWER($4), $5, $6, NOW())
            RETURNING
                id,
                token_hash,
                subject_id,
                email,
                role,
                expires_at,
                revoked_at,
                created_at,
                last_used_at
            "#,
        )
        .bind(id)
        .bind(token.token_hash)
        .bind(token.subject_id)
        .bind(token.email)
        .bind(token.role)
        .bind(token.expires_at)
        .fetch_one(&self.pool)
        .await?;

        refresh_token_from_row(&row).map_err(RefreshTokenRepositoryError::from)
    }

    async fn find_refresh_token_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RefreshTokenRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                token_hash,
                subject_id,
                email,
                role,
                expires_at,
                revoked_at,
                created_at,
                last_used_at
            FROM auth_refresh_tokens
            WHERE token_hash = $1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref()
            .map(refresh_token_from_row)
            .transpose()
            .map_err(RefreshTokenRepositoryError::from)
    }

    async fn revoke_refresh_token(&self, id: Uuid) -> Result<bool, RefreshTokenRepositoryError> {
        let result = sqlx::query(
            r#"
            UPDATE auth_refresh_tokens
            SET revoked_at = NOW(),
                last_used_at = NOW()
            WHERE id = $1
                AND revoked_at IS NULL
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }
}

fn refresh_token_from_row(row: &sqlx::postgres::PgRow) -> Result<RefreshToken, sqlx::Error> {
    Ok(RefreshToken {
        id: row.try_get("id")?,
        token_hash: row.try_get("token_hash")?,
        subject_id: row.try_get("subject_id")?,
        email: row.try_get("email")?,
        role: row.try_get("role")?,
        expires_at: row.try_get("expires_at")?,
        revoked_at: row.try_get("revoked_at")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
    })
}
