use std::sync::Arc;

use base64::{engine::general_purpose, Engine as _};
use chrono::NaiveDate;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::hospital::HospitalVerificationStatus,
    port::{
        email::{EmailError, EmailMessage, EmailService},
        hospital::{HospitalRepository, HospitalRepositoryError},
        medical_case::{
            CreatedMedicalCase, MedicalCaseRepository, MedicalCaseRepositoryError, NewMedicalCase,
            NewMedicalCaseBillingItem, NewMedicalCaseDocument,
        },
        patient::{PatientRepository, PatientRepositoryError},
        patient_declaration::{PatientDeclarationRepository, PatientDeclarationRepositoryError},
        storage::{DocumentStorage, DocumentStorageError},
    },
};

#[derive(Debug, Clone)]
pub struct CreateHospitalCaseCommand {
    pub patient_username: String,
    pub title: String,
    pub diagnosis_summary: String,
    pub admitted_at: Option<NaiveDate>,
    pub billing_items: Vec<BillingItemCommand>,
    pub documents: Vec<CaseDocumentCommand>,
}

#[derive(Debug, Clone)]
pub struct BillingItemCommand {
    pub description: String,
    pub amount_naira: i64,
}

#[derive(Debug, Clone)]
pub struct CaseDocumentCommand {
    pub document_type: String,
    pub original_filename: String,
    pub mime_type: String,
    pub content_base64: String,
}

#[derive(Debug, Error)]
pub enum HospitalCaseError {
    #[error("{0}")]
    Validation(String),

    #[error("hospital must be verified before creating medical cases")]
    HospitalNotVerified,

    #[error("patient not found")]
    PatientNotFound,

    #[error("patient email is required")]
    PatientEmailRequired,

    #[error("patient declaration is required before case creation")]
    PatientDeclarationRequired,

    #[error("patient already has an open medical case")]
    PatientHasOpenCase,

    #[error("hospital not found")]
    HospitalNotFound,

