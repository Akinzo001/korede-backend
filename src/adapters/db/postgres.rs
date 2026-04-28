// `PgPool` is SQLx's PostgreSQL connection pool type.
// `PgPoolOptions` configures how the pool should be created.
use sqlx::{PgPool, postgres::PgPoolOptions};

// Create a PostgreSQL connection pool.
//
// `database_url` is borrowed as `&str`, meaning this function reads it
// without taking ownership of the original String.
//
// Return type:
// Result<PgPool, sqlx::Error>
//
// This means:
// - Ok(pool) if the connection succeeds
// - Err(error) if the connection fails
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    // Start configuring a new PostgreSQL pool.
    PgPoolOptions::new()
        // Allow up to 5 open database connections at the same time.
        .max_connections(5)
        // Connect to PostgreSQL using the provided URL.
        .connect(database_url)
        // Wait for the async connection attempt to finish.
        .await
}

// Run SQLx migrations against the database.
//
// `pool: &PgPool` means we borrow the existing database pool.
// We do not create a second pool here.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    // `sqlx::migrate!("./migrations")` embeds the migration files at compile time.
    //
    // `.run(pool)` applies any migration that has not already been applied.
    sqlx::migrate!("./migrations").run(pool).await
}
