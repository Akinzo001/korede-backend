// `OpenApi` is a derive macro from utoipa.
//
// It generates an OpenAPI specification from the routes and schemas we list.
use utoipa::OpenApi;

// These response structs are included as schemas in the generated API docs.
use crate::api::health::{DatabaseHealthResponse, HealthResponse};

// Generate an OpenAPI document for the backend.
//
// `derive(OpenApi)` tells utoipa to create the documentation object for us.
#[derive(OpenApi)]
#[openapi(
    // General metadata shown in Swagger UI.
    info(
        title = "Korede Backend API",
        version = "0.1.0",
        description = "API documentation for the Korede backend. Korede helps donors fund verified hospital bills while keeping transactions transparent and auditable."
    ),
    // List every handler function that should appear in the docs.
    paths(
        crate::api::health::health_check,
        crate::api::health::database_health_check
    ),
    // List every response/request type that should appear as a schema.
    components(
        schemas(HealthResponse, DatabaseHealthResponse)
    ),
    // Group endpoints into named sections in Swagger UI.
    tags(
        (name = "Health", description = "Endpoints for checking whether the API and database are working.")
    )
)]
// Empty struct used only as a type that owns the generated OpenAPI document.
//
// You call `ApiDoc::openapi()` in `api/mod.rs`.
pub struct ApiDoc;
