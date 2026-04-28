// `State` extracts shared application state from Axum.
// `StatusCode` gives us HTTP status codes like 503.
// `Json` turns Rust values into JSON HTTP responses.
use axum::{Json, extract::State, http::StatusCode};

// `Serialize` lets structs be converted into JSON.
use serde::Serialize;

// `ToSchema` lets utoipa include these structs in OpenAPI docs.
use utoipa::ToSchema;

// Import the shared AppState type from the api module.
use crate::api::AppState;

// Response body for GET /health.
//
// `Serialize` allows this struct to become JSON.
// `ToSchema` allows this struct to appear in Swagger/OpenAPI docs.
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    // Simple machine-readable status.
    status: &'static str,

    // Name of this backend service.
    service: &'static str,
}

// Response body for GET /health/db.
#[derive(Serialize, ToSchema)]
pub struct DatabaseHealthResponse {
    // Simple machine-readable status.
    status: &'static str,

    // The database engine being checked.
    database: &'static str,
}

// OpenAPI documentation for the health_check handler below.
#[utoipa::path(
    // This endpoint uses the HTTP GET method.
    get,
    // This is the URL path.
    path = "/health",
    // This endpoint appears under the "Health" section in Swagger.
    tag = "Health",
    // Possible responses shown in the API docs.
    responses(
        (status = 200, description = "The Korede API is running.", body = HealthResponse)
    )
)]
// Handler for GET /health.
//
// `async` means this function can be awaited by Axum's async runtime.
// It returns JSON containing a small status object.
pub async fn health_check() -> Json<HealthResponse> {
    // Build and return a JSON response.
    Json(HealthResponse {
        status: "ok",
        service: "korede_backend",
    })
}

// OpenAPI documentation for the database_health_check handler below.
#[utoipa::path(
    // This endpoint uses the HTTP GET method.
    get,
    // This is the URL path.
    path = "/health/db",
    // This endpoint appears under the "Health" section in Swagger.
    tag = "Health",
    // Possible responses shown in the API docs.
    responses(
        (status = 200, description = "The API can reach PostgreSQL.", body = DatabaseHealthResponse),
        (status = 503, description = "The API cannot reach PostgreSQL.")
    )
)]
// Handler for GET /health/db.
//
// It checks whether the backend can talk to PostgreSQL.
pub async fn database_health_check(
    // `State(state)` extracts AppState from the Axum router.
    //
    // This gives the handler access to `state.db_pool`.
    State(state): State<AppState>,
) -> Result<Json<DatabaseHealthResponse>, StatusCode> {
    // Run a very small SQL query.
    //
    // `SELECT 1` is commonly used as a lightweight database health check.
    sqlx::query("SELECT 1")
        // Execute the query using the shared PostgreSQL pool.
        .execute(&state.db_pool)
        // Wait for the async database operation to finish.
        .await
        // If SQLx returns an error, log it and convert it into HTTP 503.
        .map_err(|error| {
            tracing::error!(%error, "database health check failed");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    // If the query worked, return a successful JSON response.
    Ok(Json(DatabaseHealthResponse {
        status: "ok",
        database: "postgres",
    }))
}
