use axum::{body::Bytes, extract::State, http::HeaderMap, routing::post, Json, Router};
use chrono::Utc;
use serde::Deserialize;
use crate::{
    api::{error::ApiError, AppState},
    domain::donation::{DonationProofStatus, DonationStatus},
    port::{
        donation::{DonationFailureUpdate, DonationPaymentUpdate},
        payment::PaymentVerificationStatus,
        sui::DonationProofRequest,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/paystack/webhook", post(handle_paystack_webhook))
}

#[derive(Debug, Deserialize)]
struct PaystackWebhookEnvelope {
    event: String,
    data: PaystackWebhookData,
}

#[derive(Debug, Deserialize)]
struct PaystackWebhookData {
    reference: Option<String>,
    dedicated_account: Option<PaystackWebhookDedicatedAccount>,
}

#[derive(Debug, Deserialize)]
struct PaystackWebhookDedicatedAccount {
    account_number: Option<String>,
}

pub async fn handle_paystack_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_paystack_webhook_signature(&state, &headers, &body)?;

    let payload: PaystackWebhookEnvelope = serde_json::from_slice(&body)
        .map_err(|_| ApiError::BadRequest("invalid webhook payload".to_owned()))?;

    if payload.event != "charge.success" {
        return Ok(Json(serde_json::json!({ "status": "ignored" })));
    }

    let locked = if let Some(reference) = payload.data.reference.as_deref() {
        state
            .donation_repository
            .lock_pending_donation_for_confirmation(reference)
            .await?
    } else if let Some(account_number) = payload
        .data
        .dedicated_account
        .as_ref()
        .and_then(|account| account.account_number.as_deref())
    {
        state
            .donation_repository
            .lock_pending_donation_by_account_number(account_number)
            .await?
    } else {
        None
    };

    let Some(locked) = locked else {
        return Ok(Json(serde_json::json!({ "status": "ignored" })));
    };

    if locked.donation.status == DonationStatus::Paid {
        return Ok(Json(serde_json::json!({ "status": "already_processed" })));
    }

    let verification = state
        .payment_gateway
        .verify_payment(
            payload
                .data
                .reference
                .as_deref()
                .unwrap_or(&locked.donation.paystack_reference),
        )
        .await?;

    if verification.status != PaymentVerificationStatus::Success {
        state
            .donation_repository
            .mark_donation_failed(DonationFailureUpdate {
                donation_id: locked.donation.id,
                status: DonationStatus::Failed,
            })
            .await?;

        return Ok(Json(serde_json::json!({ "status": "failed" })));
    }

    if verification.amount_kobo != locked.donation.amount_kobo {
        state
            .donation_repository
            .mark_donation_failed(DonationFailureUpdate {
                donation_id: locked.donation.id,
                status: DonationStatus::Failed,
            })
            .await?;

        return Ok(Json(serde_json::json!({ "status": "amount_mismatch" })));
    }

    let proof_result = state
        .donation_proof_publisher
        .publish_donation_proof(DonationProofRequest {
            case_id: locked.medical_case.id.to_string(),
            hospital_id: locked.medical_case.hospital_id.to_string(),
            amount_kobo: locked.donation.amount_kobo as u64,
            payment_reference: locked.donation.paystack_reference.clone(),
        })
        .await;

    let (proof_status, sui_network, sui_tx_digest) = match proof_result {
        Ok(receipt) => (
            DonationProofStatus::Published,
            Some(receipt.network),
            Some(receipt.tx_digest),
        ),
        Err(error) => {
            tracing::error!(%error, "failed to publish donation proof");
            (DonationProofStatus::Failed, None, None)
        }
    };

    match state
        .donation_repository
        .mark_donation_paid(DonationPaymentUpdate {
            donation_id: locked.donation.id,
            paystack_transaction_reference: verification.provider_reference,
            paid_at: verification.paid_at.unwrap_or_else(Utc::now),
            proof_status,
            sui_network,
            sui_tx_digest,
        })
        .await
    {
        Ok(_) => Ok(Json(serde_json::json!({ "status": "processed" }))),
        Err(crate::port::donation::DonationRepositoryError::AmountExceedsRemaining) => {
            Ok(Json(serde_json::json!({ "status": "overflow_rejected" })))
        }
        Err(error) => Err(error.into()),
    }
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
