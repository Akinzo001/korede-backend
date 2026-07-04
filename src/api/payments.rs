use axum::{body::Bytes, extract::State, http::HeaderMap, routing::post, Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    adapters::donation_proof_retry::next_retry_at,
    api::{error::ApiError, AppState},
    domain::donation::{DonationMethod, DonationProofStatus, DonationStatus},
    port::{
        donation::{DonationFailureUpdate, DonationPaymentUpdate, NewDonation},
        payment::PaymentVerificationStatus,
        sui::DonationProofRequest,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/paystack/webhook", post(handle_paystack_webhook))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaystackWebhookResponse {
    pub status: String,
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

    if payload.event != "charge.success" {
        return Ok(Json(PaystackWebhookResponse {
            status: "ignored".to_owned(),
        }));
    }

    if let Some(reference) = payload.data.reference.as_deref() {
        if let Some(locked) = state
            .donation_repository
            .lock_pending_donation_for_confirmation(reference)
            .await?
        {
            return finalize_checkout_donation(state, locked, reference).await;
        }
    }

    if let Some(account_number) = payload
        .data
        .dedicated_account
        .as_ref()
        .and_then(|account| account.account_number.as_deref())
    {
        if let Some(reference) = payload.data.reference.as_deref() {
            return finalize_dva_donation(state, account_number, reference).await;
        }
    }

    Ok(Json(PaystackWebhookResponse {
        status: "ignored".to_owned(),
    }))
}

async fn finalize_checkout_donation(
    state: AppState,
    locked: crate::port::donation::DonationCaseLock,
    reference: &str,
) -> Result<Json<PaystackWebhookResponse>, ApiError> {
    if locked.donation.status == DonationStatus::Paid {
        return Ok(Json(PaystackWebhookResponse {
            status: "already_processed".to_owned(),
        }));
    }

    let verification = state.payment_gateway.verify_payment(reference).await?;

    if verification.status != PaymentVerificationStatus::Success {
        state
            .donation_repository
            .mark_donation_failed(DonationFailureUpdate {
                donation_id: locked.donation.id,
                status: DonationStatus::Failed,
            })
            .await?;

        return Ok(Json(PaystackWebhookResponse {
            status: "failed".to_owned(),
        }));
    }

    if verification.amount_kobo != locked.donation.amount_kobo {
        state
            .donation_repository
            .mark_donation_failed(DonationFailureUpdate {
                donation_id: locked.donation.id,
                status: DonationStatus::Failed,
            })
            .await?;

        return Ok(Json(PaystackWebhookResponse {
            status: "amount_mismatch".to_owned(),
        }));
    }

    let now = Utc::now();
    let attempt_count = locked.donation.proof_attempt_count + 1;
    let proof_result = state
        .donation_proof_publisher
        .publish_donation_proof(DonationProofRequest {
            case_id: locked.medical_case.id.to_string(),
            hospital_id: locked.medical_case.hospital_id.to_string(),
            amount_kobo: locked.donation.amount_kobo as u64,
            payment_reference: locked.donation.paystack_reference.clone(),
        })
        .await;

    let (
        proof_status,
        sui_network,
        sui_tx_digest,
        proof_next_retry_at,
        proof_last_error,
        proof_published_at,
    ) = match proof_result {
        Ok(receipt) => (
            DonationProofStatus::Published,
            Some(receipt.network),
            Some(receipt.tx_digest),
            None,
            None,
            Some(now),
        ),
        Err(error) => {
            tracing::error!(%error, "failed to publish donation proof");
            (
                DonationProofStatus::PendingRetry,
                None,
                None,
                next_retry_at(attempt_count, now),
                Some(error.to_string()),
                None,
            )
        }
    };

    match state
        .donation_repository
        .mark_donation_paid(DonationPaymentUpdate {
            donation_id: locked.donation.id,
            paystack_transaction_reference: verification.provider_reference,
            paid_at: verification.paid_at.unwrap_or(now),
            proof_status,
            sui_network,
            sui_tx_digest,
            proof_attempt_count: attempt_count,
            proof_last_attempt_at: Some(now),
            proof_next_retry_at,
            proof_last_error,
            proof_published_at,
        })
        .await
    {
        Ok(donation) => {
            maybe_close_case_dva(
                &state,
                locked.medical_case.id,
                locked.remaining_amount_kobo - donation.amount_kobo,
            )
            .await?;
            Ok(Json(PaystackWebhookResponse {
                status: "processed".to_owned(),
            }))
        }
        Err(crate::port::donation::DonationRepositoryError::AmountExceedsRemaining) => {
            Ok(Json(PaystackWebhookResponse {
                status: "overflow_rejected".to_owned(),
            }))
        }
        Err(error) => Err(error.into()),
    }
}

async fn finalize_dva_donation(
    state: AppState,
    account_number: &str,
    reference: &str,
) -> Result<Json<PaystackWebhookResponse>, ApiError> {
    if let Some(existing) = state
        .donation_repository
        .find_donation_by_reference(reference)
        .await?
    {
        if let Some(status) = existing_dva_webhook_status(&existing) {
            return Ok(Json(PaystackWebhookResponse {
                status: status.to_owned(),
            }));
        }
    }

    let Some(case_dva) = state
        .donation_repository
        .find_case_dva_by_account_number(account_number)
        .await?
    else {
        return Ok(Json(PaystackWebhookResponse {
            status: "ignored".to_owned(),
        }));
    };

    let public_case = state
        .donation_repository
        .get_public_case_details_for_case_id(case_dva.medical_case_id)
        .await?;

    let Some(public_case) = public_case else {
        return Ok(Json(PaystackWebhookResponse {
            status: "ignored".to_owned(),
        }));
    };

    let verification = state.payment_gateway.verify_payment(reference).await?;

    if verification.status != PaymentVerificationStatus::Success {
        return Ok(Json(PaystackWebhookResponse {
            status: "failed".to_owned(),
        }));
    }

    let donation = match state
        .donation_repository
        .create_paid_donation(
            NewDonation {
                medical_case_id: public_case.medical_case.id,
                donor_display_name: "Anonymous".to_owned(),
                donor_email: "anonymous@korede.local".to_owned(),
                amount_kobo: verification.amount_kobo,
                method: DonationMethod::DvaTransfer,
                paystack_reference: reference.to_owned(),
                paystack_transaction_reference: Some(verification.provider_reference.clone()),
                paystack_access_code: None,
                paystack_authorization_url: None,
                paystack_customer_code: case_dva.paystack_customer_code.clone(),
                paystack_dedicated_account_id: Some(case_dva.paystack_dedicated_account_id),
                paystack_dedicated_account_number: Some(case_dva.account_number.clone()),
                paystack_dedicated_account_name: Some(case_dva.account_name.clone()),
                paystack_dedicated_bank_name: Some(case_dva.bank_name.clone()),
                paystack_dedicated_bank_slug: case_dva.bank_slug.clone(),
            },
            verification.paid_at.unwrap_or_else(Utc::now),
        )
        .await
    {
        Ok(donation) => donation,
        Err(crate::port::donation::DonationRepositoryError::AmountExceedsRemaining) => {
            return Ok(Json(PaystackWebhookResponse {
                status: "overflow_rejected".to_owned(),
            }));
        }
        Err(error) => return Err(error.into()),
    };

    let now = Utc::now();
    let proof_result = state
        .donation_proof_publisher
        .publish_donation_proof(DonationProofRequest {
            case_id: public_case.medical_case.id.to_string(),
            hospital_id: public_case.medical_case.hospital_id.to_string(),
            amount_kobo: donation.amount_kobo as u64,
            payment_reference: donation.paystack_reference.clone(),
        })
        .await;

    let attempt_count = donation.proof_attempt_count + 1;
    let (
        proof_status,
        sui_network,
        sui_tx_digest,
        proof_next_retry_at,
        proof_last_error,
        proof_published_at,
    ) = match proof_result {
        Ok(receipt) => (
            DonationProofStatus::Published,
            Some(receipt.network),
            Some(receipt.tx_digest),
            None,
            None,
            Some(now),
        ),
        Err(error) => (
            DonationProofStatus::PendingRetry,
            None,
            None,
            next_retry_at(attempt_count, now),
            Some(error.to_string()),
            None,
        ),
    };

    let _ = state
        .donation_repository
        .update_donation_proof(crate::port::donation::DonationProofAttemptUpdate {
            donation_id: donation.id,
            proof_status,
            sui_network,
            sui_tx_digest,
            proof_attempt_count: attempt_count,
            proof_last_attempt_at: now,
            proof_next_retry_at,
            proof_last_error,
            proof_published_at,
        })
        .await?;

    let remaining_amount_after = remaining_amount_after_dva_confirmation(
        public_case.medical_case.bill_amount_kobo,
        public_case.medical_case.amount_raised_kobo,
        donation.amount_kobo,
    );
    maybe_close_case_dva(&state, public_case.medical_case.id, remaining_amount_after).await?;

    Ok(Json(PaystackWebhookResponse {
        status: "processed".to_owned(),
    }))
}

async fn maybe_close_case_dva(
    state: &AppState,
    medical_case_id: uuid::Uuid,
    remaining_amount_after: i64,
) -> Result<(), ApiError> {
    if remaining_amount_after > 0 {
        return Ok(());
    }

    let Some(case_dva) = state
        .donation_repository
        .find_case_dva(medical_case_id)
        .await?
    else {
        return Ok(());
    };

    if !case_dva.is_active {
        return Ok(());
    }

    let deactivation_result = state
        .payment_gateway
        .deactivate_dva(case_dva.paystack_dedicated_account_id)
        .await;

    match deactivation_result {
        Ok(()) => {
            let _ = state
                .donation_repository
                .deactivate_case_dva(medical_case_id, None)
                .await?;
        }
        Err(error) => {
            let _ = state
                .donation_repository
                .deactivate_case_dva(medical_case_id, Some(error.to_string()))
                .await?;
        }
    }

    Ok(())
}

fn existing_dva_webhook_status(
    existing: &crate::domain::donation::Donation,
) -> Option<&'static str> {
    match existing.status {
        DonationStatus::Paid => Some("already_processed"),
        DonationStatus::Pending | DonationStatus::Failed | DonationStatus::RejectedOverflow => {
            Some("ignored")
        }
    }
}

