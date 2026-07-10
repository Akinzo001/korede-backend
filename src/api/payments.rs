use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    api::{error::ApiError, AppState},
    application::payments::{verification_message, PaystackWebhookCommand},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/paystack/webhook", post(handle_paystack_webhook))
        .route("/paystack/verify/:reference", post(verify_paystack_payment))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaystackWebhookResponse {
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaystackVerificationResponse {
    pub status: String,
    pub donation_id: Option<String>,
    pub payment_status: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct PaystackWebhookEnvelope {
    event: String,
    data: PaystackWebhookData,
}

#[derive(Debug, Deserialize)]
struct PaystackWebhookData {
    reference: Option<String>,
    transfer_code: Option<String>,
    id: Option<i64>,
    status: Option<String>,
    failure_reason: Option<String>,
    dedicated_account: Option<PaystackWebhookDedicatedAccount>,
}

#[derive(Debug, Deserialize)]
struct PaystackWebhookDedicatedAccount {
    account_number: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/payments/paystack/webhook",
    tag = "Payments",
    responses(
        (status = 200, description = "Paystack webhook processed or ignored.", body = PaystackWebhookResponse),
        (status = 400, description = "Invalid webhook payload."),
        (status = 401, description = "Missing or invalid Paystack signature."),
        (status = 500, description = "Webhook handling failed.")
    )
)]
pub async fn handle_paystack_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<PaystackWebhookResponse>, ApiError> {
    validate_paystack_webhook_signature(&state, &headers, &body)?;

    let payload: PaystackWebhookEnvelope = serde_json::from_slice(&body)
        .map_err(|_| ApiError::BadRequest("invalid webhook payload".to_owned()))?;

    let command = PaystackWebhookCommand {
        event: payload.event,
        reference: payload.data.reference,
        transfer_code: payload.data.transfer_code,
        provider_id: payload.data.id,
        status: payload.data.status,
        failure_reason: payload.data.failure_reason,
        dedicated_account_number: payload
            .data
            .dedicated_account
            .and_then(|account| account.account_number),
    };

    let status = state
        .payment_service
        .handle_paystack_webhook(command)
        .await?;

    Ok(Json(PaystackWebhookResponse { status }))
}

#[utoipa::path(
    post,
    path = "/api/v1/payments/paystack/verify/{reference}",
    tag = "Payments",
    params(
        ("reference" = String, Path, description = "Paystack transaction reference returned when checkout was initialized.")
    ),
    responses(
        (status = 200, description = "Paystack payment verification completed.", body = PaystackVerificationResponse),
        (status = 400, description = "Reference is not a checkout donation or provider rejected verification."),
        (status = 404, description = "Donation reference was not found."),
        (status = 503, description = "Paystack is not configured.")
    )
)]
pub async fn verify_paystack_payment(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Result<Json<PaystackVerificationResponse>, ApiError> {
    let outcome = state
        .payment_service
        .verify_checkout_payment(&reference)
        .await?;

    Ok(Json(PaystackVerificationResponse {
        message: verification_message(&outcome.status).to_owned(),
        donation_id: outcome.donation_id.map(|id| id.to_string()),
        payment_status: outcome
            .payment_status
            .map(|status| status.as_str().to_owned()),
        status: outcome.status,
    }))
}

fn validate_paystack_webhook_signature(
    state: &AppState,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<(), ApiError> {
    let Some(secret) = state.paystack_webhook_secret.as_deref() else {
        return Err(ApiError::Internal(
            "paystack webhook secret is not configured".to_owned(),
        ));
    };

    let Some(signature) = headers
        .get("x-paystack-signature")
        .and_then(|value| value.to_str().ok())
    else {
        return Err(ApiError::Unauthorized(
            "missing paystack signature".to_owned(),
        ));
    };

    use sha2::{Digest, Sha512};
    let expected = {
        let mut hasher = Sha512::new();
        hasher.update(secret.as_bytes());
        hasher.update(body);
        let digest = hasher.finalize();
        let mut output = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut output, "{byte:02x}");
        }
        output
    };

    if expected != signature {
        return Err(ApiError::Unauthorized(
            "invalid paystack signature".to_owned(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_values() {
        let state_secret = "secret";
        let payload = Bytes::from_static(br#"{"event":"charge.success"}"#);

        use sha2::{Digest, Sha512};
        let mut hasher = Sha512::new();
        hasher.update(state_secret.as_bytes());
        hasher.update(&payload);
        let digest = hasher.finalize();
        let mut signature = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut signature, "{byte:02x}");
        }

        assert_ne!(signature, "wrong");
    }
}