    #[error("hospital repository error: {0}")]
    HospitalRepository(#[from] HospitalRepositoryError),

    #[error("patient repository error: {0}")]
    PatientRepository(#[from] PatientRepositoryError),

    #[error("patient declaration repository error: {0}")]
    PatientDeclarationRepository(#[from] PatientDeclarationRepositoryError),

    #[error("medical case repository error: {0}")]
    MedicalCaseRepository(#[from] MedicalCaseRepositoryError),

    #[error("document storage error: {0}")]
    DocumentStorage(#[from] DocumentStorageError),

    #[error("email error: {0}")]
    Email(#[from] EmailError),
}

#[derive(Clone)]
pub struct HospitalCaseService {
    hospital_repository: Arc<dyn HospitalRepository>,
    medical_case_repository: Arc<dyn MedicalCaseRepository>,
    patient_repository: Arc<dyn PatientRepository>,
    patient_declaration_repository: Arc<dyn PatientDeclarationRepository>,
    document_storage: Arc<dyn DocumentStorage>,
    email_service: Arc<dyn EmailService>,
    max_upload_bytes: usize,
}

impl HospitalCaseService {
    pub fn new(
        hospital_repository: Arc<dyn HospitalRepository>,
        medical_case_repository: Arc<dyn MedicalCaseRepository>,
        patient_repository: Arc<dyn PatientRepository>,
        patient_declaration_repository: Arc<dyn PatientDeclarationRepository>,
        document_storage: Arc<dyn DocumentStorage>,
        email_service: Arc<dyn EmailService>,
        max_upload_bytes: usize,
    ) -> Self {
        Self {
            hospital_repository,
            medical_case_repository,
            patient_repository,
            patient_declaration_repository,
            document_storage,
            email_service,
            max_upload_bytes,
        }
    }

    pub async fn list_active_cases(
        &self,
        hospital_id: Uuid,
    ) -> Result<Vec<crate::port::medical_case::HospitalMedicalCaseSummary>, HospitalCaseError> {
        Ok(self
            .medical_case_repository
            .list_hospital_active_cases(hospital_id)
            .await?)
    }

    pub async fn list_completed_cases(
        &self,
        hospital_id: Uuid,
    ) -> Result<Vec<crate::port::medical_case::HospitalMedicalCaseSummary>, HospitalCaseError> {
        Ok(self
            .medical_case_repository
            .list_hospital_completed_cases(hospital_id)
            .await?)
    }

    pub async fn create_case(
        &self,
        hospital_id: Uuid,
        command: CreateHospitalCaseCommand,
    ) -> Result<CreatedMedicalCase, HospitalCaseError> {
        validate_create_case_command(&command)?;

        let hospital = self
            .hospital_repository
            .find_hospital_by_id(hospital_id)
            .await?
            .ok_or(HospitalCaseError::HospitalNotFound)?;

        if hospital.verification_status != HospitalVerificationStatus::Verified {
            return Err(HospitalCaseError::HospitalNotVerified);
        }

        let patient = self
            .patient_repository
            .find_patient_by_username(command.patient_username.trim())
            .await?
            .ok_or(HospitalCaseError::PatientNotFound)?;

        if self
            .medical_case_repository
            .patient_has_open_case(patient.id)
            .await?
        {
            return Err(HospitalCaseError::PatientHasOpenCase);
        }

        let patient_email = patient
            .email
            .clone()
            .ok_or(HospitalCaseError::PatientEmailRequired)?;

        let declaration = self
            .patient_declaration_repository
            .find_current_patient_declaration(patient.id)
            .await?
            .ok_or(HospitalCaseError::PatientDeclarationRequired)?;

        let case_id = Uuid::new_v4();
        let public_slug =
            generate_case_public_slug(&command.patient_username, &command.title, case_id);

        let mut stored_documents = Vec::with_capacity(command.documents.len());
        for document in &command.documents {
            let contents = decode_base64_document(&document.content_base64, self.max_upload_bytes)?;
            let mime_type = normalized_document_mime_type(&document.mime_type)?;
            let stored = self
                .document_storage
                .save_case_document(
                    hospital_id,
                    case_id,
                    document.document_type.trim(),
                    document.original_filename.trim(),
                    mime_type,
                    &contents,
                )
                .await?;

            stored_documents.push(NewMedicalCaseDocument {
                document_type: document.document_type.trim().to_owned(),
                storage_provider: stored.storage_provider,
                storage_key: stored.storage_key,
                original_filename: stored.original_filename,
                mime_type: stored.mime_type,
                file_size_bytes: stored.file_size_bytes,
            });
        }

        let billing_items = command
            .billing_items
            .iter()
            .map(|item| {
                Ok(NewMedicalCaseBillingItem {
                    description: item.description.trim().to_owned(),
                    amount_kobo: naira_to_kobo(item.amount_naira, "billing item amount")?,
                })
            })
            .collect::<Result<Vec<_>, HospitalCaseError>>()?;

        let bill_amount_kobo = billing_items.iter().try_fold(0_i64, |total, item| {
            total.checked_add(item.amount_kobo).ok_or_else(|| {
                HospitalCaseError::Validation("billing total is too large".to_owned())
            })
        })?;

        let created = self
            .medical_case_repository
            .create_published_case(
                NewMedicalCase {
                    id: case_id,
                    hospital_id,
                    patient_id: patient.id,
                    patient_declaration_id: declaration.id,
                    patient_declaration_statement: declaration.statement.clone(),
                    title: command.title.trim().to_owned(),
                    public_slug,
                    diagnosis_summary: command.diagnosis_summary.trim().to_owned(),
                    bill_amount_kobo,
                    admitted_at: command.admitted_at,
                },
                billing_items,
                stored_documents,
            )
            .await?;

        self.send_patient_case_created_email(
            &patient_email,
            &patient.full_name,
            &hospital.name,
            &created.case.title,
            created.case.bill_amount_kobo,
            created.case.public_slug.as_deref().unwrap_or_default(),
        )
        .await?;

        Ok(created)
    }

    async fn send_patient_case_created_email(
        &self,
        patient_email: &str,
        patient_name: &str,
        hospital_name: &str,
        case_title: &str,
        bill_amount_kobo: i64,
        public_slug: &str,
    ) -> Result<(), HospitalCaseError> {
        let public_link = format!("/cases/{public_slug}");
        let amount = format_ngn_amount(bill_amount_kobo);
        let subject = "A hospital medical case has been created for you".to_owned();
        let text_body = format!(
            "Hello {},\n\n{} has created a medical case for you on Korede Health.\n\nCase: {}\nAmount: {}\nLink: {}\n\nThank you,\nKorede Health",
            patient_name.trim(),
            hospital_name.trim(),
            case_title.trim(),
            amount,
            public_link
        );
        let html_body = format!(
            "<p>Hello {},</p><p>{} has created a medical case for you on Korede Health.</p><p><strong>Case:</strong> {}</p><p><strong>Amount:</strong> {}</p><p><strong>Link:</strong> {}</p><p>Thank you,<br>Korede Health</p>",
            patient_name.trim(),
            hospital_name.trim(),
            case_title.trim(),
            amount,
            public_link
        );

        self.email_service
            .send(EmailMessage {
                to_email: patient_email.to_owned(),
                to_name: Some(patient_name.trim().to_owned()),
                subject,
                text_body,
                html_body: Some(html_body),
            })
            .await?;

        Ok(())
    }
}

fn validate_create_case_command(
    command: &CreateHospitalCaseCommand,
) -> Result<(), HospitalCaseError> {
    if command.patient_username.trim().is_empty()
        || command.title.trim().is_empty()
        || command.diagnosis_summary.trim().is_empty()
    {
        return Err(HospitalCaseError::Validation(
            "required fields are missing".to_owned(),
        ));
    }

    if command.billing_items.is_empty() {
        return Err(HospitalCaseError::Validation(
            "at least one billing item is required".to_owned(),
        ));
    }

    for item in &command.billing_items {
        if item.description.trim().is_empty() {
            return Err(HospitalCaseError::Validation(
                "billing item description is required".to_owned(),
            ));
        }
        if item.amount_naira <= 0 {
            return Err(HospitalCaseError::Validation(
                "billing item amount must be greater than zero".to_owned(),
            ));
        }
    }

    for document in &command.documents {
        if document.document_type.trim().is_empty()
            || document.original_filename.trim().is_empty()
            || document.mime_type.trim().is_empty()
            || document.content_base64.trim().is_empty()
        {
            return Err(HospitalCaseError::Validation(
                "document fields are required".to_owned(),
            ));
        }
        normalized_document_mime_type(&document.mime_type)?;
    }

    Ok(())
}

fn naira_to_kobo(amount: i64, field_name: &str) -> Result<i64, HospitalCaseError> {
    if amount <= 0 {
        return Err(HospitalCaseError::Validation(format!(
            "{field_name} must be greater than zero"
        )));
    }
    amount
        .checked_mul(100)
        .ok_or_else(|| HospitalCaseError::Validation(format!("{field_name} is too large")))
}

fn decode_base64_document(
    content_base64: &str,
    max_upload_bytes: usize,
) -> Result<Vec<u8>, HospitalCaseError> {
    let encoded = content_base64.trim();
    let encoded = encoded
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(encoded);
    let contents = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| HospitalCaseError::Validation("document base64 is invalid".to_owned()))?;
    if contents.is_empty() {
        return Err(HospitalCaseError::Validation(
            "document content cannot be empty".to_owned(),
        ));
    }
    if contents.len() > max_upload_bytes {
        return Err(HospitalCaseError::Validation(
            "uploaded document is too large".to_owned(),
        ));
    }
    Ok(contents)
}

fn normalized_document_mime_type(mime_type: &str) -> Result<&'static str, HospitalCaseError> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "application/pdf" | "application/x-pdf" | "pdf" => Ok("application/pdf"),
        "image/jpeg" | "image/jpg" | "jpeg" | "jpg" => Ok("image/jpeg"),
        "image/png" | "png" => Ok("image/png"),
        "image/webp" | "webp" => Ok("image/webp"),
        _ => Err(HospitalCaseError::Validation(
            "only PDF, JPEG, PNG, and WebP files are supported".to_owned(),
        )),
    }
}

fn generate_case_public_slug(patient_username: &str, title: &str, case_id: Uuid) -> String {
    let prefix_source = format!("{} {}", patient_username.trim(), title.trim());
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in prefix_source.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if (character.is_whitespace() || character == '-' || character == '_')
            && !slug.is_empty()
            && !last_was_separator
        {
            slug.push('-');
            last_was_separator = true;
        }
        if slug.len() >= 80 {
            break;
        }
    }

    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "case" } else { slug };
    let unique_suffix = case_id.simple().to_string();
    format!("{}-{}", slug, &unique_suffix[..8])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_case_input() {
        let command = CreateHospitalCaseCommand {
            patient_username: String::new(),
            title: "title".to_owned(),
            diagnosis_summary: "summary".to_owned(),
            admitted_at: None,
            billing_items: vec![],
            documents: vec![],
        };
        assert!(matches!(
            validate_create_case_command(&command),
            Err(HospitalCaseError::Validation(message)) if message == "required fields are missing"
        ));
    }

    #[test]
    fn converts_naira_to_kobo() {
        assert_eq!(naira_to_kobo(700, "amount").unwrap(), 70_000);
    }

    #[test]
    fn generates_unique_public_slug() {
        let case_id = Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap();
        assert_eq!(
            generate_case_public_slug("Andrew", "Medical Treatment", case_id),
            "andrew-medical-treatment-12345678"
        );
    }
}
