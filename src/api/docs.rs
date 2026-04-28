use utoipa::OpenApi;

use crate::api::health::{DatabaseHealthResponse, HealthResponse};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Korede Backend API",
        version = "0.1.0",
        description = "API documentation for the Korede backend. Korede helps donors fund verified hospital bills while keeping transactions transparent and auditable."
    ),
    paths(
        crate::api::health::health_check,
        crate::api::health::database_health_check
    ),
    components(
        schemas(HealthResponse, DatabaseHealthResponse)
    ),
    tags(
        (name = "Health", description = "Endpoints for checking whether the API and database are working.")
    )
)]
pub struct ApiDoc;
