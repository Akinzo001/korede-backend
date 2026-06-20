use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    infrastructure::config::PaymentConfig,
    port::payment::{
        PaymentGateway, PaymentGatewayError, PaymentInitialization, PaymentInitializationRequest,
        PaymentVerification, PaymentVerificationStatus,
    },
};

#[derive(Debug, Clone)]
pub struct PaystackPaymentGateway {
    client: Client,
    secret_key: String,
    preferred_bank: String,
    country: String,
}

#[derive(Debug, Serialize)]
struct PaystackCreateCustomerRequest<'a> {
    email: &'a str,
    first_name: &'a str,
    last_name: &'a str,
    metadata: PaystackMetadata<'a>,
}

#[derive(Debug, Serialize)]
struct PaystackMetadata<'a> {
    donor_name: &'a str,
    case_public_slug: &'a str,
    case_title: &'a str,
    payment_label: &'a str,
    donation_reference: &'a str,
}

#[derive(Debug, Serialize)]
struct PaystackDedicatedAccountRequest<'a> {
    customer: &'a str,
    preferred_bank: &'a str,
    country: &'a str,
    first_name: &'a str,
    last_name: &'a str,
}

#[derive(Debug, Deserialize)]
struct PaystackEnvelope<T> {
    status: bool,
    message: String,
    data: T,
}

#[derive(Debug, Deserialize)]
struct PaystackCustomerResponse {
    customer_code: String,
}

#[derive(Debug, Deserialize)]
struct PaystackDedicatedAccountResponse {
    id: i64,
    account_name: String,
    account_number: String,
    bank: PaystackBank,
    customer: Option<PaystackCustomerResponse>,
}

#[derive(Debug, Deserialize)]
struct PaystackBank {
    name: String,
    slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PaystackVerifyResponse {
    amount: i64,
    reference: String,
    status: String,
    paid_at: Option<String>,
}

impl PaystackPaymentGateway {
    pub fn from_config(config: &PaymentConfig) -> Result<Self, PaymentGatewayError> {
        let secret_key = config
            .paystack_secret_key
            .clone()
            .ok_or(PaymentGatewayError::MissingConfig("PAYSTACK_SECRET_KEY"))?;

        Ok(Self {
            client: Client::new(),
            secret_key,
            preferred_bank: config.paystack_dva_preferred_bank.clone(),
            country: config.paystack_dva_country.clone(),
        })
    }

    async fn ensure_customer_code(
        &self,
        request: &PaymentInitializationRequest,
        first_name: &str,
        last_name: &str,
    ) -> Result<String, PaymentGatewayError> {
        let payload = PaystackCreateCustomerRequest {
            email: &request.donor_email,
            first_name,
            last_name,
            metadata: PaystackMetadata {
                donor_name: &request.donor_display_name,
                case_public_slug: &request.case_public_slug,
                case_title: &request.case_title,
                payment_label: &request.payment_label,
                donation_reference: &request.reference,
            },
        };

        let response = self
            .client
            .post("https://api.paystack.co/customer")
            .bearer_auth(&self.secret_key)
            .json(&payload)
            .send()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if response.status().is_success() {
            let envelope: PaystackEnvelope<PaystackCustomerResponse> = response
                .json()
                .await
                .map_err(|_| PaymentGatewayError::RequestFailed)?;

            if !envelope.status {
                return Err(PaymentGatewayError::Provider(envelope.message));
            }

            return Ok(envelope.data.customer_code);
        }

        let fallback = self
            .client
            .get(format!(
                "https://api.paystack.co/customer/{}",
                request.donor_email
            ))
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if !fallback.status().is_success() {
            return Err(PaymentGatewayError::Provider(
                fallback.text().await.unwrap_or_default(),
            ));
        }

        let envelope: PaystackEnvelope<PaystackCustomerResponse> = fallback
            .json()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if !envelope.status {
            return Err(PaymentGatewayError::Provider(envelope.message));
        }

        Ok(envelope.data.customer_code)
    }
}

#[async_trait]
impl PaymentGateway for PaystackPaymentGateway {
    async fn initialize_payment(
        &self,
        request: PaymentInitializationRequest,
    ) -> Result<PaymentInitialization, PaymentGatewayError> {
        let (first_name, last_name) = split_payment_label(&request.payment_label);
        let customer_code = self
            .ensure_customer_code(&request, &first_name, &last_name)
            .await?;
        let payload = PaystackDedicatedAccountRequest {
            customer: &customer_code,
            preferred_bank: &self.preferred_bank,
            country: &self.country,
            first_name: &first_name,
            last_name: &last_name,
        };

        let response = self
            .client
            .post("https://api.paystack.co/dedicated_account")
            .bearer_auth(&self.secret_key)
            .json(&payload)
            .send()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(PaymentGatewayError::Provider(
                response.text().await.unwrap_or_default(),
            ));
        }