fn remaining_amount_after_dva_confirmation(
    bill_amount_kobo: i64,
    amount_raised_kobo: i64,
    donation_amount_kobo: i64,
) -> i64 {
    (bill_amount_kobo - (amount_raised_kobo + donation_amount_kobo)).max(0)
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
    use crate::domain::donation::{Donation, DonationMethod, DonationProofStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn donation_with_status(status: DonationStatus) -> Donation {
        let now = Utc::now();
        Donation {
            id: Uuid::new_v4(),
            medical_case_id: Uuid::new_v4(),
            donor_display_name: "Anonymous".to_owned(),
            donor_email: "anonymous@korede.local".to_owned(),
            amount_kobo: 5_000,
            method: DonationMethod::DvaTransfer,
            paystack_reference: "ref-123".to_owned(),
            paystack_transaction_reference: Some("txn-123".to_owned()),
            paystack_access_code: None,
            paystack_authorization_url: None,
            paystack_customer_code: None,
            paystack_dedicated_account_id: Some(10),
            paystack_dedicated_account_number: Some("1234567890".to_owned()),
            paystack_dedicated_account_name: Some("Hospital - Patient".to_owned()),
            paystack_dedicated_bank_name: Some("Test Bank".to_owned()),
            paystack_dedicated_bank_slug: Some("test-bank".to_owned()),
            status,
            paid_at: Some(now),
            proof_status: DonationProofStatus::Pending,
            sui_network: None,
            sui_tx_digest: None,
            proof_attempt_count: 0,
            proof_last_attempt_at: None,
            proof_next_retry_at: None,
            proof_last_error: None,
            proof_published_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn existing_dva_webhook_status_is_idempotent_for_paid_donation() {
        let donation = donation_with_status(DonationStatus::Paid);
        assert_eq!(
            existing_dva_webhook_status(&donation),
            Some("already_processed")
        );
    }

    #[test]
    fn existing_dva_webhook_status_ignores_non_paid_donation() {
        assert_eq!(
            existing_dva_webhook_status(&donation_with_status(DonationStatus::Pending)),
            Some("ignored")
        );
        assert_eq!(
            existing_dva_webhook_status(&donation_with_status(DonationStatus::Failed)),
            Some("ignored")
        );
        assert_eq!(
            existing_dva_webhook_status(&donation_with_status(DonationStatus::RejectedOverflow)),
            Some("ignored")
        );
    }

    #[test]
    fn remaining_amount_after_dva_confirmation_clamps_at_zero() {
        assert_eq!(
            remaining_amount_after_dva_confirmation(10_000, 9_000, 500),
            500
        );
        assert_eq!(
            remaining_amount_after_dva_confirmation(10_000, 9_000, 1_500),
            0
        );
    }
}
