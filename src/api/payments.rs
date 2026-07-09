use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
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

struct PaymentConfirmationOutcome {
    status: String,
    donation_id: Option<uuid::Uuid>,
    payment_status: Option<DonationStatus>,
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
            let outcome =
                finalize_checkout_donation(state, locked, reference, PendingPaymentBehavior::Fail)
                    .await?;
            return Ok(Json(PaystackWebhookResponse {
                status: outcome.status,
            }));
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
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(ApiError::BadRequest(
            "paystack reference is required".to_owned(),
        ));
    }

    let Some(locked) = state
        .donation_repository
        .lock_pending_donation_for_confirmation(reference)
        .await?
    else {
        return Err(ApiError::NotFound("donation not found".to_owned()));
    };

    if locked.donation.method != DonationMethod::Checkout {
        return Err(ApiError::BadRequest(
            "manual verification is only supported for checkout donations".to_owned(),
        ));
    }

    let outcome = finalize_checkout_donation(
        state,
        locked,
        reference,
        PendingPaymentBehavior::LeavePending,
    )
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

#[derive(Debug, Clone, Copy)]
enum PendingPaymentBehavior {
    Fail,
    LeavePending,
}

async fn finalize_checkout_donation(
    state: AppState,
    locked: crate::port::donation::DonationCaseLock,
    reference: &str,
    pending_behavior: PendingPaymentBehavior,
) -> Result<PaymentConfirmationOutcome, ApiError> {
    if locked.donation.status == DonationStatus::Paid {
        return Ok(PaymentConfirmationOutcome {
            status: "already_processed".to_owned(),
            donation_id: Some(locked.donation.id),
            payment_status: Some(locked.donation.status),
        });
    }

    if locked.donation.status == DonationStatus::RejectedOverflow {
        return Ok(PaymentConfirmationOutcome {
            status: "overflow_rejected".to_owned(),
            donation_id: Some(locked.donation.id),
            payment_status: Some(locked.donation.status),
        });
    }

    let verification = state.payment_gateway.verify_payment(reference).await?;

    if verification.status == PaymentVerificationStatus::Pending {
        if matches!(pending_behavior, PendingPaymentBehavior::LeavePending) {
            return Ok(PaymentConfirmationOutcome {
                status: "pending".to_owned(),
                donation_id: Some(locked.donation.id),
                payment_status: Some(locked.donation.status),
            });
        }
    }

    if verification.status != PaymentVerificationStatus::Success {
        state
            .donation_repository
            .mark_donation_failed(DonationFailureUpdate {
                donation_id: locked.donation.id,
                status: DonationStatus::Failed,
            })
            .await?;

        return Ok(PaymentConfirmationOutcome {
            status: "failed".to_owned(),
            donation_id: Some(locked.donation.id),
            payment_status: Some(DonationStatus::Failed),
        });
    }

    if verification.amount_kobo != locked.donation.amount_kobo {
        state
            .donation_repository
            .mark_donation_failed(DonationFailureUpdate {
                donation_id: locked.donation.id,
                status: DonationStatus::Failed,
            })
            .await?;

        return Ok(PaymentConfirmationOutcome {
            status: "amount_mismatch".to_owned(),
            donation_id: Some(locked.donation.id),
            payment_status: Some(DonationStatus::Failed),
        });
    }

    let now = Utc::now();
    let paid_at = verification.paid_at.unwrap_or(now);
    let is_late_payment = is_late_checkout_payment(locked.donation.reservation_expires_at, paid_at);
    let payment_note = is_late_payment.then(|| {
        "Payment completed after the five-minute checkout reservation expired.".to_owned()
    });

    let payment_result = match state
        .donation_repository
        .mark_donation_paid(DonationPaymentUpdate {
            donation_id: locked.donation.id,
            paystack_transaction_reference: verification.provider_reference,
            paid_at,
            is_late_payment,
            payment_note: payment_note.clone(),
            proof_status: locked.donation.proof_status.clone(),
            sui_network: locked.donation.sui_network.clone(),
            sui_tx_digest: locked.donation.sui_tx_digest.clone(),
            proof_attempt_count: locked.donation.proof_attempt_count,
            proof_last_attempt_at: locked.donation.proof_last_attempt_at,
            proof_next_retry_at: locked.donation.proof_next_retry_at,
            proof_last_error: locked.donation.proof_last_error.clone(),
            proof_published_at: locked.donation.proof_published_at,
        })
        .await
    {
        Ok(result) => result,
        Err(crate::port::donation::DonationRepositoryError::AmountExceedsRemaining) => {
            return Ok(PaymentConfirmationOutcome {
                status: "overflow_rejected".to_owned(),
                donation_id: Some(locked.donation.id),
                payment_status: Some(DonationStatus::RejectedOverflow),
            });
        }
        Err(error) => return Err(error.into()),
    };

    if !payment_result.newly_paid {
        return Ok(PaymentConfirmationOutcome {
            status: "already_processed".to_owned(),
            donation_id: Some(payment_result.donation.id),
            payment_status: Some(payment_result.donation.status),
        });
    }

    let donation = payment_result.donation;
    let attempt_count = donation.proof_attempt_count + 1;
    let proof_result = state
        .donation_proof_publisher
        .publish_donation_proof(DonationProofRequest {
            case_id: locked.medical_case.id.to_string(),
            hospital_id: locked.medical_case.hospital_id.to_string(),
            amount_kobo: donation.amount_kobo as u64,
            payment_reference: donation.paystack_reference.clone(),
        })
        .await;

    let proof_update = match proof_result {
        Ok(receipt) => crate::port::donation::DonationProofAttemptUpdate {
            donation_id: donation.id,
            proof_status: DonationProofStatus::Published,
            sui_network: Some(receipt.network),
            sui_tx_digest: Some(receipt.tx_digest),
            proof_attempt_count: attempt_count,
            proof_last_attempt_at: now,
            proof_next_retry_at: None,
            proof_last_error: None,
            proof_published_at: Some(now),
        },
        Err(error) => {
            tracing::error!(%error, "failed to publish donation proof");
            crate::port::donation::DonationProofAttemptUpdate {
                donation_id: donation.id,
                proof_status: DonationProofStatus::PendingRetry,
                sui_network: None,
                sui_tx_digest: None,
                proof_attempt_count: attempt_count,
                proof_last_attempt_at: now,
                proof_next_retry_at: next_retry_at(attempt_count, now),
                proof_last_error: Some(error.to_string()),
                proof_published_at: None,
            }
        }
    };
    let donation = state
        .donation_repository
        .update_donation_proof(proof_update)
        .await?;

    maybe_close_case_dva(
        &state,
        locked.medical_case.id,
        locked.remaining_amount_kobo - donation.amount_kobo,
    )
    .await?;

    Ok(PaymentConfirmationOutcome {
        status: if is_late_payment {
            "processed_late".to_owned()
        } else {
            "processed".to_owned()
        },
        donation_id: Some(donation.id),
        payment_status: Some(donation.status),
    })
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
                reservation_expires_at: None,
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
        DonationStatus::Pending
        | DonationStatus::Failed
        | DonationStatus::Expired
        | DonationStatus::RejectedOverflow => Some("ignored"),
    }
}

