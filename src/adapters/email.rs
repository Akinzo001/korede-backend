use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

use crate::{
    infrastructure::config::{BrevoConfig, EmailConfig},
    port::email::{EmailError, EmailMessage, EmailService},
};

#[derive(Debug, Clone)]
pub struct DisabledEmailService;

#[derive(Debug, Clone)]
pub struct BrevoEmailService {
    client: Client,
    api_key: String,
    from_email: String,
    from_name: String,
}

#[derive(Debug, Serialize)]
struct BrevoSendEmailRequest {
    sender: BrevoContact,
    to: Vec<BrevoContact>,
    subject: String,
    #[serde(rename = "textContent")]
    text_content: String,
    #[serde(rename = "htmlContent", skip_serializing_if = "Option::is_none")]
    html_content: Option<String>,
}

#[derive(Debug, Serialize)]
struct BrevoContact {
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl BrevoEmailService {
    pub fn from_config(config: &EmailConfig, brevo: &BrevoConfig) -> Result<Self, EmailError> {
        let api_key = brevo
            .api_key
            .clone()
            .ok_or(EmailError::MissingConfig("BREVO_API_KEY"))?;

        let from_email = config
            .from_email
            .clone()
            .ok_or(EmailError::MissingConfig("EMAIL_FROM_ADDRESS"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            from_email,
            from_name: config
                .from_name
                .clone()
                .unwrap_or_else(|| "Korede Health".to_owned()),
        })
    }
}

#[async_trait]
impl EmailService for DisabledEmailService {
    async fn send(&self, _message: EmailMessage) -> Result<(), EmailError> {
        Ok(())
    }
}

#[async_trait]
impl EmailService for BrevoEmailService {
    async fn send(&self, message: EmailMessage) -> Result<(), EmailError> {
        let request = BrevoSendEmailRequest {
            sender: BrevoContact {
                email: self.from_email.clone(),
                name: Some(self.from_name.clone()),
            },
            to: vec![BrevoContact {
                email: message.to_email,
                name: message.to_name,
            }],
            subject: message.subject,
            text_content: message.text_body,
            html_content: message.html_body,
        };

        let response = self
            .client
            .post("https://api.brevo.com/v3/smtp/email")
            .header("api-key", &self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(%error, "failed to call Brevo email API");
                EmailError::SendFailed
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(%status, %body, "Brevo email API rejected request");

            return Err(EmailError::Provider(format!(
                "Brevo returned status {status}"
            )));
        }

        Ok(())
    }
}
