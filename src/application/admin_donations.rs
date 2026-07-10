use std::sync::Arc;

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    adapters::donation_proof_retry::next_retry_at,
    domain::donation::{Donation, DonationMethod, DonationProofStatus, DonationStatus},
    port::{
        donation::{
            AdminDonationFilters, AdminDonationListQuery, AdminDonationOperation,
            DonationProofAttemptUpdate, DonationRepository, DonationRepositoryError,
        },
        sui::{
            DonationProofError, DonationProofPublisher, DonationProofReceipt, DonationProofRequest,
        },
    },
};

#[derive(Debug, Clone)]
pub struct AdminDonationListCommand {
    pub status: Option<String>,
    pub method: Option<String>,
    pub proof_status: Option<String>,
    pub hospital_id: Option<Uuid>,
    pub medical_case_id: Option<Uuid>,
    pub paystack_reference: Option<String>,
    pub is_late_payment: Option<bool>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AdminDonationListResult {
    pub donations: Vec<AdminDonationOperation>,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct AdminDonationProofRetryResult {
    pub message: String,
    pub donation: Donation,
}

#[derive(Debug, Error)]
pub enum AdminDonationError {
    #[error("{0}")]
    Validation(String),

    #[error("donation not found")]
    DonationNotFound,

    #[error("{0}")]
    Conflict(String),

    #[error("donation repository error: {0}")]
    DonationRepository(#[from] DonationRepositoryError),

    #[error("donation proof error: {0}")]
    DonationProof(#[from] DonationProofError),
}

pub struct AdminDonationService {
    donation_repository: Arc<dyn DonationRepository>,
    donation_proof_publisher: Arc<dyn DonationProofPublisher>,
}

impl AdminDonationService {
    pub fn new(
        donation_repository: Arc<dyn DonationRepository>,
        donation_proof_publisher: Arc<dyn DonationProofPublisher>,
    ) -> Self {
        Self {
            donation_repository,
            donation_proof_publisher,
        }
    }

    pub async fn list_donations(
        &self,
        command: AdminDonationListCommand,
    ) -> Result<AdminDonationListResult, AdminDonationError> {
        let query = admin_donation_list_query(command)?;
        let total = self
            .donation_repository
            .count_admin_donations(query.filters.clone())
            .await?;
        let donations = self
            .donation_repository
            .list_admin_donations(query.clone())
            .await?;

        Ok(AdminDonationListResult {
            donations,
            limit: query.limit,
            offset: query.offset,
            total,
        })
    }

    pub async fn get_donation(
        &self,
        donation_id: Uuid,
    ) -> Result<AdminDonationOperation, AdminDonationError> {
        self.donation_repository
            .get_admin_donation(donation_id)
            .await?
            .ok_or(AdminDonationError::DonationNotFound)
    }

    pub async fn retry_donation_proof(
        &self,
        donation_id: Uuid,
    ) -> Result<AdminDonationProofRetryResult, AdminDonationError> {
        let operation = self
            .donation_repository
            .get_admin_donation(donation_id)
            .await?
            .ok_or(AdminDonationError::DonationNotFound)?;
        validate_manual_retry_target(&operation.donation)?;

        let now = Utc::now();
        let attempt_count = operation.donation.proof_attempt_count + 1;
        let proof_result = self
            .donation_proof_publisher
            .publish_donation_proof(DonationProofRequest {
                case_id: operation.donation.medical_case_id.to_string(),
                hospital_id: operation.hospital_id.to_string(),
                amount_kobo: operation.donation.amount_kobo as u64,
                payment_reference: operation.donation.paystack_reference.clone(),
            })
            .await;

        let (message, update) = match proof_result {
            Ok(receipt) => (
                "published".to_owned(),
                proof_retry_success_update(donation_id, attempt_count, receipt, now),
            ),
            Err(DonationProofError::MissingConfig(key)) => {
                return Err(DonationProofError::MissingConfig(key).into());
            }
            Err(error) => {
                let update =
                    proof_retry_failure_update(donation_id, attempt_count, error.to_string(), now);
                let message = if update.proof_status == DonationProofStatus::Failed {
                    "failed".to_owned()
                } else {
                    "scheduled_for_retry".to_owned()
                };
                (message, update)
            }
        };

        let donation = self
            .donation_repository
            .update_donation_proof(update)
            .await?;

        Ok(AdminDonationProofRetryResult { message, donation })
    }
}

fn admin_donation_list_query(
    command: AdminDonationListCommand,
) -> Result<AdminDonationListQuery, AdminDonationError> {
    Ok(AdminDonationListQuery {
        filters: AdminDonationFilters {
            status: parse_optional_donation_status(command.status.as_deref())?,
            method: parse_optional_donation_method(command.method.as_deref())?,
            proof_status: parse_optional_proof_status(command.proof_status.as_deref())?,
            hospital_id: command.hospital_id,
            medical_case_id: command.medical_case_id,
            paystack_reference: command
                .paystack_reference
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            is_late_payment: command.is_late_payment,
            from: command.from,
            to: command.to,
        },
        limit: normalize_limit(command.limit)?,
        offset: normalize_offset(command.offset)?,
    })
}

fn parse_optional_donation_status(
    value: Option<&str>,
) -> Result<Option<DonationStatus>, AdminDonationError> {
    let Some(value) = normalized_optional_filter(value) else {
        return Ok(None);
    };

    let status = match value.as_str() {
        "pending" => DonationStatus::Pending,
        "paid" => DonationStatus::Paid,
        "failed" => DonationStatus::Failed,
        "expired" => DonationStatus::Expired,
        "rejected_overflow" => DonationStatus::RejectedOverflow,
        _ => {
            return Err(AdminDonationError::Validation(
                "status must be pending, paid, failed, expired, or rejected_overflow".to_owned(),
            ))
        }
    };

    Ok(Some(status))
}

fn parse_optional_donation_method(
    value: Option<&str>,
) -> Result<Option<DonationMethod>, AdminDonationError> {
    let Some(value) = normalized_optional_filter(value) else {
        return Ok(None);
    };

    let method = match value.as_str() {
        "checkout" => DonationMethod::Checkout,
        "dva_transfer" => DonationMethod::DvaTransfer,
        _ => {
            return Err(AdminDonationError::Validation(
                "method must be checkout or dva_transfer".to_owned(),
            ))
        }
    };

    Ok(Some(method))
}

fn parse_optional_proof_status(
    value: Option<&str>,
) -> Result<Option<DonationProofStatus>, AdminDonationError> {
    let Some(value) = normalized_optional_filter(value) else {
        return Ok(None);
    };

    let status = match value.as_str() {
        "pending" => DonationProofStatus::Pending,
        "pending_retry" => DonationProofStatus::PendingRetry,
        "published" => DonationProofStatus::Published,
        "failed" => DonationProofStatus::Failed,
        _ => {
            return Err(AdminDonationError::Validation(
                "proof_status must be pending, pending_retry, published, or failed".to_owned(),
            ))
        }
    };

    Ok(Some(status))
}

fn normalized_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn normalize_limit(limit: Option<i64>) -> Result<i64, AdminDonationError> {
    match limit {
        Some(value) if value < 1 => Err(AdminDonationError::Validation(
            "limit must be greater than zero".to_owned(),
        )),
        Some(value) => Ok(value.min(100)),
        None => Ok(50),
    }
}

fn normalize_offset(offset: Option<i64>) -> Result<i64, AdminDonationError> {
    match offset {
        Some(value) if value < 0 => Err(AdminDonationError::Validation(
            "offset must be zero or greater".to_owned(),
        )),
        Some(value) => Ok(value),
        None => Ok(0),
    }
}

fn validate_manual_retry_target(donation: &Donation) -> Result<(), AdminDonationError> {
    if donation.status != DonationStatus::Paid {
        return Err(AdminDonationError::Conflict(
            "only paid donations can publish proof".to_owned(),
        ));
    }

    if donation.proof_status == DonationProofStatus::Published {
        return Err(AdminDonationError::Conflict(
            "donation proof is already published".to_owned(),
        ));
    }

    Ok(())
}

fn proof_retry_success_update(
    donation_id: Uuid,
    attempt_count: i32,
    receipt: DonationProofReceipt,
    now: DateTime<Utc>,
) -> DonationProofAttemptUpdate {
    DonationProofAttemptUpdate {
        donation_id,
        proof_status: DonationProofStatus::Published,
        sui_network: Some(receipt.network),
        sui_tx_digest: Some(receipt.tx_digest),
        proof_attempt_count: attempt_count,
        proof_last_attempt_at: now,
        proof_next_retry_at: None,
        proof_last_error: None,
        proof_published_at: Some(now),
    }
}

fn proof_retry_failure_update(
    donation_id: Uuid,
    attempt_count: i32,
    error: String,
    now: DateTime<Utc>,
) -> DonationProofAttemptUpdate {
    let proof_next_retry_at = next_retry_at(attempt_count, now);
    DonationProofAttemptUpdate {
        donation_id,
        proof_status: if proof_next_retry_at.is_some() {
            DonationProofStatus::PendingRetry
        } else {
            DonationProofStatus::Failed
        },
        sui_network: None,
        sui_tx_digest: None,
        proof_attempt_count: attempt_count,
        proof_last_attempt_at: now,
        proof_next_retry_at,
        proof_last_error: Some(error),
        proof_published_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn admin_donation_filters_reject_invalid_values() {
        let command = AdminDonationListCommand {
            status: Some("complete".to_owned()),
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            is_late_payment: None,
            from: None,
            to: None,
            limit: None,
            offset: None,
        };

        assert!(matches!(
            admin_donation_list_query(command),
            Err(AdminDonationError::Validation(_))
        ));
    }

    #[test]
    fn admin_donation_filters_accept_expired_checkout_status() {
        assert_eq!(
            parse_optional_donation_status(Some("expired")).unwrap(),
            Some(DonationStatus::Expired)
        );
    }

    #[test]
    fn admin_donation_pagination_defaults_and_clamps_limit() {
        let default_query = admin_donation_list_query(AdminDonationListCommand {
            status: None,
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            is_late_payment: None,
            from: None,
            to: None,
            limit: None,
            offset: None,
        })
        .expect("default pagination should be valid");

        assert_eq!(default_query.limit, 50);
        assert_eq!(default_query.offset, 0);

        let clamped_query = admin_donation_list_query(AdminDonationListCommand {
            status: None,
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            is_late_payment: None,
            from: None,
            to: None,
            limit: Some(500),
            offset: Some(10),
        })
        .expect("large limit should be clamped");

        assert_eq!(clamped_query.limit, 100);
        assert_eq!(clamped_query.offset, 10);
    }

    #[test]
    fn admin_donation_pagination_rejects_invalid_offset() {
        let command = AdminDonationListCommand {
            status: None,
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            is_late_payment: None,
            from: None,
            to: None,
            limit: Some(25),
            offset: Some(-1),
        };

        assert!(matches!(
            admin_donation_list_query(command),
            Err(AdminDonationError::Validation(_))
        ));
    }

    #[test]
    fn manual_retry_rejects_unpaid_donation() {
        let mut donation = test_donation(DonationStatus::Pending, DonationProofStatus::Pending);

        assert!(matches!(
            validate_manual_retry_target(&donation),
            Err(AdminDonationError::Conflict(_))
        ));

        donation.status = DonationStatus::Paid;
        donation.proof_status = DonationProofStatus::Published;

        assert!(matches!(
            validate_manual_retry_target(&donation),
            Err(AdminDonationError::Conflict(_))
        ));
    }

    #[test]
    fn proof_retry_success_maps_receipt_into_published_state() {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        let donation_id = Uuid::new_v4();

        let update = proof_retry_success_update(
            donation_id,
            2,
            DonationProofReceipt {
                network: "testnet".to_owned(),
                tx_digest: "digest-1".to_owned(),
            },
            now,
        );

        assert_eq!(update.donation_id, donation_id);
        assert_eq!(update.proof_status, DonationProofStatus::Published);
        assert_eq!(update.sui_network.as_deref(), Some("testnet"));
        assert_eq!(update.sui_tx_digest.as_deref(), Some("digest-1"));
        assert_eq!(update.proof_attempt_count, 2);
        assert_eq!(update.proof_published_at, Some(now));
        assert!(update.proof_next_retry_at.is_none());
        assert!(update.proof_last_error.is_none());
    }

    #[test]
    fn proof_retry_failure_schedules_retry_then_terminal_failure() {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        let donation_id = Uuid::new_v4();

        let retry_update =
            proof_retry_failure_update(donation_id, 1, "temporary sui error".to_owned(), now);

        assert_eq!(retry_update.proof_status, DonationProofStatus::PendingRetry);
        assert!(retry_update.proof_next_retry_at.is_some());
        assert_eq!(
            retry_update.proof_last_error.as_deref(),
            Some("temporary sui error")
        );

        let failed_update =
            proof_retry_failure_update(donation_id, 7, "permanent sui error".to_owned(), now);

        assert_eq!(failed_update.proof_status, DonationProofStatus::Failed);
        assert!(failed_update.proof_next_retry_at.is_none());
    }

    fn test_donation(status: DonationStatus, proof_status: DonationProofStatus) -> Donation {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        Donation {
            id: Uuid::new_v4(),
            medical_case_id: Uuid::new_v4(),
            donor_display_name: "Ada".to_owned(),
            donor_email: "ada@example.com".to_owned(),
            amount_kobo: 10_000,
            method: DonationMethod::Checkout,
            paystack_reference: "ref-1".to_owned(),
            paystack_transaction_reference: None,
            paystack_access_code: None,
            paystack_authorization_url: None,
            paystack_customer_code: None,
            paystack_dedicated_account_id: None,
            paystack_dedicated_account_number: None,
            paystack_dedicated_account_name: None,
            paystack_dedicated_bank_name: None,
            paystack_dedicated_bank_slug: None,
            status,
            paid_at: None,
            reservation_expires_at: None,
            expired_at: None,
            is_late_payment: false,
            payment_note: None,
            proof_status,
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
}
