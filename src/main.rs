// This project has both:
// 1. a library crate: `src/lib.rs`
// 2. a binary crate: `src/main.rs`
//
// `src/lib.rs` publicly exposes modules like `adapters` and `api`.
// That is why `main.rs` can import them using the package name:
// `korede_backend`.
use korede_backend::{
    // `connect` creates a PostgreSQL connection pool.
    // `run_migrations` applies SQL files from the `migrations/` folder.
    adapters::db::postgres::{connect, run_migrations},

    // `AppState` is shared application state.
    // Right now it contains the database pool.
    //
    // `app` builds the Axum router with routes like:
    // GET /health
    // GET /health/db
    api::{AppState, app},

    // `AppConfig` is the central place where environment variables
    // are loaded and validated.
    //
    // `init_tracing` configures logging/tracing for the whole app.
    infrastructure::{config::AppConfig, logging::init_tracing},
};

// Rust's normal `main` function cannot use `.await` by itself.
//
// But this backend needs async operations:
// - connecting to PostgreSQL
// - running migrations
// - listening for HTTP requests
//
// `#[tokio::main]` tells Rust:
// "Start the Tokio async runtime before running this main function."
//
// Tokio is the async engine that Axum uses under the hood.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure application logging/tracing once, at startup.
    //
    // This handles logs from your own code, plus request logs from
    // the HTTP tracing middleware.
    init_tracing();

    // Load and validate environment variables.
    //
    // Required:
    // - DATABASE_URL
    // - JWT_SECRET
    //
    // Optional, with defaults:
    // - APP_HOST defaults to 127.0.0.1
    // - APP_PORT defaults to 4000
    // - SOLANA_RPC_URL defaults to Solana devnet
    //
    // Optional API keys:
    // - PAYSTACK_SECRET_KEY
    // - FLUTTERWAVE_SECRET_KEY
    let config = AppConfig::from_env()?;

    // Create the PostgreSQL connection pool.
    //
    // A pool is a reusable group of database connections.
    // Instead of opening a new PostgreSQL connection for every request,
    // the app borrows an existing connection from the pool.
    //
    // `.await` means:
    // "This is async work. Pause here until the database connection is ready."
    //
    // The `?` means:
    // "If this fails, return the error from main immediately."
    //
    // So if PostgreSQL is not running, this line will fail.
    let db_pool = connect(&config.database.url).await?;

    // Run database migrations on startup.
    //
    // Migrations are SQL files that create or update database tables.
    //
    // In this project, the first migration creates:
    // - hospitals
    // - patients
    // - medical_cases
    //
    // SQLx remembers which migrations have already been run,
    // so it will not rerun the same migration every time.
    run_migrations(&db_pool).await?;

    // Build shared application state.
    //
    // Axum handlers often need access to shared things like:
    // - database pool
    // - config
    // - services
    //
    // Right now our state only contains `db_pool`.
    //
    // Important Rust detail:
    // This moves ownership of `db_pool` into `AppState`.
    // That is fine because from this point onward the router owns the state.
    let state = AppState { db_pool };

    // Build the Axum router.
    //
    // A router maps HTTP requests to handler functions.
    //
    // For example:
    // GET /health    -> health_check()
    // GET /health/db -> database_health_check()
    //
    // The router also receives the shared `state`,
    // so handlers can use the database pool when needed.
    let router = app(state);

    // Choose where the server will listen based on APP_HOST and APP_PORT.
    let address = config.server_addr()?;

    // Bind a TCP listener to the address.
    //
    // A listener is what waits for incoming HTTP connections.
    //
    // If another app is already using port 4000,
    // this line will fail.
    let listener = tokio::net::TcpListener::bind(address).await?;

    // Print a startup message to the terminal.
    //
    // This uses the logging system we configured earlier.
    tracing::info!("Korede backend listening on http://{address}");

    // Start serving HTTP requests using Axum.
    //
    // This line keeps running while the server is alive.
    // It does not finish after one request.
    //
    // You usually stop it with Ctrl+C in the terminal.
    axum::serve(listener, router).await?;

    // If the server shuts down cleanly, return Ok.
    //
    // In Rust, `Ok(())` means:
    // "The program finished successfully and returns no useful value."
    Ok(())
}
