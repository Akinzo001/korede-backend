use std::sync::Arc;

use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    adapters::donation_proof_retry::next_retry_at,
    domain::{
        donation::{Donation, DonationMethod, DonationProofStatus, DonationStatus},
        hospital::Hospital,
        medical_case::MedicalCase,
        settlement::{HospitalSettlement, HospitalSettlementStatus},
    },
    port::{
        donation::{
            DonationCaseLock, DonationFailureUpdate, DonationPaymentUpdate, DonationRepository,
            DonationRepositoryError, NewDonation,
        },
        email::{EmailError, EmailMessage, EmailService},
        hospital::{HospitalRepository, HospitalRepositoryError},
        patient::{PatientRepository, PatientRepositoryError},
        payment::{
            PaymentGateway, PaymentGatewayError, PaymentVerificationStatus,
            TransferInitiationRequest, TransferRecipientRequest, TransferStatus,
        },
        settlement::{
            NewHospitalSettlement, SettlementRecipientUpdate, SettlementRepositoryError,
            SettlementStatusUpdate, SettlementTransferUpdate,
        },
        sui::{DonationProofPublisher, DonationProofRequest},
    },
};

#[derive(Debug, Clone)]
pub struct PaystackWebhookCommand {
    pub event: String,
    pub reference: Option<String>,
    pub transfer_code: Option<String>,
    pub provider_id: Option<i64>,
    pub status: Option<String>,
    pub failure_reason: Option<String>,
    pub dedicated_account_number: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PaymentConfirmationOutcome {
    pub status: String,
    pub donation_id: Option<Uuid>,
    pub payment_status: Option<DonationStatus>,
}

#[derive(Debug, Clone, Copy)]
enum PendingPaymentBehavior {
    Fail,
    LeavePending,
}

#[derive(Debug, Error)]
pub enum PaymentApplicationError {
    #[error("{0}")]
    BadRequest(String),

    #[error("donation not found")]
    DonationNotFound,

    #[error("hospital not found")]
    HospitalNotFound,

    #[error("patient not found")]
    PatientNotFound,

