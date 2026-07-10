use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::{HospitalDocument, HospitalDocumentStatus, HospitalDocumentType},
    },
    port::{
        email::{EmailError, EmailMessage, EmailService},
        hospital::{HospitalDocumentReview, HospitalRepository, HospitalRepositoryError},
    },
};

const MAX_REVIEW_MESSAGE_LENGTH: usize = 2_000;

#[derive(Debug, Clone)]
pub struct ReviewHospitalDocumentCommand {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReviewHospitalDocumentResult {
    pub document: HospitalDocument,
    pub hospital_verification_status: HospitalVerificationStatus,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AdminHospitalError {
    #[error("{0}")]
    Validation(String),

    #[error("hospital not found")]
    HospitalNotFound,

    #[error("hospital repository error: {0}")]
    HospitalRepository(#[from] HospitalRepositoryError),

    #[error("email error: {0}")]
    Email(#[from] EmailError),
}

pub struct AdminHospitalService {
    hospital_repository: Arc<dyn HospitalRepository>,
    email_service: Arc<dyn EmailService>,
}

impl AdminHospitalService {
    pub fn new(
        hospital_repository: Arc<dyn HospitalRepository>,
        email_service: Arc<dyn EmailService>,
    ) -> Self {
        Self {
            hospital_repository,
            email_service,
        }
    }

    pub async fn review_hospital_document(
        &self,
        hospital_id: Uuid,
        document_id: Uuid,
        command: ReviewHospitalDocumentCommand,
    ) -> Result<ReviewHospitalDocumentResult, AdminHospitalError> {
        let review = validate_document_review_command(&command)?;
        self.hospital_repository
            .find_hospital_by_id(hospital_id)
            .await?
            .ok_or(AdminHospitalError::HospitalNotFound)?;

        let document = self
            .hospital_repository
            .review_hospital_document(hospital_id, document_id, review.clone())
            .await?;
        let documents = self
            .hospital_repository
            .list_hospital_documents(hospital_id)
            .await?;
        let verification_status = hospital_verification_status_after_document_review(&documents);
        let hospital = self
            .hospital_repository
            .update_hospital_verification_status(hospital_id, verification_status.clone())
            .await?;

        self.send_hospital_document_review_email(&hospital, &document, &verification_status)
            .await?;

        Ok(ReviewHospitalDocumentResult {
            document,
            hospital_verification_status: verification_status,
            message: document_review_response_message(&review.status).to_owned(),
        })
    }

    async fn send_hospital_document_review_email(
        &self,
        hospital: &Hospital,
        document: &HospitalDocument,
        verification_status: &HospitalVerificationStatus,
    ) -> Result<(), AdminHospitalError> {
        let message =
            hospital_document_review_email_message(hospital, document, verification_status);
        self.email_service.send(message).await.map_err(|error| {
            tracing::error!(%error, hospital_id = %hospital.id, document_id = %document.id, "failed to send hospital document review email");
            error.into()
        })
    }
}

fn validate_document_review_command(
    command: &ReviewHospitalDocumentCommand,
) -> Result<HospitalDocumentReview, AdminHospitalError> {
    let status = parse_document_review_status(&command.status)?;
    let review_message = match status {
        HospitalDocumentStatus::Approved => None,
        HospitalDocumentStatus::Rejected => {
            let message = command
                .message
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AdminHospitalError::Validation(
                        "message is required when rejecting a hospital document".to_owned(),
                    )
                })?;

            if message.chars().count() > MAX_REVIEW_MESSAGE_LENGTH {
                return Err(AdminHospitalError::Validation(format!(
                    "message cannot exceed {MAX_REVIEW_MESSAGE_LENGTH} characters"
                )));
            }

            Some(message.to_owned())
        }
        HospitalDocumentStatus::Pending => {
            return Err(AdminHospitalError::Validation(
                "status must be approved or rejected".to_owned(),
            ))
        }
    };

    Ok(HospitalDocumentReview {
        status,
        review_message,
    })
}

fn parse_document_review_status(value: &str) -> Result<HospitalDocumentStatus, AdminHospitalError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "approved" => Ok(HospitalDocumentStatus::Approved),
        "rejected" => Ok(HospitalDocumentStatus::Rejected),
        _ => Err(AdminHospitalError::Validation(
            "status must be approved or rejected".to_owned(),
        )),
    }
}

