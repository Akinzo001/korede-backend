use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to_email: String,
    pub to_name: Option<String>,
    pub subject: String,
    pub text_body: String,
    pub html_body: Option<String>,
}

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("email provider is disabled")]
    Disabled,

    #[error("missing email configuration: {0}")]
    MissingConfig(&'static str),

    #[error("email provider rejected the request: {0}")]
    Provider(String),

    #[error("failed to send email")]
    SendFailed,
}

#[async_trait]
pub trait EmailService: Send + Sync {
    async fn send(&self, message: EmailMessage) -> Result<(), EmailError>;
}