    #[error("donation repository error: {0}")]
    DonationRepository(#[from] DonationRepositoryError),

    #[error("hospital repository error: {0}")]
    HospitalRepository(#[from] HospitalRepositoryError),

    #[error("patient repository error: {0}")]
    PatientRepository(#[from] PatientRepositoryError),

    #[error("settlement repository error: {0}")]
    SettlementRepository(#[from] SettlementRepositoryError),

    #[error("payment gateway error: {0}")]
    PaymentGateway(#[from] PaymentGatewayError),

    #[error("email error: {0}")]
    Email(#[from] EmailError),
}

pub struct PaymentApplicationService {
    donation_repository: Arc<dyn DonationRepository>,
    hospital_repository: Arc<dyn HospitalRepository>,
    patient_repository: Arc<dyn PatientRepository>,
    settlement_repository: Arc<dyn crate::port::settlement::HospitalSettlementRepository>,
    payment_gateway: Arc<dyn PaymentGateway>,
    donation_proof_publisher: Arc<dyn DonationProofPublisher>,
    email_service: Arc<dyn EmailService>,
    app_base_url: String,
    paystack_transfers_enabled: bool,
    paystack_transfer_currency: String,
    paystack_transfer_source: String,
}

impl PaymentApplicationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        donation_repository: Arc<dyn DonationRepository>,
        hospital_repository: Arc<dyn HospitalRepository>,
        patient_repository: Arc<dyn PatientRepository>,
        settlement_repository: Arc<dyn crate::port::settlement::HospitalSettlementRepository>,
        payment_gateway: Arc<dyn PaymentGateway>,
        donation_proof_publisher: Arc<dyn DonationProofPublisher>,
        email_service: Arc<dyn EmailService>,
        app_base_url: String,
        paystack_transfers_enabled: bool,
        paystack_transfer_currency: String,
        paystack_transfer_source: String,
    ) -> Self {
        Self {
            donation_repository,
            hospital_repository,
            patient_repository,
            settlement_repository,
            payment_gateway,
            donation_proof_publisher,
            email_service,
            app_base_url,
            paystack_transfers_enabled,
            paystack_transfer_currency,
            paystack_transfer_source,
        }
    }

    pub async fn verify_checkout_payment(
        &self,
        reference: &str,
    ) -> Result<PaymentConfirmationOutcome, PaymentApplicationError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(PaymentApplicationError::BadRequest(
                "paystack reference is required".to_owned(),
            ));
        }

        let Some(locked) = self
            .donation_repository
            .lock_pending_donation_for_confirmation(reference)
            .await?
        else {
            return Err(PaymentApplicationError::DonationNotFound);
        };

        if locked.donation.method != DonationMethod::Checkout {
            return Err(PaymentApplicationError::BadRequest(
                "manual verification is only supported for checkout donations".to_owned(),
            ));
        }

        self.finalize_checkout_donation(locked, reference, PendingPaymentBehavior::LeavePending)
            .await
    }

    pub async fn handle_paystack_webhook(
        &self,
        command: PaystackWebhookCommand,
    ) -> Result<String, PaymentApplicationError> {
        if command.event.starts_with("transfer.") {
            return self.finalize_transfer_webhook(command).await;
        }

        if command.event != "charge.success" {
            return Ok("ignored".to_owned());
        }

        if let Some(reference) = command.reference.as_deref() {
            if let Some(locked) = self
                .donation_repository
                .lock_pending_donation_for_confirmation(reference)
                .await?
            {
                let outcome = self
                    .finalize_checkout_donation(locked, reference, PendingPaymentBehavior::Fail)
                    .await?;
                return Ok(outcome.status);
            }
        }

        if let (Some(account_number), Some(reference)) = (
            command.dedicated_account_number.as_deref(),
            command.reference.as_deref(),
        ) {
            return self.finalize_dva_donation(account_number, reference).await;
        }

        Ok("ignored".to_owned())
    }

    pub async fn process_hospital_settlement_for_case(
        &self,
        medical_case: &MedicalCase,
    ) -> Result<HospitalSettlement, PaymentApplicationError> {
        let hospital = self
            .hospital_repository
            .find_hospital_by_id(medical_case.hospital_id)
            .await?
            .ok_or(PaymentApplicationError::HospitalNotFound)?;

        let settlement = self
            .settlement_repository
            .create_or_get_settlement(new_hospital_settlement(&hospital, medical_case))
            .await?;

        if !settlement.status.can_retry() {
            return Ok(settlement);
        }

        if !self.paystack_transfers_enabled {
            return self
                .settlement_repository
                .update_status(SettlementStatusUpdate {
                    settlement_id: settlement.id,
                    status: HospitalSettlementStatus::FailedConfig,
                    paystack_status: None,
                    failure_reason: Some("Paystack transfers are disabled".to_owned()),
                })
                .await
                .map_err(Into::into);
        }

        let Some(bank_code) = hospital_bank_code(&hospital) else {
            return self
                .settlement_repository
                .update_status(SettlementStatusUpdate {
                    settlement_id: settlement.id,
                    status: HospitalSettlementStatus::BankDetailsRequired,
                    paystack_status: None,
                    failure_reason: Some("hospital corporate_bank_code is required".to_owned()),
                })
                .await
                .map_err(Into::into);
        };

        if hospital.corporate_account_name.trim().is_empty()
            || hospital.corporate_account_number.trim().is_empty()
        {
            return self
                .settlement_repository
                .update_status(SettlementStatusUpdate {
                    settlement_id: settlement.id,
                    status: HospitalSettlementStatus::BankDetailsRequired,
                    paystack_status: None,
                    failure_reason: Some(
                        "hospital corporate account name and number are required".to_owned(),
                    ),
                })
                .await
                .map_err(Into::into);
        }

        let settlement = if settlement.paystack_recipient_code.is_none() {
            match self
                .payment_gateway
                .create_transfer_recipient(TransferRecipientRequest {
                    name: hospital.corporate_account_name.trim().to_owned(),
                    account_number: hospital.corporate_account_number.trim().to_owned(),
                    bank_code,
                    currency: self.paystack_transfer_currency.clone(),
                    description: format!("Korede settlement for case {}", medical_case.id),
                })
                .await
            {
                Ok(recipient) => {
                    let _ = (
                        recipient.provider_id,
                        recipient.account_name,
                        recipient.bank_name,
                        recipient.bank_code,
                    );
                    self.settlement_repository
                        .update_recipient(SettlementRecipientUpdate {
                            settlement_id: settlement.id,
                            status: HospitalSettlementStatus::RecipientCreated,
                            paystack_recipient_code: recipient.recipient_code,
                            paystack_status: Some("recipient_created".to_owned()),
                        })
                        .await?
                }
                Err(error) => {
                    return self
                        .settlement_gateway_failure_update(
                            settlement.id,
                            transfer_failure_status(&error),
                            error.to_string(),
                        )
                        .await;
                }
            }
        } else {
            settlement
        };

        let Some(recipient_code) = settlement.paystack_recipient_code.clone() else {
            return self
                .settlement_repository
                .update_status(SettlementStatusUpdate {
                    settlement_id: settlement.id,
                    status: HospitalSettlementStatus::Failed,
                    paystack_status: None,
                    failure_reason: Some("Paystack recipient code is missing".to_owned()),
                })
                .await
                .map_err(Into::into);
        };

        match self
            .payment_gateway
            .initiate_transfer(TransferInitiationRequest {
                amount_kobo: settlement.amount_kobo,
                recipient_code,
                reference: settlement.settlement_reference.clone(),
                reason: format!("Korede Health payout for {}", medical_case.title.trim()),
                source: self.paystack_transfer_source.clone(),
            })
            .await
        {
            Ok(transfer) => {
                let status = settlement_status_from_transfer_status(&transfer.status);
                self.settlement_repository
                    .update_transfer(SettlementTransferUpdate {
                        settlement_id: settlement.id,
                        status,
                        paystack_transfer_code: transfer.transfer_code,
                        paystack_transfer_id: transfer.provider_id,
                        paystack_status: transfer.provider_status,
                        failure_reason: transfer.message,
                    })
                    .await
                    .map_err(Into::into)
            }
            Err(error) => {
                self.settlement_gateway_failure_update(
                    settlement.id,
                    transfer_failure_status(&error),
                    error.to_string(),
                )
                .await
            }
        }
    }

    async fn finalize_checkout_donation(
        &self,
        locked: DonationCaseLock,
        reference: &str,
        pending_behavior: PendingPaymentBehavior,
    ) -> Result<PaymentConfirmationOutcome, PaymentApplicationError> {
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

        let verification = self.payment_gateway.verify_payment(reference).await?;

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
            self.donation_repository
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
            self.donation_repository
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
        let is_late_payment =
            is_late_checkout_payment(locked.donation.reservation_expires_at, paid_at);
        let payment_note = is_late_payment.then(|| {
            "Payment completed after the five-minute checkout reservation expired.".to_owned()
        });

        let payment_result = match self
            .donation_repository
            .mark_donation_paid(DonationPaymentUpdate {
                donation_id: locked.donation.id,
                paystack_transaction_reference: verification.provider_reference,
                paid_at,
                is_late_payment,
                payment_note,
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
            Err(DonationRepositoryError::AmountExceedsRemaining) => {
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

        let donation = self
            .publish_or_schedule_donation_proof(payment_result.donation, &locked.medical_case)
            .await?;

        self.handle_case_funding_completion(
            &locked.medical_case,
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
        &self,
        account_number: &str,
        reference: &str,
    ) -> Result<String, PaymentApplicationError> {
        if let Some(existing) = self
            .donation_repository
            .find_donation_by_reference(reference)
            .await?
        {
            if let Some(status) = existing_dva_webhook_status(&existing) {
                return Ok(status.to_owned());
            }
        }

        let Some(case_dva) = self
            .donation_repository
            .find_case_dva_by_account_number(account_number)
            .await?
        else {
            return Ok("ignored".to_owned());
        };

        let Some(public_case) = self
            .donation_repository
            .get_public_case_details_for_case_id(case_dva.medical_case_id)
            .await?
        else {
            return Ok("ignored".to_owned());
        };

        let verification = self.payment_gateway.verify_payment(reference).await?;

        if verification.status != PaymentVerificationStatus::Success {
            return Ok("failed".to_owned());
        }

        let donation = match self
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
            Err(DonationRepositoryError::AmountExceedsRemaining) => {
                return Ok("overflow_rejected".to_owned());
            }
            Err(error) => return Err(error.into()),
        };

        let donation = self
            .publish_or_schedule_donation_proof(donation, &public_case.medical_case)
            .await?;

        let remaining_amount_after = remaining_amount_after_dva_confirmation(
            public_case.medical_case.bill_amount_kobo,
            public_case.medical_case.amount_raised_kobo,
            donation.amount_kobo,
        );
        self.handle_case_funding_completion(&public_case.medical_case, remaining_amount_after)
            .await?;

        Ok("processed".to_owned())
    }

    async fn publish_or_schedule_donation_proof(
        &self,
        donation: Donation,
        medical_case: &MedicalCase,
    ) -> Result<Donation, PaymentApplicationError> {
        let now = Utc::now();
        let attempt_count = donation.proof_attempt_count + 1;
        let proof_result = self
            .donation_proof_publisher
            .publish_donation_proof(DonationProofRequest {
                case_id: medical_case.id.to_string(),
                hospital_id: medical_case.hospital_id.to_string(),
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

        self.donation_repository
            .update_donation_proof(proof_update)
            .await
            .map_err(Into::into)
    }

    async fn handle_case_funding_completion(
        &self,
        medical_case: &MedicalCase,
        remaining_amount_after: i64,
    ) -> Result<(), PaymentApplicationError> {
        if remaining_amount_after <= 0 {
            self.trigger_hospital_settlement(medical_case).await;
            self.notify_patient_case_funded(medical_case).await;
            self.notify_hospital_case_funded(medical_case).await;
        }

        self.maybe_close_case_dva(medical_case.id, remaining_amount_after)
            .await
    }

    async fn trigger_hospital_settlement(&self, medical_case: &MedicalCase) {
        match self
            .process_hospital_settlement_for_case(medical_case)
            .await
        {
            Ok(settlement) => {
                tracing::info!(
                    medical_case_id = %medical_case.id,
                    hospital_id = %medical_case.hospital_id,
                    settlement_id = %settlement.id,
                    settlement_status = settlement.status.as_str(),
                    "hospital settlement processed after case funding completion"
                );
            }
            Err(error) => {
                tracing::error!(
                    ?error,
                    medical_case_id = %medical_case.id,
                    hospital_id = %medical_case.hospital_id,
                    "failed to process hospital settlement after case funding completion"
                );
            }
        }
    }

    async fn settlement_gateway_failure_update(
        &self,
        settlement_id: Uuid,
        status: HospitalSettlementStatus,
        failure_reason: String,
    ) -> Result<HospitalSettlement, PaymentApplicationError> {
        self.settlement_repository
            .update_status(SettlementStatusUpdate {
                settlement_id,
                status,
                paystack_status: None,
                failure_reason: Some(failure_reason),
            })
            .await
            .map_err(Into::into)
    }

    async fn finalize_transfer_webhook(
        &self,
        command: PaystackWebhookCommand,
    ) -> Result<String, PaymentApplicationError> {
        let Some(status) =
            settlement_status_from_transfer_webhook(&command.event, command.status.as_deref())
        else {
            return Ok("ignored".to_owned());
        };

        let settlement = match command
            .reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(reference) => {
                self.settlement_repository
                    .find_by_reference(reference)
                    .await?
            }
            None => None,
        };
        let settlement = if settlement.is_none() {
            match command
                .transfer_code
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(transfer_code) => {
                    self.settlement_repository
                        .find_by_transfer_code(transfer_code)
                        .await?
                }
                None => None,
            }
        } else {
            settlement
        };

        let Some(settlement) = settlement else {
            return Ok("ignored".to_owned());
        };

        if settlement.status == status {
            return Ok("already_processed".to_owned());
        }

        let failure_reason = if matches!(
            status,
            HospitalSettlementStatus::Failed | HospitalSettlementStatus::Reversed
        ) {
            command
                .failure_reason
                .or_else(|| Some(format!("Paystack transfer event: {}", command.event)))
        } else {
            None
        };

        self.settlement_repository
            .update_transfer(SettlementTransferUpdate {
                settlement_id: settlement.id,
                status,
                paystack_transfer_code: command.transfer_code,
                paystack_transfer_id: command.provider_id,
                paystack_status: command.status.or_else(|| Some(command.event)),
                failure_reason,
            })
            .await?;

        Ok("processed".to_owned())
    }

    async fn notify_patient_case_funded(&self, medical_case: &MedicalCase) {
        if let Err(error) = self.send_patient_case_funded_email(medical_case).await {
            tracing::error!(
                ?error,
                medical_case_id = %medical_case.id,
                patient_id = %medical_case.patient_id,
                "failed to send patient funded-case email"
            );
        }
    }

    async fn notify_hospital_case_funded(&self, medical_case: &MedicalCase) {
        if let Err(error) = self.send_hospital_case_funded_email(medical_case).await {
            tracing::error!(
                ?error,
                medical_case_id = %medical_case.id,
                hospital_id = %medical_case.hospital_id,
                "failed to send hospital funded-case email"
            );
        }
    }

    async fn send_patient_case_funded_email(
        &self,
        medical_case: &MedicalCase,
    ) -> Result<(), PaymentApplicationError> {
        let patient = self
            .patient_repository
            .find_patient_by_id(medical_case.patient_id)
            .await?
            .ok_or(PaymentApplicationError::PatientNotFound)?;

        let Some(patient_email) = patient
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
        else {
            tracing::warn!(
                medical_case_id = %medical_case.id,
                patient_id = %medical_case.patient_id,
                "cannot send funded-case email because patient has no email address"
            );
            return Ok(());
        };

        let message = patient_case_funded_email_message(
            patient_email,
            patient.full_name.trim(),
            medical_case.title.trim(),
            medical_case.bill_amount_kobo,
            medical_case.public_slug.as_deref(),
            &self.app_base_url,
        );

        self.email_service.send(message).await.map_err(|error| {
            tracing::error!(
                %error,
                medical_case_id = %medical_case.id,
                patient_id = %medical_case.patient_id,
                "email provider failed to send funded-case email"
            );
            error.into()
        })
    }

    async fn send_hospital_case_funded_email(
        &self,
        medical_case: &MedicalCase,
    ) -> Result<(), PaymentApplicationError> {
        let hospital = self
            .hospital_repository
            .find_hospital_by_id(medical_case.hospital_id)
            .await?
            .ok_or(PaymentApplicationError::HospitalNotFound)?;

        let recipient_name = hospital
            .administrator_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| hospital.name.trim());
        let message = hospital_case_funded_email_message(
            hospital.email.trim(),
            recipient_name,
            hospital.name.trim(),
            medical_case.title.trim(),
            medical_case.bill_amount_kobo,
            medical_case.public_slug.as_deref(),
            &self.app_base_url,
        );

        self.email_service.send(message).await.map_err(|error| {
            tracing::error!(
                %error,
                medical_case_id = %medical_case.id,
                hospital_id = %medical_case.hospital_id,
                "email provider failed to send hospital funded-case email"
            );
            error.into()
        })
    }

    async fn maybe_close_case_dva(
        &self,
        medical_case_id: Uuid,
        remaining_amount_after: i64,
    ) -> Result<(), PaymentApplicationError> {
        if remaining_amount_after > 0 {
            return Ok(());
        }

        let Some(case_dva) = self
            .donation_repository
            .find_case_dva(medical_case_id)
            .await?
        else {
            return Ok(());
        };

        if !case_dva.is_active {
            return Ok(());
        }

        let deactivation_result = self
            .payment_gateway
            .deactivate_dva(case_dva.paystack_dedicated_account_id)
            .await;

        match deactivation_result {
            Ok(()) => {
                let _ = self
                    .donation_repository
                    .deactivate_case_dva(medical_case_id, None)
                    .await?;
            }
            Err(error) => {
                let _ = self
                    .donation_repository
                    .deactivate_case_dva(medical_case_id, Some(error.to_string()))
                    .await?;
            }
        }

        Ok(())
    }
}

fn new_hospital_settlement(
    hospital: &Hospital,
    medical_case: &MedicalCase,
) -> NewHospitalSettlement {
    NewHospitalSettlement {
        hospital_id: hospital.id,
        medical_case_id: medical_case.id,
        amount_kobo: medical_case.bill_amount_kobo,
        status: HospitalSettlementStatus::Pending,
        settlement_reference: settlement_reference_for_case(medical_case.id),
        bank_name: hospital.bank_name.trim().to_owned(),
        bank_code: hospital_bank_code(hospital),
        account_name: hospital.corporate_account_name.trim().to_owned(),
        account_number: hospital.corporate_account_number.trim().to_owned(),
        failure_reason: None,
    }
}

fn hospital_bank_code(hospital: &Hospital) -> Option<String> {
    hospital
        .corporate_bank_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn settlement_reference_for_case(medical_case_id: Uuid) -> String {
    format!("korede-settlement-{}", medical_case_id.simple())
}

fn transfer_failure_status(error: &PaymentGatewayError) -> HospitalSettlementStatus {
    match error {
        PaymentGatewayError::MissingConfig(_) => HospitalSettlementStatus::FailedConfig,
        PaymentGatewayError::RequestFailed | PaymentGatewayError::Provider(_) => {
            HospitalSettlementStatus::Failed
        }
    }
}

fn settlement_status_from_transfer_status(status: &TransferStatus) -> HospitalSettlementStatus {
    match status {
        TransferStatus::Success => HospitalSettlementStatus::Paid,
        TransferStatus::Failed => HospitalSettlementStatus::Failed,
        TransferStatus::OtpRequired => HospitalSettlementStatus::OtpRequired,
        TransferStatus::Pending => HospitalSettlementStatus::Processing,
    }
}

fn settlement_status_from_transfer_webhook(
    event: &str,
    provider_status: Option<&str>,
) -> Option<HospitalSettlementStatus> {
    match event {
        "transfer.success" => Some(HospitalSettlementStatus::Paid),
        "transfer.failed" => Some(HospitalSettlementStatus::Failed),
        "transfer.reversed" => Some(HospitalSettlementStatus::Reversed),
        "transfer.otp" => Some(HospitalSettlementStatus::OtpRequired),
        _ => provider_status.map(|status| match status {
            "success" => HospitalSettlementStatus::Paid,
            "failed" => HospitalSettlementStatus::Failed,
            "reversed" => HospitalSettlementStatus::Reversed,
            "otp" => HospitalSettlementStatus::OtpRequired,
            _ => HospitalSettlementStatus::Processing,
        }),
    }
}

fn patient_case_funded_email_message(
    patient_email: &str,
    patient_name: &str,
    case_title: &str,
    bill_amount_kobo: i64,
    public_slug: Option<&str>,
    app_base_url: &str,
) -> EmailMessage {
    let patient_name = if patient_name.is_empty() {
        "there"
    } else {
        patient_name
    };
    let case_title = if case_title.is_empty() {
        "your medical case"
    } else {
        case_title
    };
    let amount = format_ngn_amount(bill_amount_kobo);
    let public_link = funded_case_public_link(app_base_url, public_slug);
    let link_line = public_link
        .as_ref()
        .map(|link| format!("\nLink: {link}"))
        .unwrap_or_default();
    let html_link = public_link
        .as_ref()
        .map(|link| format!("<p><strong>Link:</strong> <a href=\"{link}\">{link}</a></p>"))
        .unwrap_or_default();

    EmailMessage {
        to_email: patient_email.to_owned(),
        to_name: Some(patient_name.to_owned()),
        subject: "Your Korede Health medical case is fully funded".to_owned(),
        text_body: format!(
            "Hello {patient_name},\n\nGood news - donations for your medical case on Korede Health are now complete.\n\nCase: {case_title}\nAmount funded: {amount}{link_line}\n\nYour hospital will follow up with the next treatment steps.\n\nThank you,\nKorede Health"
        ),
        html_body: Some(format!(
            "<p>Hello {patient_name},</p><p>Good news - donations for your medical case on Korede Health are now complete.</p><p><strong>Case:</strong> {case_title}</p><p><strong>Amount funded:</strong> {amount}</p>{html_link}<p>Your hospital will follow up with the next treatment steps.</p><p>Thank you,<br>Korede Health</p>"
        )),
    }
}

fn hospital_case_funded_email_message(
    hospital_email: &str,
    recipient_name: &str,
    hospital_name: &str,
    case_title: &str,
    bill_amount_kobo: i64,
    public_slug: Option<&str>,
    app_base_url: &str,
) -> EmailMessage {
    let recipient_name = if recipient_name.is_empty() {
        "there"
    } else {
        recipient_name
    };
    let hospital_name = if hospital_name.is_empty() {
        "your hospital"
    } else {
        hospital_name
    };
    let case_title = if case_title.is_empty() {
        "the medical case"
    } else {
        case_title
    };
    let amount = format_ngn_amount(bill_amount_kobo);
    let public_link = funded_case_public_link(app_base_url, public_slug);
    let link_line = public_link
        .as_ref()
        .map(|link| format!("\nLink: {link}"))
        .unwrap_or_default();
    let html_link = public_link
        .as_ref()
        .map(|link| format!("<p><strong>Link:</strong> <a href=\"{link}\">{link}</a></p>"))
        .unwrap_or_default();

    EmailMessage {
        to_email: hospital_email.to_owned(),
        to_name: Some(recipient_name.to_owned()),
        subject: "A Korede Health medical case is fully funded".to_owned(),
        text_body: format!(
            "Hello {recipient_name},\n\nGood news - donations for {case_title} on Korede Health are now complete.\n\nHospital: {hospital_name}\nCase: {case_title}\nAmount funded: {amount}{link_line}\n\nPlease follow up with the patient and continue the treatment process.\n\nThank you,\nKorede Health"
        ),
        html_body: Some(format!(
            "<p>Hello {recipient_name},</p><p>Good news - donations for {case_title} on Korede Health are now complete.</p><p><strong>Hospital:</strong> {hospital_name}</p><p><strong>Case:</strong> {case_title}</p><p><strong>Amount funded:</strong> {amount}</p>{html_link}<p>Please follow up with the patient and continue the treatment process.</p><p>Thank you,<br>Korede Health</p>"
        )),
    }
}

fn funded_case_public_link(app_base_url: &str, public_slug: Option<&str>) -> Option<String> {
    let public_slug = public_slug?.trim();
    if public_slug.is_empty() {
        return None;
    }

    Some(format!(
        "{}/cases/{public_slug}",
        app_base_url.trim_end_matches('/')
    ))
}

fn existing_dva_webhook_status(existing: &Donation) -> Option<&'static str> {
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

fn format_ngn_amount(amount_kobo: i64) -> String {
    let naira = amount_kobo / 100;
    let kobo = amount_kobo.abs() % 100;
    let mut digits = naira.abs().to_string();
    let mut formatted = String::new();

    while digits.len() > 3 {
        let tail = digits.split_off(digits.len() - 3);
        if formatted.is_empty() {
            formatted = tail;
        } else {
            formatted = format!("{tail},{formatted}");
        }
    }

    if formatted.is_empty() {
        formatted = digits;
    } else {
        formatted = format!("{digits},{formatted}");
    }

    if amount_kobo < 0 {
        formatted = format!("-{formatted}");
    }

    format!("NGN {formatted}.{kobo:02}")
}

pub fn verification_message(status: &str) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::donation::DonationProofStatus;
    use crate::domain::hospital::HospitalVerificationStatus;

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

    fn test_hospital(bank_code: Option<String>) -> Hospital {
        let now = Utc::now();
        Hospital {
            id: Uuid::new_v4(),
            name: "Korede Hospital".to_owned(),
            email: "hospital@example.com".to_owned(),
            email_verified: true,
            email_verified_at: Some(now),
            password_hash: "hash".to_owned(),
            phone_number: None,
            official_address: Some("1 Health Way".to_owned()),
            administrator_name: Some("Dr Admin".to_owned()),
            cac_registration_number: None,
            medical_license_number: None,
            corporate_account_name: "Korede Hospital Ltd".to_owned(),
            corporate_account_number: "0123456789".to_owned(),
            corporate_bank_code: bank_code,
            bank_name: "Test Bank".to_owned(),
            verification_status: HospitalVerificationStatus::Verified,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn existing_dva_webhook_status_is_idempotent_for_paid_donation() {
        assert_eq!(
            existing_dva_webhook_status(&donation_with_status(DonationStatus::Paid)),
            Some("already_processed")
        );
    }

    #[test]
    fn settlement_reference_is_stable_for_case_id() {
        let case_id = Uuid::new_v4();

        assert_eq!(
            settlement_reference_for_case(case_id),
            settlement_reference_for_case(case_id)
        );
        assert!(settlement_reference_for_case(case_id).starts_with("korede-settlement-"));
    }

    #[test]
    fn hospital_bank_code_trims_blank_values() {
        assert_eq!(
            hospital_bank_code(&test_hospital(Some(" 058 ".to_owned()))),
            Some("058".to_owned())
        );
        assert_eq!(
            hospital_bank_code(&test_hospital(Some("   ".to_owned()))),
            None
        );
        assert_eq!(hospital_bank_code(&test_hospital(None)), None);
    }

    #[test]
    fn transfer_status_maps_to_settlement_status() {
        assert_eq!(
            settlement_status_from_transfer_status(&TransferStatus::Success),
            HospitalSettlementStatus::Paid
        );
        assert_eq!(
            settlement_status_from_transfer_status(&TransferStatus::Pending),
            HospitalSettlementStatus::Processing
        );
        assert_eq!(
            settlement_status_from_transfer_status(&TransferStatus::OtpRequired),
            HospitalSettlementStatus::OtpRequired
        );
        assert_eq!(
            settlement_status_from_transfer_status(&TransferStatus::Failed),
            HospitalSettlementStatus::Failed
        );
    }

    #[test]
    fn transfer_webhook_event_maps_to_settlement_status() {
        assert_eq!(
            settlement_status_from_transfer_webhook("transfer.success", None),
            Some(HospitalSettlementStatus::Paid)
        );
        assert_eq!(
            settlement_status_from_transfer_webhook("transfer.failed", None),
            Some(HospitalSettlementStatus::Failed)
        );
        assert_eq!(
            settlement_status_from_transfer_webhook("transfer.reversed", None),
            Some(HospitalSettlementStatus::Reversed)
        );
        assert_eq!(
            settlement_status_from_transfer_webhook("transfer.update", Some("otp")),
            Some(HospitalSettlementStatus::OtpRequired)
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
            None,
            expires_at + chrono::Duration::minutes(1)
        ));
        assert!(!is_late_checkout_payment(Some(expires_at), expires_at));
        assert!(is_late_checkout_payment(
            Some(expires_at),
            expires_at + chrono::Duration::seconds(1)
        ));
    }

    #[test]
    fn funded_case_public_link_uses_app_base_url() {
        assert_eq!(
            funded_case_public_link(
                "https://korede-health.akinzo.buzz/",
                Some("patient-case-123")
            ),
            Some("https://korede-health.akinzo.buzz/cases/patient-case-123".to_owned())
        );
    }

    #[test]
    fn funded_case_email_mentions_patient_case_amount_and_link() {
        let message = patient_case_funded_email_message(
            "patient@example.com",
            "Andrew",
            "Surgery",
            1_250_050,
            Some("andrew-surgery-123"),
            "https://korede-health.akinzo.buzz/",
        );

        assert!(message.subject.contains("fully funded"));
        assert!(message.text_body.contains("Andrew"));
        assert!(message.text_body.contains("Surgery"));
        assert!(message.text_body.contains("NGN 12,500.50"));
        assert!(message
            .text_body
            .contains("https://korede-health.akinzo.buzz/cases/andrew-surgery-123"));
    }

    #[test]
    fn funded_case_email_mentions_hospital_case_amount_and_link() {
        let message = hospital_case_funded_email_message(
            "hospital@example.com",
            "Dr Admin",
            "Arike Clinic",
            "Surgery",
            2_000_000,
            Some("andrew-surgery-123"),
            "https://korede-health.akinzo.buzz",
        );

        assert!(message.subject.contains("fully funded"));
        assert!(message.text_body.contains("Dr Admin"));
        assert!(message.text_body.contains("Arike Clinic"));
        assert!(message.text_body.contains("Surgery"));
        assert!(message.text_body.contains("NGN 20,000.00"));
        assert!(message
            .text_body
            .contains("https://korede-health.akinzo.buzz/cases/andrew-surgery-123"));
    }
}