        let envelope: PaystackEnvelope<PaystackDedicatedAccountResponse> = response
            .json()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if !envelope.status {
            return Err(PaymentGatewayError::Provider(envelope.message));
        }

        Ok(PaymentInitialization {
            provider_reference: request.reference,
            customer_code: envelope
                .data
                .customer
                .map(|customer| customer.customer_code)
                .or(Some(customer_code)),
            dedicated_account_id: Some(envelope.data.id),
            bank_name: envelope.data.bank.name,
            bank_slug: envelope.data.bank.slug,
            account_name: envelope.data.account_name,
            account_number: envelope.data.account_number,
        })
    }

    async fn verify_payment(
        &self,
        reference: &str,
    ) -> Result<PaymentVerification, PaymentGatewayError> {
        let response = self
            .client
            .get(format!(
                "https://api.paystack.co/transaction/verify/{reference}"
            ))
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(PaymentGatewayError::Provider(
                response.text().await.unwrap_or_default(),
            ));
        }

        let envelope: PaystackEnvelope<PaystackVerifyResponse> = response
            .json()
            .await
            .map_err(|_| PaymentGatewayError::RequestFailed)?;

        if !envelope.status {
            return Err(PaymentGatewayError::Provider(envelope.message));
        }

        let status = match envelope.data.status.as_str() {
            "success" => PaymentVerificationStatus::Success,
            "failed" => PaymentVerificationStatus::Failed,
            _ => PaymentVerificationStatus::Pending,
        };

        let paid_at = envelope
            .data
            .paid_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&chrono::Utc));

        Ok(PaymentVerification {
            provider_reference: envelope.data.reference,
            amount_kobo: envelope.data.amount,
            status,
            paid_at,
        })
    }

    fn generate_reference(&self) -> String {
        format!("korede-{}", Uuid::new_v4().simple())
    }
}

fn split_payment_label(label: &str) -> (String, String) {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return ("Hospital".to_owned(), "Patient".to_owned());
    }

    if let Some((hospital_name, patient_name)) = trimmed.split_once(" - ") {
        let first_name = hospital_name.trim();
        let last_name = patient_name.trim();
        if !first_name.is_empty() && !last_name.is_empty() {
            return (first_name.to_owned(), last_name.to_owned());
        }
    }

    let mut parts = trimmed.split_whitespace();
    let first_name = parts.next().unwrap_or("Hospital").to_owned();
    let last_name = parts.collect::<Vec<_>>().join(" ");

    if last_name.is_empty() {
        (first_name, "Patient".to_owned())
    } else {
        (first_name, last_name)
    }
}

#[cfg(test)]
mod tests {
    use super::split_payment_label;

    #[test]
    fn split_payment_label_prefers_hospital_patient_format() {
        let (first_name, last_name) = split_payment_label("Lagoon Hospital - John Doe");

        assert_eq!(first_name, "Lagoon Hospital");
        assert_eq!(last_name, "John Doe");
    }

    #[test]
    fn split_payment_label_has_fallback_for_single_segment() {
        let (first_name, last_name) = split_payment_label("SingleLabel");

        assert_eq!(first_name, "SingleLabel");
        assert_eq!(last_name, "Patient");
    }
}