fn hospital_verification_status_after_document_review(
    documents: &[HospitalDocument],
) -> HospitalVerificationStatus {
    if required_document_status(documents, &HospitalDocumentType::CacCertificate)
        == Some(HospitalDocumentStatus::Rejected)
        || required_document_status(documents, &HospitalDocumentType::MedicalLicense)
            == Some(HospitalDocumentStatus::Rejected)
    {
        return HospitalVerificationStatus::Rejected;
    }

    let cac_approved = required_document_status(documents, &HospitalDocumentType::CacCertificate)
        == Some(HospitalDocumentStatus::Approved);
    let license_approved =
        required_document_status(documents, &HospitalDocumentType::MedicalLicense)
            == Some(HospitalDocumentStatus::Approved);

    if cac_approved && license_approved {
        HospitalVerificationStatus::Verified
    } else {
        HospitalVerificationStatus::Pending
    }
}

fn required_document_status(
    documents: &[HospitalDocument],
    document_type: &HospitalDocumentType,
) -> Option<HospitalDocumentStatus> {
    if documents.iter().any(|document| {
        &document.document_type == document_type
            && document.status == HospitalDocumentStatus::Rejected
    }) {
        return Some(HospitalDocumentStatus::Rejected);
    }

    if documents.iter().any(|document| {
        &document.document_type == document_type
            && document.status == HospitalDocumentStatus::Approved
    }) {
        return Some(HospitalDocumentStatus::Approved);
    }

    None
}

fn hospital_document_review_email_message(
    hospital: &Hospital,
    document: &HospitalDocument,
    verification_status: &HospitalVerificationStatus,
) -> EmailMessage {
    let document_name = hospital_document_type_label(&document.document_type);
    match document.status {
        HospitalDocumentStatus::Approved => {
            let verified_note = if verification_status == &HospitalVerificationStatus::Verified {
                "\n\nAll required verification documents have now been approved. Your hospital account is verified and has full dashboard access."
            } else {
                "\n\nWe will notify you once all required verification documents have been reviewed."
            };
            let html_verified_note = if verification_status == &HospitalVerificationStatus::Verified {
                "<p>All required verification documents have now been approved. Your hospital account is verified and has full dashboard access.</p>"
            } else {
                "<p>We will notify you once all required verification documents have been reviewed.</p>"
            };

            EmailMessage {
                to_email: hospital.email.clone(),
                to_name: hospital.administrator_name.clone(),
                subject: format!("Your {document_name} has been approved"),
                text_body: format!(
                    "Hello {},\n\nYour {} has been reviewed and approved.{}\n\nThank you,\nKorede Health",
                    hospital_display_name(hospital),
                    document_name,
                    verified_note
                ),
                html_body: Some(format!(
                    "<p>Hello {},</p><p>Your {} has been reviewed and approved.</p>{}<p>Thank you,<br>Korede Health</p>",
                    hospital_display_name(hospital),
                    document_name,
                    html_verified_note
                )),
            }
        }
        HospitalDocumentStatus::Rejected => {
            let reason = document.review_message.as_deref().unwrap_or(
                "Your document did not meet the verification requirements. Please contact support for details.",
            );
            EmailMessage {
                to_email: hospital.email.clone(),
                to_name: hospital.administrator_name.clone(),
                subject: format!("Your {document_name} was rejected"),
                text_body: format!(
                    "Hello {},\n\nYour {} has been reviewed and rejected.\n\nReason:\n{}\n\nPlease upload a corrected document or contact Korede Health support for help.\n\nThank you,\nKorede Health",
                    hospital_display_name(hospital),
                    document_name,
                    reason
                ),
                html_body: Some(format!(
                    "<p>Hello {},</p><p>Your {} has been reviewed and rejected.</p><p><strong>Reason:</strong><br>{}</p><p>Please upload a corrected document or contact Korede Health support for help.</p><p>Thank you,<br>Korede Health</p>",
                    hospital_display_name(hospital),
                    document_name,
                    reason
                )),
            }
        }
        HospitalDocumentStatus::Pending => EmailMessage {
            to_email: hospital.email.clone(),
            to_name: hospital.administrator_name.clone(),
            subject: format!("Your {document_name} is pending review"),
            text_body: format!(
                "Hello {},\n\nYour {} is still pending review.\n\nThank you,\nKorede Health",
                hospital_display_name(hospital),
                document_name
            ),
            html_body: Some(format!(
                "<p>Hello {},</p><p>Your {} is still pending review.</p><p>Thank you,<br>Korede Health</p>",
                hospital_display_name(hospital),
                document_name
            )),
        },
    }
}

