use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::{
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::{
            HospitalDocument, HospitalDocumentStatus, HospitalDocumentType, StorageProvider,
        },
    },
    port::hospital::{
        HospitalRepository, HospitalRepositoryError, NewHospital, NewHospitalDocument,
    },
};

#[derive(Debug, Clone)]
pub struct PostgresHospitalRepository {
    pool: PgPool,
}

impl PostgresHospitalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HospitalRepository for PostgresHospitalRepository {
    async fn create_hospital(
        &self,
        hospital: NewHospital,
    ) -> Result<Hospital, HospitalRepositoryError> {
        let id = Uuid::new_v4();

        let result = sqlx::query(
            r#"
            INSERT INTO hospitals (
                id,
                name,
                email,
                password_hash,
                phone_number,
                cac_registration_number,
                medical_license_number,
                corporate_account_name,
                corporate_account_number,
                bank_name,
                verification_status,
                created_at,
                updated_at
            )
            VALUES ($1, $2, LOWER($3), $4, $5, $6, $7, $8, $9, $10, 'pending', NOW(), NOW())
            RETURNING
                id,
                name,
                email,
                password_hash,
                phone_number,
                cac_registration_number,
                medical_license_number,
                corporate_account_name,
                corporate_account_number,
                bank_name,
                verification_status,
                created_at,
                updated_at
            "#,
        )
        .bind(id)
        .bind(hospital.name)
        .bind(hospital.email)
        .bind(hospital.password_hash)
        .bind(hospital.phone_number)
        .bind(hospital.cac_registration_number)
        .bind(hospital.medical_license_number)
        .bind(hospital.corporate_account_name)
        .bind(hospital.corporate_account_number)
        .bind(hospital.bank_name)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(row) => hospital_from_row(&row).map_err(HospitalRepositoryError::Database),
            Err(error) if is_unique_violation(&error) => {
                Err(HospitalRepositoryError::DuplicateEmail)
            }
            Err(error) => Err(HospitalRepositoryError::Database(error)),
        }
    }

    async fn find_hospital_by_email(
        &self,
        email: &str,
    ) -> Result<Option<Hospital>, HospitalRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                name,
                email,
                password_hash,
                phone_number,
                cac_registration_number,
                medical_license_number,
                corporate_account_name,
                corporate_account_number,
                bank_name,
                verification_status,
                created_at,
                updated_at
            FROM hospitals
            WHERE LOWER(email) = LOWER($1)
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| hospital_from_row(&row))
            .transpose()
            .map_err(HospitalRepositoryError::Database)
    }

    async fn find_hospital_by_id(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<Hospital>, HospitalRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                name,
                email,
                password_hash,
                phone_number,
                cac_registration_number,
                medical_license_number,
                corporate_account_name,
                corporate_account_number,
                bank_name,
                verification_status,
                created_at,
                updated_at
            FROM hospitals
            WHERE id = $1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| hospital_from_row(&row))
            .transpose()
            .map_err(HospitalRepositoryError::Database)
    }

    async fn save_hospital_document(
        &self,
        document: NewHospitalDocument,
    ) -> Result<HospitalDocument, HospitalRepositoryError> {
        let id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO hospital_documents (
                id,
                hospital_id,
                document_type,
                storage_provider,
                storage_key,
                original_filename,
                mime_type,
                file_size_bytes,
                status,
                uploaded_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', NOW())
            RETURNING
                id,
                hospital_id,
                document_type,
                storage_provider,
                storage_key,
                original_filename,
                mime_type,
                file_size_bytes,
                status,
                uploaded_at,
                reviewed_at
            "#,
        )
        .bind(id)
        .bind(document.hospital_id)
        .bind(document.document_type.as_str())
        .bind(document.storage_provider)
        .bind(document.storage_key)
        .bind(document.original_filename)
        .bind(document.mime_type)
        .bind(document.file_size_bytes)
        .fetch_one(&self.pool)
        .await?;

        document_from_row(&row).map_err(HospitalRepositoryError::Database)
    }

    async fn list_hospital_documents(
        &self,
        hospital_id: Uuid,
    ) -> Result<Vec<HospitalDocument>, HospitalRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id,
                hospital_id,
                document_type,
                storage_provider,
                storage_key,
                original_filename,
                mime_type,
                file_size_bytes,
                status,
                uploaded_at,
                reviewed_at
            FROM hospital_documents
            WHERE hospital_id = $1
            ORDER BY uploaded_at DESC
            "#,
        )
        .bind(hospital_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(document_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(HospitalRepositoryError::Database)
    }
}

fn hospital_from_row(row: &sqlx::postgres::PgRow) -> Result<Hospital, sqlx::Error> {
    Ok(Hospital {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        email: row.try_get("email")?,
        password_hash: row.try_get("password_hash")?,
        phone_number: row.try_get("phone_number")?,
        cac_registration_number: row.try_get("cac_registration_number")?,
        medical_license_number: row.try_get("medical_license_number")?,
        corporate_account_name: row.try_get("corporate_account_name")?,
        corporate_account_number: row.try_get("corporate_account_number")?,
        bank_name: row.try_get("bank_name")?,
        verification_status: verification_status_from_str(row.try_get("verification_status")?),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn document_from_row(row: &sqlx::postgres::PgRow) -> Result<HospitalDocument, sqlx::Error> {
    Ok(HospitalDocument {
        id: row.try_get("id")?,
        hospital_id: row.try_get("hospital_id")?,
        document_type: document_type_from_str(row.try_get("document_type")?),
        storage_provider: storage_provider_from_str(row.try_get("storage_provider")?),
        storage_key: row.try_get("storage_key")?,
        original_filename: row.try_get("original_filename")?,
        mime_type: row.try_get("mime_type")?,
        file_size_bytes: row.try_get("file_size_bytes")?,
        status: document_status_from_str(row.try_get("status")?),
        uploaded_at: row.try_get::<DateTime<Utc>, _>("uploaded_at")?,
        reviewed_at: row.try_get("reviewed_at")?,
    })
}

fn verification_status_from_str(value: &str) -> HospitalVerificationStatus {
    match value {
        "verified" => HospitalVerificationStatus::Verified,
        "rejected" => HospitalVerificationStatus::Rejected,
        "suspended" => HospitalVerificationStatus::Suspended,
        _ => HospitalVerificationStatus::Pending,
    }
}

fn document_type_from_str(value: &str) -> HospitalDocumentType {
    match value {
        "medical_license" => HospitalDocumentType::MedicalLicense,
        _ => HospitalDocumentType::CacCertificate,
    }
}

fn document_status_from_str(value: &str) -> HospitalDocumentStatus {
    match value {
        "approved" => HospitalDocumentStatus::Approved,
        "rejected" => HospitalDocumentStatus::Rejected,
        _ => HospitalDocumentStatus::Pending,
    }
}

fn storage_provider_from_str(value: &str) -> StorageProvider {
    match value {
        "s3" => StorageProvider::S3,
        _ => StorageProvider::Local,
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| code == "23505")
}
