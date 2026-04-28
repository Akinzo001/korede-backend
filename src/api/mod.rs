pub mod docs;
pub mod health;

use axum::{Router, routing::get};
use sqlx::PgPool;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", docs::ApiDoc::openapi()))
        .route("/health", get(health::health_check))
        .route("/health/db", get(health::database_health_check))
        .with_state(state)
}