fn remaining_amount_after_dva_confirmation(
    bill_amount_kobo: i64,
    amount_raised_kobo: i64,
    donation_amount_kobo: i64,
) -> i64 {
    (bill_amount_kobo - (amount_raised_kobo + donation_amount_kobo)).max(0)
}

fn is_late_checkout_payment(
    reservation_expires_at: Option<chrono::DateTime<Utc>>,
    paid_at: chrono::DateTime<Utc>,
) -> bool {
    reservation_expires_at.is_some_and(|expires_at| paid_at > expires_at)
}

fn verification_message(status: &str) -> &'static str {
    match status {
        "processed" => "Payment verified and donation marked as paid.",
        "processed_late" => "Late payment verified and applied to the medical case.",
        "already_processed" => "Donation was already marked as paid.",
        "pending" => "Paystack has not confirmed this payment yet.",
        "failed" => "Paystack reported that this payment failed.",
        "amount_mismatch" => "Paystack amount does not match the donation amount.",
        "overflow_rejected" => {
            "Payment was verified but rejected because the case is already fully funded."
        }
        _ => "Payment verification completed.",
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
            reservation_expires_at: None,
            expired_at: None,
            is_late_payment: false,
            payment_note: None,
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

    #[test]
    fn checkout_payment_is_late_only_when_provider_paid_at_is_after_expiry() {
        let expires_at = Utc::now();

        assert!(!is_late_checkout_payment(
            Some(expires_at),
            expires_at - chrono::TimeDelta::seconds(1)
        ));
        assert!(!is_late_checkout_payment(Some(expires_at), expires_at));
        assert!(is_late_checkout_payment(
            Some(expires_at),
            expires_at + chrono::TimeDelta::seconds(1)
        ));
    }
}
