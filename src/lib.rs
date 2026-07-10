// This file is the root of the library crate.
//
// `main.rs` is the binary entry point that starts the server.
// `lib.rs` exposes reusable modules that `main.rs` and tests can import.

// Infrastructure adapters, such as PostgreSQL.
pub mod adapters;

// Application services that orchestrate business workflows through ports.
pub mod application;

// HTTP/API layer, such as Axum routes and Swagger docs.
pub mod api;

// Pure business/domain models.
pub mod domain;

// Cross-cutting infrastructure, such as config and logging.
pub mod infrastructure;

// Ports are interfaces/traits the domain can depend on later.
pub mod port;
