use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::{
        medical_case::{MedicalCase, MedicalCaseStatus},
        medical_case_billing_item::MedicalCaseBillingItem,
        medical_case_document::MedicalCaseDocument,
    },
    port::medical_case::{
        CreatedMedicalCase, MedicalCaseRepository, MedicalCaseRepositoryError, NewMedicalCase,
        NewMedicalCaseBillingItem, NewMedicalCaseDocument,
    },
};

#[derive(Debug, Clone)]
pub struct PostgresMedicalCaseRepository {
    pool: PgPool,
}

impl PostgresMedicalCaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MedicalCaseRepository for PostgresMedicalCaseRepository {
    async fn create_published_case(
        &self,
        medical_case: NewMedicalCase,
        billing_items: Vec<NewMedicalCaseBillingItem>,
        documents: Vec<NewMedicalCaseDocument>,
    ) -> Result<CreatedMedicalCase, MedicalCaseRepositoryError> {
        let mut transaction = self.pool.begin().await?;
        let case_id = medical_case.id;

        let case_row = sqlx::query(
            r#"
            INSERT INTO medical_cases (
                id,
                hospital_id,
                patient_id,
                title,
                diagnosis_summary,
                bill_amount_kobo,
                amount_raised_kobo,
                status,
                admitted_at,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $8, NOW(), NOW())
            RETURNING
                id,
                hospital_id,
                patient_id,
                title,
                diagnosis_summary,
                bill_amount_kobo,
                amount_raised_kobo,
                status,
                blockchain_network,
                blockchain_tx_digest,
                blockchain_record_id,
                admitted_at,
                created_at,
                updated_at
            "#,
        )
        .bind(case_id)
        .bind(medical_case.hospital_id)
        .bind(medical_case.patient_id)
        .bind(medical_case.title)
        .bind(medical_case.diagnosis_summary)
        .bind(medical_case.bill_amount_kobo)
        .bind(MedicalCaseStatus::Active.as_str())
        .bind(medical_case.admitted_at)
        .fetch_one(&mut *transaction)
        .await?;

        let mut created_billing_items = Vec::with_capacity(billing_items.len());
        for item in billing_items {
            let item_id = Uuid::new_v4();
            let row = sqlx::query(
                r#"
                INSERT INTO medical_case_billing_items (
                    id,
                    medical_case_id,
                    description,
                    amount_kobo,
                    created_at
                )
                VALUES ($1, $2, $3, $4, NOW())
                RETURNING
                    id,
                    medical_case_id,
                    description,
                    amount_kobo,
                    created_at
                "#,
            )
            .bind(item_id)
            .bind(case_id)
            .bind(item.description)
            .bind(item.amount_kobo)
            .fetch_one(&mut *transaction)
            .await?;

            created_billing_items.push(billing_item_from_row(&row)?);
        }

        let mut created_documents = Vec::with_capacity(documents.len());
        for document in documents {
            let document_id = Uuid::new_v4();
            let row = sqlx::query(
                r#"
                INSERT INTO medical_case_documents (
                    id,
                    medical_case_id,
                    hospital_id,
                    document_type,
                    storage_provider,
                    storage_key,
                    original_filename,
                    mime_type,
                    file_size_bytes,
                    uploaded_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
                RETURNING
                    id,
                    medical_case_id,
                    hospital_id,
                    document_type,
                    storage_provider,
                    storage_key,
                    original_filename,
                    mime_type,
                    file_size_bytes,
                    uploaded_at
                "#,
            )
            .bind(document_id)
            .bind(case_id)
            .bind(medical_case.hospital_id)
            .bind(document.document_type)
            .bind(document.storage_provider)
            .bind(document.storage_key)
            .bind(document.original_filename)
            .bind(document.mime_type)
            .bind(document.file_size_bytes)
            .fetch_one(&mut *transaction)
            .await?;

            created_documents.push(document_from_row(&row)?);
        }

        transaction.commit().await?;

        Ok(CreatedMedicalCase {
            case: medical_case_from_row(&case_row)?,
            billing_items: created_billing_items,
            documents: created_documents,
        })
    }
}

fn medical_case_from_row(row: &sqlx::postgres::PgRow) -> Result<MedicalCase, sqlx::Error> {
    let status: String = row.try_get("status")?;

    Ok(MedicalCase {
        id: row.try_get("id")?,
        hospital_id: row.try_get("hospital_id")?,
        patient_id: row.try_get("patient_id")?,
        title: row.try_get("title")?,
        diagnosis_summary: row.try_get("diagnosis_summary")?,
        bill_amount_kobo: row.try_get("bill_amount_kobo")?,
        amount_raised_kobo: row.try_get("amount_raised_kobo")?,
        status: MedicalCaseStatus::from_str(&status),
        admitted_at: row.try_get::<Option<NaiveDate>, _>("admitted_at")?,
        blockchain_network: row.try_get("blockchain_network")?,
        blockchain_tx_digest: row.try_get("blockchain_tx_digest")?,
        blockchain_record_id: row.try_get("blockchain_record_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

fn billing_item_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<MedicalCaseBillingItem, sqlx::Error> {
    Ok(MedicalCaseBillingItem {
        id: row.try_get("id")?,
        medical_case_id: row.try_get("medical_case_id")?,
        description: row.try_get("description")?,
        amount_kobo: row.try_get("amount_kobo")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
    })
}

fn document_from_row(row: &sqlx::postgres::PgRow) -> Result<MedicalCaseDocument, sqlx::Error> {
    Ok(MedicalCaseDocument {
        id: row.try_get("id")?,
        medical_case_id: row.try_get("medical_case_id")?,
        hospital_id: row.try_get("hospital_id")?,
        document_type: row.try_get("document_type")?,
        storage_provider: row.try_get("storage_provider")?,
        storage_key: row.try_get("storage_key")?,
        original_filename: row.try_get("original_filename")?,
        mime_type: row.try_get("mime_type")?,
        file_size_bytes: row.try_get("file_size_bytes")?,
        uploaded_at: row.try_get::<DateTime<Utc>, _>("uploaded_at")?,
    })
}
