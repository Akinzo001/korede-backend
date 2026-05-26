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

    // Super-admin credentials for platform administration.
    pub admin: AdminConfig,

    // Sui-related settings, like RPC URL and package ID.
    pub sui: SuiConfig,

    // Payment provider settings, like Paystack or Flutterwave keys.
    pub payments: PaymentConfig,

    // Upload/storage settings for KYC documents.
    pub storage: StorageConfig,

    // Email settings for transactional notifications.
    pub email: EmailConfig,
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

    // Number of seconds before issued JWTs expire.
    pub jwt_expires_in_seconds: i64,
}

// Settings for the platform super-admin account.
#[derive(Debug, Clone)]
pub struct AdminConfig {
    // Super-admin login email.
    pub email: String,

    // Super-admin login password.
    pub password: String,
}

// Settings for document storage.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    // Storage provider selected for uploads.
    //
    // This phase implements local storage. S3 can be added later.
    pub provider: String,

    // Local directory where uploaded documents are stored.
    pub local_root: String,

    // Maximum accepted upload size in bytes.
    pub max_upload_bytes: usize,

    // Backblaze B2 settings for the S3-compatible storage adapter.
    pub backblaze: BackblazeConfig,
}

// Settings for Backblaze B2 S3-compatible object storage.
#[derive(Debug, Clone)]
pub struct BackblazeConfig {
    // Backblaze bucket where KYC documents will be stored.
    pub bucket: Option<String>,

    // S3-compatible Backblaze endpoint.
    //
    // Example:
    // https://s3.eu-central-003.backblazeb2.com
    pub endpoint: Option<String>,

    // Backblaze application key ID.
    pub access_key_id: Option<String>,

    // Backblaze application key.
    pub secret_access_key: Option<String>,

    // Backblaze S3-compatible region.
    pub region: String,
}

// Settings for outbound transactional email.
#[derive(Debug, Clone)]
pub struct EmailConfig {
    // Email provider selected at runtime.
    //
    // Supported values:
    // - disabled
    // - brevo
    pub provider: String,

    // Email address used as the sender.
    pub from_email: Option<String>,

    // Display name used as the sender.
    pub from_name: Option<String>,

    // Brevo-specific settings.
    pub brevo: BrevoConfig,
}

// Settings for Brevo's transactional email API.
#[derive(Debug, Clone)]
pub struct BrevoConfig {
    // Brevo SMTP/API key.
    pub api_key: Option<String>,
}

// Settings for Sui.
#[derive(Debug, Clone)]
pub struct SuiConfig {
    // Sui network label.
    pub network: String,

    // The RPC endpoint the backend will use when talking to Sui.
    pub rpc_url: String,

    // Published Sui package ID.
    pub package_id: Option<String>,

    // Platform/admin Sui address.
    pub admin_address: Option<String>,

    // Path to Sui keystore for future backend transaction signing.
    pub keystore_path: Option<String>,

    // Default gas budget for future Sui transactions.
    pub gas_budget: u64,
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

    // This error is used when a numeric environment variable is invalid.
    #[error("{key} must be a valid number, got: {value}")]
    InvalidNumber { key: &'static str, value: String },

    // This error is used when STORAGE_PROVIDER is unknown.
    #[error("unsupported storage provider: {0}")]
    UnsupportedStorageProvider(String),

    // This error is used when EMAIL_PROVIDER is unknown.
    #[error("unsupported email provider: {0}")]
    UnsupportedEmailProvider(String),
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
                jwt_expires_in_seconds: optional_env("JWT_EXPIRES_IN_SECONDS")
                    .unwrap_or_else(|| "86400".to_owned())
                    .parse()
                    .map_err(|_| ConfigError::InvalidNumber {
                        key: "JWT_EXPIRES_IN_SECONDS",
                        value: optional_env("JWT_EXPIRES_IN_SECONDS")
                            .unwrap_or_else(|| "86400".to_owned()),
                    })?,
            },

            admin: AdminConfig {
                email: required_env("SUPER_ADMIN_EMAIL")?.to_lowercase(),
                password: required_env("SUPER_ADMIN_PASSWORD")?,
            },

            // Build the nested Sui config.
            sui: SuiConfig {
                network: optional_env("SUI_NETWORK").unwrap_or_else(|| "testnet".to_owned()),
                rpc_url: optional_env("SUI_RPC_URL")
                    .unwrap_or_else(|| "https://fullnode.testnet.sui.io:443".to_owned()),
                package_id: optional_env("SUI_PACKAGE_ID"),
                admin_address: optional_env("SUI_ADMIN_ADDRESS"),
                keystore_path: optional_env("SUI_KEYSTORE_PATH"),
                gas_budget: optional_env("SUI_GAS_BUDGET")
                    .unwrap_or_else(|| "10000000".to_owned())
                    .parse()
                    .map_err(|_| ConfigError::InvalidNumber {
                        key: "SUI_GAS_BUDGET",
                        value: optional_env("SUI_GAS_BUDGET")
                            .unwrap_or_else(|| "10000000".to_owned()),
                    })?,
            },

            // Build the nested payment config.
            payments: PaymentConfig {
                // These are optional because payment integration is not built yet.
                paystack_secret_key: optional_env("PAYSTACK_SECRET_KEY"),
                flutterwave_secret_key: optional_env("FLUTTERWAVE_SECRET_KEY"),
            },

            storage: {
                let provider =
                    optional_env("STORAGE_PROVIDER").unwrap_or_else(|| "local".to_owned());

                if provider != "local" && provider != "backblaze" {
                    return Err(ConfigError::UnsupportedStorageProvider(provider));
                }

                StorageConfig {
                    provider,
                    local_root: optional_env("LOCAL_STORAGE_ROOT")
                        .unwrap_or_else(|| "storage".to_owned()),
                    max_upload_bytes: optional_env("MAX_UPLOAD_BYTES")
                        .unwrap_or_else(|| "10485760".to_owned())
                        .parse()
                        .map_err(|_| ConfigError::InvalidNumber {
                            key: "MAX_UPLOAD_BYTES",
                            value: optional_env("MAX_UPLOAD_BYTES")
                                .unwrap_or_else(|| "10485760".to_owned()),
                        })?,
                    backblaze: BackblazeConfig {
                        bucket: optional_env("BACKBLAZE_BUCKET"),
                        endpoint: optional_env("BACKBLAZE_ENDPOINT"),
                        access_key_id: optional_env("BACKBLAZE_ACCESS_KEY_ID"),
                        secret_access_key: optional_env("BACKBLAZE_SECRET_ACCESS_KEY"),
                        region: optional_env("BACKBLAZE_REGION")
                            .unwrap_or_else(|| "eu-central-003".to_owned()),
                    },
                }
            },

            email: {
                let provider =
                    optional_env("EMAIL_PROVIDER").unwrap_or_else(|| "disabled".to_owned());

                if provider != "disabled" && provider != "brevo" {
                    return Err(ConfigError::UnsupportedEmailProvider(provider));
                }

                EmailConfig {
                    provider,
                    from_email: optional_env("EMAIL_FROM_ADDRESS"),
                    from_name: optional_env("EMAIL_FROM_NAME"),
                    brevo: BrevoConfig {
                        api_key: optional_env("BREVO_API_KEY"),
                    },
                }
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
