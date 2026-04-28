// `std` is Rust's standard library.
//
// `env` lets us read environment variables such as DATABASE_URL.
// `SocketAddr` represents a network address like 127.0.0.1:4000.
use std::{env, net::SocketAddr};

// `thiserror::Error` lets us create clean custom error types
// without manually writing a lot of error-handling boilerplate.
use thiserror::Error;

// `AppConfig` is the top-level configuration object for the whole app.
//
// Instead of reading environment variables everywhere in the codebase,
// we read them once here and pass around this typed config.
#[derive(Debug, Clone)]
pub struct AppConfig {
    // Server-related settings, like host and port.
    pub server: ServerConfig,

    // Database-related settings, like the PostgreSQL URL.
    pub database: DatabaseConfig,

    // Authentication-related settings, like JWT secret.
    pub auth: AuthConfig,

    // Solana-related settings, like RPC URL.
    pub solana: SolanaConfig,

    // Payment provider settings, like Paystack or Flutterwave keys.
    pub payments: PaymentConfig,
}

// Settings for the HTTP server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    // The network host the app should listen on.
    //
    // Local development usually uses 127.0.0.1.
    // Cloud deployment usually uses 0.0.0.0.
    pub host: String,

    // The TCP port the app should listen on.
    //
    // Example: 4000 means http://127.0.0.1:4000 locally.
    pub port: u16,
}

// Settings for PostgreSQL.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    // The full PostgreSQL connection URL.
    //
    // Example:
    // postgres://user:password@host:port/database?sslmode=require
    pub url: String,
}

// Settings for authentication.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    // Secret used later to sign and verify JWT tokens.
    //
    // This must be private in production.
    pub jwt_secret: String,
}

// Settings for Solana.
#[derive(Debug, Clone)]
pub struct SolanaConfig {
    // The RPC endpoint the backend will use when talking to Solana.
    //
    // For early development we default to devnet.
    pub rpc_url: String,
}

// Settings for payment providers.
#[derive(Debug, Clone)]
pub struct PaymentConfig {
    // Paystack secret key.
    //
    // `Option<String>` means this value may or may not exist.
    // It is `Some(value)` when configured and `None` when missing.
    pub paystack_secret_key: Option<String>,

    // Flutterwave secret key.
    //
    // Also optional because you may not integrate every provider immediately.
    pub flutterwave_secret_key: Option<String>,
}

// Errors that can happen while loading config.
//
// `Debug` helps Rust print the error while debugging.
// `Error` comes from `thiserror` and turns this enum into a real error type.
#[derive(Debug, Error)]
pub enum ConfigError {
    // This error is used when a required environment variable is missing.
    //
    // `{0}` means "print the first value inside this enum variant".
    #[error("missing required environment variable: {0}")]
    MissingVariable(&'static str),

    // This error is used when APP_PORT exists but is not a valid number.
    #[error("APP_PORT must be a valid port number, got: {0}")]
    InvalidPort(String),

    // This error is used when APP_HOST + APP_PORT cannot become a SocketAddr.
    #[error("APP_HOST and APP_PORT must form a valid socket address, got: {0}")]
    InvalidSocketAddress(String),
}

// `impl AppConfig` means:
// "Define functions that belong to AppConfig."
impl AppConfig {
    // Load the app configuration from environment variables.
    //
    // Return type:
    // Result<Self, ConfigError>
    //
    // `Self` means AppConfig.
    // So this returns either:
    // - Ok(AppConfig)
    // - Err(ConfigError)
    pub fn from_env() -> Result<Self, ConfigError> {
        // Load variables from `.env` into the running process.
        //
        // `.ok()` intentionally ignores failure because production environments
        // often provide variables directly without a `.env` file.
        dotenvy::dotenv().ok();

        // Build and return the full AppConfig.
        Ok(Self {
            // Build the nested server config.
            server: ServerConfig {
                // Read APP_HOST if it exists.
                //
                // If APP_HOST is missing or empty, use 127.0.0.1.
                host: optional_env("APP_HOST").unwrap_or_else(|| "127.0.0.1".to_owned()),

                // Read APP_PORT if it exists.
                //
                // If APP_PORT is missing or empty, use "4000".
                // Then parse the string into a u16 number.
                port: optional_env("APP_PORT")
                    .unwrap_or_else(|| "4000".to_owned())
                    .parse()
                    .map_err(|_| {
                        // If parsing fails, convert that parsing failure
                        // into our own ConfigError::InvalidPort.
                        ConfigError::InvalidPort(
                            optional_env("APP_PORT").unwrap_or_else(|| "4000".to_owned()),
                        )
                    })?,
            },

            // Build the nested database config.
            database: DatabaseConfig {
                // DATABASE_URL is required because the app cannot run
                // without a PostgreSQL connection string.
                url: required_env("DATABASE_URL")?,
            },

            // Build the nested auth config.
            auth: AuthConfig {
                // JWT_SECRET is required because auth will need it later.
                jwt_secret: required_env("JWT_SECRET")?,
            },

            // Build the nested Solana config.
            solana: SolanaConfig {
                // SOLANA_RPC_URL is optional for now.
                //
                // If it is missing, we use Solana devnet so development
                // does not accidentally point at mainnet.
                rpc_url: optional_env("SOLANA_RPC_URL")
                    .unwrap_or_else(|| "https://api.devnet.solana.com".to_owned()),
            },

            // Build the nested payment config.
            payments: PaymentConfig {
                // These are optional because payment integration is not built yet.
                paystack_secret_key: optional_env("PAYSTACK_SECRET_KEY"),
                flutterwave_secret_key: optional_env("FLUTTERWAVE_SECRET_KEY"),
            },
        })
    }

    // Convert APP_HOST and APP_PORT into a SocketAddr.
    //
    // Axum/Tokio needs a SocketAddr to bind the server.
    pub fn server_addr(&self) -> Result<SocketAddr, ConfigError> {
        // Combine host and port into one string.
        //
        // Example result:
        // "127.0.0.1:4000"
        let address = format!("{}:{}", self.server.host, self.server.port);

        // Try to parse the string into SocketAddr.
        //
        // If parsing fails, return a ConfigError with the bad address.
        address
            .parse()
            .map_err(|_| ConfigError::InvalidSocketAddress(address))
    }
}

// Read a required environment variable.
//
// If the value is missing or empty, return a ConfigError.
fn required_env(key: &'static str) -> Result<String, ConfigError> {
    optional_env(key).ok_or(ConfigError::MissingVariable(key))
}

// Read an optional environment variable.
//
// This returns:
// - Some(value) when the variable exists and is not empty
// - None when it is missing or empty
fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        // Convert Result<String, VarError> into Option<String>.
        //
        // Ok(value) becomes Some(value).
        // Err(_) becomes None.
        .ok()
        // Remove whitespace from both ends of the value.
        .map(|value| value.trim().to_owned())
        // Treat empty strings as missing config.
        .filter(|value| !value.is_empty())
}
