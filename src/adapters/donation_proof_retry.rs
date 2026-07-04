use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::time::interval;
use tracing::{error, info};

use crate::{
    domain::donation::DonationProofStatus,
    port::{
        donation::{DonationProofAttemptUpdate, DonationProofJob, DonationRepository},
        sui::{DonationProofPublisher, DonationProofRequest},
    },
};

const PROOF_RETRY_DELAYS_SECONDS: [i64; 6] = [60, 300, 900, 3600, 21600, 86400];
const PROOF_RETRY_BATCH_SIZE: i64 = 25;
const PROOF_RETRY_POLL_SECONDS: u64 = 30;

pub async fn run_donation_proof_retry_worker(
    donation_repository: std::sync::Arc<dyn DonationRepository>,
    donation_proof_publisher: std::sync::Arc<dyn DonationProofPublisher>,
) {
    let mut ticker = interval(Duration::from_secs(PROOF_RETRY_POLL_SECONDS));

    loop {
        ticker.tick().await;

        if let Err(error) =
            process_retry_batch(&*donation_repository, &*donation_proof_publisher).await
        {
            error!(%error, "donation proof retry worker pass failed");
        }
    }
}

async fn process_retry_batch(
    donation_repository: &dyn DonationRepository,
    donation_proof_publisher: &dyn DonationProofPublisher,
) -> Result<(), crate::port::donation::DonationRepositoryError> {
    let jobs = donation_repository
        .acquire_retryable_proof_jobs(PROOF_RETRY_BATCH_SIZE, Utc::now())
        .await?;

    for job in jobs {
        process_retry_job(donation_repository, donation_proof_publisher, job).await;
    }

    Ok(())
}

async fn process_retry_job(
    donation_repository: &dyn DonationRepository,
    donation_proof_publisher: &dyn DonationProofPublisher,
    job: DonationProofJob,
) {
    if !is_retryable_proof_status(&job.donation.proof_status)
        || job.donation.sui_tx_digest.is_some()
    {
        return;
    }

    let now = Utc::now();
    let attempt_count = job.donation.proof_attempt_count + 1;

    match donation_proof_publisher
        .publish_donation_proof(DonationProofRequest {
            case_id: job.medical_case.id.to_string(),
            hospital_id: job.medical_case.hospital_id.to_string(),
            amount_kobo: job.donation.amount_kobo as u64,
            payment_reference: job.donation.paystack_reference.clone(),
        })
        .await
    {
        Ok(receipt) => {
            if let Err(error) = donation_repository
                .update_donation_proof(DonationProofAttemptUpdate {
                    donation_id: job.donation.id,
                    proof_status: DonationProofStatus::Published,
                    sui_network: Some(receipt.network),
                    sui_tx_digest: Some(receipt.tx_digest),
                    proof_attempt_count: attempt_count,
                    proof_last_attempt_at: now,
                    proof_next_retry_at: None,
                    proof_last_error: None,
                    proof_published_at: Some(now),
                })
                .await
            {
                error!(%error, donation_id = %job.donation.id, "failed to persist published donation proof");
            } else {
                info!(donation_id = %job.donation.id, "published donation proof from retry worker");
            }
        }
        Err(error) => {
            let next_retry_at = next_retry_at(attempt_count, now);
            let status = if next_retry_at.is_some() {
                DonationProofStatus::PendingRetry
            } else {
                DonationProofStatus::Failed
            };

            if let Err(repository_error) = donation_repository
                .update_donation_proof(DonationProofAttemptUpdate {
                    donation_id: job.donation.id,
                    proof_status: status,
                    sui_network: None,
                    sui_tx_digest: None,
                    proof_attempt_count: attempt_count,
                    proof_last_attempt_at: now,
                    proof_next_retry_at: next_retry_at,
                    proof_last_error: Some(error.to_string()),
                    proof_published_at: None,
                })
                .await
            {
                error!(%repository_error, donation_id = %job.donation.id, "failed to persist donation proof retry failure");
            }
        }
    }
}

pub fn next_retry_at(attempt_count: i32, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let index = usize::try_from(attempt_count.saturating_sub(1)).ok()?;
    PROOF_RETRY_DELAYS_SECONDS
        .get(index)
        .map(|seconds| now + chrono::TimeDelta::seconds(*seconds))
}

fn is_retryable_proof_status(status: &DonationProofStatus) -> bool {
    matches!(
        status,
        DonationProofStatus::Pending | DonationProofStatus::PendingRetry
    )
}

#[cfg(test)]
mod tests {
    use super::{is_retryable_proof_status, next_retry_at};
    use crate::domain::donation::DonationProofStatus;
    use chrono::{TimeZone, Utc};

    #[test]
    fn next_retry_uses_expected_backoff_schedule() {
        let now = Utc.with_ymd_and_hms(2026, 6, 20, 10, 0, 0).unwrap();

        assert_eq!(
            next_retry_at(1, now).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 20, 10, 1, 0).unwrap()
        );
        assert_eq!(
            next_retry_at(2, now).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 20, 10, 5, 0).unwrap()
        );
        assert_eq!(
            next_retry_at(6, now).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 21, 10, 0, 0).unwrap()
        );
        assert!(next_retry_at(7, now).is_none());
    }

    #[test]
    fn terminal_failed_proofs_are_not_retryable() {
        assert!(is_retryable_proof_status(&DonationProofStatus::Pending));
        assert!(is_retryable_proof_status(
            &DonationProofStatus::PendingRetry
        ));
        assert!(!is_retryable_proof_status(&DonationProofStatus::Published));
        assert!(!is_retryable_proof_status(&DonationProofStatus::Failed));
    }
}