fn hospital_document_type_label(document_type: &HospitalDocumentType) -> &'static str {
    match document_type {
        HospitalDocumentType::CacCertificate => "CAC certificate",
        HospitalDocumentType::MedicalLicense => "medical license",
    }
}

fn hospital_display_name(hospital: &Hospital) -> &str {
    hospital
        .administrator_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&hospital.name)
}

fn document_review_response_message(status: &HospitalDocumentStatus) -> &'static str {
    match status {
        HospitalDocumentStatus::Approved => "hospital document approved",
        HospitalDocumentStatus::Rejected => "hospital document rejected",
        HospitalDocumentStatus::Pending => "hospital document pending review",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::hospital_document::StorageProvider;
    use chrono::{TimeZone, Utc};

    #[test]
    fn document_review_validation_accepts_approval() {
        let review = validate_document_review_command(&ReviewHospitalDocumentCommand {
            status: "approved".to_owned(),
            message: Some("ignored for approvals".to_owned()),
        })
        .expect("approval should be valid");

        assert_eq!(review.status, HospitalDocumentStatus::Approved);
        assert!(review.review_message.is_none());
    }

    #[test]
    fn document_review_validation_requires_rejection_message() {
        let error = validate_document_review_command(&ReviewHospitalDocumentCommand {
            status: "rejected".to_owned(),
            message: Some("   ".to_owned()),
        })
        .expect_err("blank rejection reason should fail");

        assert!(matches!(error, AdminHospitalError::Validation(_)));
    }

    #[test]
    fn document_review_validation_rejects_invalid_status() {
        let error = validate_document_review_command(&ReviewHospitalDocumentCommand {
            status: "pending".to_owned(),
            message: None,
        })
        .expect_err("pending is not a review action");

        assert!(matches!(error, AdminHospitalError::Validation(_)));
    }

    #[test]
    fn document_review_validation_rejects_long_rejection_message() {
        let error = validate_document_review_command(&ReviewHospitalDocumentCommand {
            status: "rejected".to_owned(),
            message: Some("x".repeat(MAX_REVIEW_MESSAGE_LENGTH + 1)),
        })
        .expect_err("overlong rejection reason should fail");

        assert!(matches!(error, AdminHospitalError::Validation(_)));
    }

    #[test]
    fn document_review_verifies_hospital_when_required_documents_are_approved() {
        let hospital_id = Uuid::new_v4();
        let documents = vec![
            test_hospital_document(
                hospital_id,
                HospitalDocumentType::CacCertificate,
                HospitalDocumentStatus::Approved,
                None,
            ),
            test_hospital_document(
                hospital_id,
                HospitalDocumentType::MedicalLicense,
                HospitalDocumentStatus::Approved,
                None,
            ),
        ];

        assert_eq!(
            hospital_verification_status_after_document_review(&documents),
            HospitalVerificationStatus::Verified
        );
    }

    #[test]
    fn document_review_rejects_hospital_when_required_document_is_rejected() {
        let hospital_id = Uuid::new_v4();
        let documents = vec![
            test_hospital_document(
                hospital_id,
                HospitalDocumentType::CacCertificate,
                HospitalDocumentStatus::Approved,
                None,
            ),
            test_hospital_document(
                hospital_id,
                HospitalDocumentType::MedicalLicense,
                HospitalDocumentStatus::Rejected,
                Some("license expired".to_owned()),
            ),
        ];

        assert_eq!(
            hospital_verification_status_after_document_review(&documents),
            HospitalVerificationStatus::Rejected
        );
    }

    #[test]
    fn document_review_keeps_hospital_pending_until_all_required_documents_are_approved() {
        let hospital_id = Uuid::new_v4();
        let documents = vec![test_hospital_document(
            hospital_id,
            HospitalDocumentType::CacCertificate,
            HospitalDocumentStatus::Approved,
            None,
        )];

        assert_eq!(
            hospital_verification_status_after_document_review(&documents),
            HospitalVerificationStatus::Pending
        );
    }

    #[test]
    fn document_review_approval_email_mentions_document_and_verification() {
        let hospital = test_hospital();
        let document = test_hospital_document(
            hospital.id,
            HospitalDocumentType::CacCertificate,
            HospitalDocumentStatus::Approved,
            None,
        );

        let message = hospital_document_review_email_message(
            &hospital,
            &document,
            &HospitalVerificationStatus::Verified,
        );

        assert!(message.subject.contains("CAC certificate"));
        assert!(message.text_body.contains("approved"));
        assert!(message.text_body.contains("verified"));
    }

    #[test]
    fn document_review_rejection_email_includes_admin_message() {
        let hospital = test_hospital();
        let document = test_hospital_document(
            hospital.id,
            HospitalDocumentType::MedicalLicense,
            HospitalDocumentStatus::Rejected,
            Some("The license has expired.".to_owned()),
        );

        let message = hospital_document_review_email_message(
            &hospital,
            &document,
            &HospitalVerificationStatus::Rejected,
        );

        assert!(message.subject.contains("medical license"));
        assert!(message.text_body.contains("rejected"));
        assert!(message.text_body.contains("The license has expired."));
    }

    fn test_hospital() -> Hospital {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        Hospital {
            id: Uuid::new_v4(),
            name: "Lagoon Hospital".to_owned(),
            email: "admin@lagoon.example".to_owned(),
            email_verified: true,
            email_verified_at: Some(now),
            password_hash: "hash".to_owned(),
            phone_number: Some("+2348012345678".to_owned()),
            official_address: Some("1 Hospital Road".to_owned()),
            administrator_name: Some("Dr Jane".to_owned()),
            cac_registration_number: Some("RC123".to_owned()),
            medical_license_number: Some("ML123".to_owned()),
            corporate_account_name: "Lagoon Hospital".to_owned(),
            corporate_account_number: "0123456789".to_owned(),
            corporate_bank_code: Some("035".to_owned()),
            bank_name: "Wema Bank".to_owned(),
            verification_status: HospitalVerificationStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    fn test_hospital_document(
        hospital_id: Uuid,
        document_type: HospitalDocumentType,
        status: HospitalDocumentStatus,
        review_message: Option<String>,
    ) -> HospitalDocument {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        HospitalDocument {
            id: Uuid::new_v4(),
            hospital_id,
            document_type,
            storage_provider: StorageProvider::Local,
            storage_key: "hospital/doc.pdf".to_owned(),
            original_filename: "doc.pdf".to_owned(),
            mime_type: "application/pdf".to_owned(),
            file_size_bytes: 1024,
            status,
            uploaded_at: now,
            reviewed_at: Some(now),
            review_message,
        }
    }
}
