// `std` is Rust's standard library.
//
// `env` lets us read environment variables such as DATABASE_URL.
// `SocketAddr` represents a network address like 127.0.0.1:4000.
use std::{env, net::SocketAddr};

// `thiserror::Error` lets us create clean custom error types
// without manually writing a lot of error-handling boilerplate.
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub admin: AdminConfig,
    pub sui: SuiConfig,
    pub payments: PaymentConfig,
    pub storage: StorageConfig,
    pub email: EmailConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expires_in_seconds: i64,
    pub refresh_token_expires_in_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct AdminConfig {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub provider: String,
    pub local_root: String,
    pub max_upload_bytes: usize,
    pub backblaze: BackblazeConfig,
}

#[derive(Debug, Clone)]
pub struct BackblazeConfig {
    pub bucket: Option<String>,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub region: String,
}

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub provider: String,
    pub from_email: Option<String>,
    pub from_name: Option<String>,
    pub brevo: BrevoConfig,
    pub resend: ResendConfig,
}

#[derive(Debug, Clone)]
pub struct BrevoConfig {
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResendConfig {
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SuiConfig {
    pub cli_path: String,
    pub network: String,
    pub rpc_url: String,
    pub package_id: Option<String>,
    pub admin_address: Option<String>,
    pub keystore_path: Option<String>,
    pub client_config_path: Option<String>,
    pub gas_budget: u64,
    pub clock_object_id: String,
    pub request_timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct PaymentConfig {
    pub base_url: String,
    pub app_name: String,
    pub paystack_secret_key: Option<String>,
    pub paystack_webhook_secret: Option<String>,
    pub paystack_dva_preferred_bank: String,
    pub paystack_dva_country: String,
    pub flutterwave_secret_key: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingVariable(&'static str),

    #[error("APP_PORT or PORT must be a valid port number, got: {0}")]
    InvalidPort(String),

    #[error("APP_HOST and APP_PORT/PORT must form a valid socket address, got: {0}")]
    InvalidSocketAddress(String),

    #[error("{key} must be a valid number, got: {value}")]
    InvalidNumber { key: &'static str, value: String },

    #[error("unsupported storage provider: {0}")]
    UnsupportedStorageProvider(String),

    #[error("unsupported email provider: {0}")]
    UnsupportedEmailProvider(String),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        Ok(Self {
            server: ServerConfig {
                host: optional_env("APP_HOST").unwrap_or_else(|| "0.0.0.0".to_owned()),
                port: {
                    let raw_port = optional_env("APP_PORT")
                        .or_else(|| optional_env("PORT"))
                        .unwrap_or_else(|| "4000".to_owned());

                    raw_port
                        .parse()
                        .map_err(|_| ConfigError::InvalidPort(raw_port))?
                },
            },
            database: DatabaseConfig {
                url: required_env("DATABASE_URL")?,
            },
            auth: AuthConfig {
                jwt_secret: required_env("JWT_SECRET")?,
                jwt_expires_in_seconds: parse_i64_env("JWT_EXPIRES_IN_SECONDS", "86400")?,
                refresh_token_expires_in_seconds: parse_i64_env(
                    "REFRESH_TOKEN_EXPIRES_IN_SECONDS",
                    "2592000",
                )?,
            },
            admin: AdminConfig {
                email: required_env("SUPER_ADMIN_EMAIL")?.to_lowercase(),
                password: required_env("SUPER_ADMIN_PASSWORD")?,
            },
            sui: SuiConfig {
                cli_path: optional_env("SUI_CLI_PATH").unwrap_or_else(|| "sui".to_owned()),
                network: optional_env("SUI_NETWORK").unwrap_or_else(|| "testnet".to_owned()),
                rpc_url: optional_env("SUI_RPC_URL")
                    .unwrap_or_else(|| "https://sui-testnet.grpc.ankr.com:443".to_owned()),
                package_id: optional_env("SUI_PACKAGE_ID"),
                admin_address: optional_env("SUI_ADMIN_ADDRESS"),
                keystore_path: optional_env("SUI_KEYSTORE_PATH"),
                client_config_path: optional_env("SUI_CLIENT_CONFIG_PATH"),
                gas_budget: parse_u64_env("SUI_GAS_BUDGET", "10000000")?,
                clock_object_id: optional_env("SUI_CLOCK_OBJECT_ID")
                    .unwrap_or_else(|| "0x6".to_owned()),
                request_timeout_seconds: parse_u64_env("SUI_REQUEST_TIMEOUT_SECONDS", "30")?,
            },
            payments: PaymentConfig {
                base_url: optional_env("APP_BASE_URL")
                    .unwrap_or_else(|| "https://korede-health.akinzo.buzz".to_owned()),
                app_name: optional_env("APP_NAME").unwrap_or_else(|| "Korede Health".to_owned()),
                paystack_secret_key: optional_env("PAYSTACK_SECRET_KEY"),
                paystack_webhook_secret: optional_env("PAYSTACK_WEBHOOK_SECRET"),
                paystack_dva_preferred_bank: optional_env("PAYSTACK_DVA_PREFERRED_BANK")
                    .unwrap_or_else(|| "test-bank".to_owned()),
                paystack_dva_country: optional_env("PAYSTACK_DVA_COUNTRY")
                    .unwrap_or_else(|| "NG".to_owned()),
                flutterwave_secret_key: optional_env("FLUTTERWAVE_SECRET_KEY"),
            },
            storage: {
                let provider = optional_env("STORAGE_PROVIDER")
                    .unwrap_or_else(|| "local".to_owned())
                    .to_ascii_lowercase();

                if provider != "local" && provider != "backblaze" {
                    return Err(ConfigError::UnsupportedStorageProvider(provider));
                }

                StorageConfig {
                    provider,
                    local_root: optional_env("LOCAL_STORAGE_ROOT")
                        .unwrap_or_else(|| "storage".to_owned()),
                    max_upload_bytes: parse_usize_env("MAX_UPLOAD_BYTES", "10485760")?,
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
                let provider = optional_env("EMAIL_PROVIDER")
                    .unwrap_or_else(|| "disabled".to_owned())
                    .to_ascii_lowercase();

                if provider != "disabled" && provider != "brevo" && provider != "resend" {
                    return Err(ConfigError::UnsupportedEmailProvider(provider));
                }

                EmailConfig {
                    provider,
                    from_email: optional_env("EMAIL_FROM_ADDRESS"),
                    from_name: optional_env("EMAIL_FROM_NAME"),
                    brevo: BrevoConfig {
                        api_key: optional_env("BREVO_API_KEY"),
                    },
                    resend: ResendConfig {
                        api_key: optional_env("RESEND_API_KEY"),
                    },
                }
            },
        })
    }

    pub fn server_addr(&self) -> Result<SocketAddr, ConfigError> {
        let address = format!("{}:{}", self.server.host, self.server.port);
        address
            .parse()
            .map_err(|_| ConfigError::InvalidSocketAddress(address))
    }
}

fn required_env(key: &'static str) -> Result<String, ConfigError> {
    optional_env(key).ok_or(ConfigError::MissingVariable(key))
}

fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_i64_env(key: &'static str, default: &'static str) -> Result<i64, ConfigError> {
    let value = optional_env(key).unwrap_or_else(|| default.to_owned());
    value
        .parse()
        .map_err(|_| ConfigError::InvalidNumber { key, value })
}

fn parse_u64_env(key: &'static str, default: &'static str) -> Result<u64, ConfigError> {
    let value = optional_env(key).unwrap_or_else(|| default.to_owned());
    value
        .parse()
        .map_err(|_| ConfigError::InvalidNumber { key, value })
}

fn parse_usize_env(key: &'static str, default: &'static str) -> Result<usize, ConfigError> {
    let value = optional_env(key).unwrap_or_else(|| default.to_owned());
    value
        .parse()
        .map_err(|_| ConfigError::InvalidNumber { key, value })
}
