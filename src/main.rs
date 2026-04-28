// `std` means "standard library". These are built into Rust.
//
// `env` lets us read environment variables like DATABASE_URL.
// `SocketAddr` represents an IP address + port, for example:
// 127.0.0.1:4000
use std::{env, net::SocketAddr};

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
};

// `tracing_subscriber` configures logging for the application.
// `tracing::info!`, `tracing::error!`, etc. need a subscriber
// to decide how logs should be printed.
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
    // Load environment variables from the `.env` file into the process.
    //
    // Example from `.env`:
    // DATABASE_URL=postgres://postgres:postgres@localhost:5432/korede_backend
    //
    // `.ok()` means:
    // "If loading `.env` fails, don't crash here."
    //
    // This is useful because in production you may not use a `.env` file.
    // The environment variables may already be provided by the server.
    dotenvy::dotenv().ok();

    // Configure application logging.
    //
    // Think of this as saying:
    // "When the app logs something, print it nicely to the terminal."
    tracing_subscriber::registry()
        .with(
            // Try to read a logging filter from the environment.
            //
            // Example:
            // RUST_LOG=debug
            //
            // If RUST_LOG exists, Rust will use it.
            tracing_subscriber::EnvFilter::try_from_default_env()
                // If RUST_LOG does not exist, use this default:
                //
                // korede_backend=debug:
                //   Show debug-level logs from this project.
                //
                // tower_http=debug:
                //   Show debug logs from Tower HTTP middleware if we add it later.
                .unwrap_or_else(|_| "korede_backend=debug,tower_http=debug".into()),
        )
        // Use the default terminal-friendly log format.
        .with(tracing_subscriber::fmt::layer())
        // Activate this logging setup globally.
        .init();

    // Read the DATABASE_URL environment variable.
    //
    // `env::var("DATABASE_URL")` returns a Result:
    // - Ok(value) if the variable exists
    // - Err(...) if it does not exist
    //
    // `.expect(...)` means:
    // "If DATABASE_URL is missing, stop the app and show this message."
    //
    // We want to stop immediately because the app cannot run without the DB.
    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL must be set in your .env file");

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
    let db_pool = connect(&database_url).await?;

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

    // Choose where the server will listen.
    //
    // 127.0.0.1 means "localhost only".
    // Your machine can access it, but other machines cannot.
    //
    // 4000 is the port number.
    //
    // Full local URL:
    // http://127.0.0.1:4000
    let address = SocketAddr::from(([127, 0, 0, 1], 4000));

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
