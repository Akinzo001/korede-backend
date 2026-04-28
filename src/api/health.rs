use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use utoipa::ToSchema;

use crate::api::AppState;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct DatabaseHealthResponse {
    status: &'static str,
    database: &'static str,
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "The Korede API is running.", body = HealthResponse)
    )
)]
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "korede_backend",
    })
}

#[utoipa::path(
    get,
    path = "/health/db",
    tag = "Health",
    responses(
        (status = 200, description = "The API can reach PostgreSQL.", body = DatabaseHealthResponse),
        (status = 503, description = "The API cannot reach PostgreSQL.")
    )
)]
pub async fn database_health_check(
    State(state): State<AppState>,
) -> Result<Json<DatabaseHealthResponse>, StatusCode> {
    sqlx::query("SELECT 1")
        .execute(&state.db_pool)
        .await
        .map_err(|error| {
            tracing::error!(%error, "database health check failed");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    Ok(Json(DatabaseHealthResponse {
        status: "ok",
        database: "postgres",
    }))
}
