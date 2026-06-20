use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::{
        donation::{Donation, DonationProofStatus, DonationStatus},
        medical_case::{MedicalCase, MedicalCaseStatus},
        public_case::PublicCaseDetails,
    },
    port::donation::{
        DonationCaseLock, DonationFailureUpdate, DonationPaymentUpdate, DonationRepository,
        DonationRepositoryError, NewDonation,
    },
};

#[derive(Debug, Clone)]
pub struct PostgresDonationRepository {
    pool: PgPool,
}

impl PostgresDonationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DonationRepository for PostgresDonationRepository {
    async fn create_pending_donation(
        &self,
        donation: NewDonation,
    ) -> Result<Donation, DonationRepositoryError> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO case_donations (
                id,
                medical_case_id,
                donor_display_name,
                donor_email,
                amount_kobo,
                paystack_reference,
                paystack_transaction_reference,
                paystack_access_code,
                paystack_authorization_url,
                paystack_customer_code,
                paystack_dedicated_account_id,
                paystack_dedicated_account_number,
                paystack_dedicated_account_name,
                paystack_dedicated_bank_name,
                paystack_dedicated_bank_slug,
                status,
                paid_at,
                proof_status,
                sui_network,
                sui_tx_digest,
                created_at,
                updated_at
            )
            VALUES (
                $1, $2, $3, LOWER($4), $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                'pending', NULL, 'pending', NULL, NULL, NOW(), NOW()
            )
            RETURNING
                id,
                medical_case_id,
                donor_display_name,
                donor_email,
                amount_kobo,
                paystack_reference,
                paystack_transaction_reference,
                paystack_access_code,
                paystack_authorization_url,
                paystack_customer_code,
                paystack_dedicated_account_id,
                paystack_dedicated_account_number,
                paystack_dedicated_account_name,
                paystack_dedicated_bank_name,
                paystack_dedicated_bank_slug,
                status,
                paid_at,
                proof_status,
                sui_network,
                sui_tx_digest,
                created_at,
                updated_at
            "#,
        )
        .bind(id)
        .bind(donation.medical_case_id)
        .bind(donation.donor_display_name)
        .bind(donation.donor_email)
        .bind(donation.amount_kobo)
        .bind(donation.paystack_reference)
        .bind(donation.paystack_transaction_reference)
        .bind(donation.paystack_access_code)
        .bind(donation.paystack_authorization_url)
        .bind(donation.paystack_customer_code)
        .bind(donation.paystack_dedicated_account_id)
        .bind(donation.paystack_dedicated_account_number)
        .bind(donation.paystack_dedicated_account_name)
        .bind(donation.paystack_dedicated_bank_name)
        .bind(donation.paystack_dedicated_bank_slug)
        .fetch_one(&self.pool)
        .await
        .map_err(map_donation_error)?;

        donation_from_row(&row).map_err(DonationRepositoryError::Database)
    }

    async fn find_donation_by_reference(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<Donation>, DonationRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                medical_case_id,
                donor_display_name,
                donor_email,
                amount_kobo,
                paystack_reference,
                paystack_transaction_reference,
                paystack_access_code,
                paystack_authorization_url,
                paystack_customer_code,
                paystack_dedicated_account_id,
                paystack_dedicated_account_number,
                paystack_dedicated_account_name,
                paystack_dedicated_bank_name,
                paystack_dedicated_bank_slug,
                status,
                paid_at,
                proof_status,
                sui_network,
                sui_tx_digest,
                created_at,
                updated_at
            FROM case_donations
            WHERE paystack_reference = $1
            "#,
        )
        .bind(paystack_reference)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| donation_from_row(&row))
            .transpose()
            .map_err(DonationRepositoryError::Database)
    }

    async fn get_public_case_details(
        &self,
        public_slug: &str,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError> {
        let case_row = sqlx::query(
            r#"
            SELECT
                id,
                hospital_id,
                patient_id,
                title,
                public_slug,
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
            FROM medical_cases
            WHERE public_slug = $1
            LIMIT 1
            "#,
        )
        .bind(public_slug)
        .fetch_optional(&self.pool)
        .await?;

        let Some(case_row) = case_row else {
            return Ok(None);
        };

        let medical_case =
            medical_case_from_row(&case_row).map_err(DonationRepositoryError::Database)?;
        let donation_rows = sqlx::query(
            r#"
            SELECT
                id,
                medical_case_id,
                donor_display_name,
                donor_email,
                amount_kobo,
                paystack_reference,
                paystack_transaction_reference,
                paystack_access_code,
                paystack_authorization_url,
                paystack_customer_code,
                paystack_dedicated_account_id,
                paystack_dedicated_account_number,
                paystack_dedicated_account_name,
                paystack_dedicated_bank_name,
                paystack_dedicated_bank_slug,
                status,
                paid_at,
                proof_status,
                sui_network,
                sui_tx_digest,
                created_at,
                updated_at
            FROM case_donations
            WHERE medical_case_id = $1
              AND status = 'paid'
            ORDER BY paid_at DESC, created_at DESC
            "#,
        )
        .bind(medical_case.id)
        .fetch_all(&self.pool)
        .await?;

        let donations = donation_rows
            .iter()
            .map(donation_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(DonationRepositoryError::Database)?;

        Ok(Some(PublicCaseDetails {
            medical_case,
            donations,
        }))
    }

    async fn lock_pending_donation_for_confirmation(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<DonationCaseLock>, DonationRepositoryError> {
        lock_donation_case(
            &self.pool,
            "WHERE paystack_reference = $1",
            paystack_reference,
        )
        .await
    }

    async fn lock_pending_donation_by_account_number(
        &self,
        account_number: &str,
    ) -> Result<Option<DonationCaseLock>, DonationRepositoryError> {
        lock_donation_case(
            &self.pool,
            "WHERE paystack_dedicated_account_number = $1",
            account_number,
        )
        .await
    }

    async fn mark_donation_paid(
        &self,
        update: DonationPaymentUpdate,
    ) -> Result<Donation, DonationRepositoryError> {
        let mut transaction = self.pool.begin().await?;

        let donation_row = sqlx::query(
            r#"
            SELECT
                id,
                medical_case_id,
                donor_display_name,
                donor_email,
                amount_kobo,
                paystack_reference,
                paystack_transaction_reference,
                paystack_access_code,
                paystack_authorization_url,
                paystack_customer_code,
                paystack_dedicated_account_id,
                paystack_dedicated_account_number,
                paystack_dedicated_account_name,
                paystack_dedicated_bank_name,
                paystack_dedicated_bank_slug,
                status,
                paid_at,
                proof_status,
                sui_network,
                sui_tx_digest,
                created_at,
                updated_at
            FROM case_donations
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(update.donation_id)
        .fetch_optional(&mut *transaction)
        .await?;

        let Some(donation_row) = donation_row else {
            transaction.commit().await?;
            return Err(DonationRepositoryError::NotFound);
        };

        let current_donation =
            donation_from_row(&donation_row).map_err(DonationRepositoryError::Database)?;

        if current_donation.status == DonationStatus::Paid {
            transaction.commit().await?;
            return Ok(current_donation);
        }

        let (bill_amount_kobo, amount_raised_kobo) = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT bill_amount_kobo, amount_raised_kobo
            FROM medical_cases
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(current_donation.medical_case_id)
        .fetch_one(&mut *transaction)
        .await?;

        let remaining_amount_kobo = (bill_amount_kobo - amount_raised_kobo).max(0);
        if current_donation.amount_kobo > remaining_amount_kobo {
            sqlx::query(
                r#"
                UPDATE case_donations
                SET status = 'rejected_overflow',
                    paystack_transaction_reference = COALESCE(paystack_transaction_reference, $2),
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(update.donation_id)
            .bind(&update.paystack_transaction_reference)
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;
            return Err(DonationRepositoryError::AmountExceedsRemaining);
        }

        sqlx::query(
            r#"
            UPDATE medical_cases
            SET amount_raised_kobo = amount_raised_kobo + $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(current_donation.medical_case_id)
        .bind(current_donation.amount_kobo)
        .execute(&mut *transaction)
        .await?;

        let row = sqlx::query(
            r#"
            UPDATE case_donations
            SET status = 'paid',
                paystack_transaction_reference = $2,
                paid_at = $3,
                proof_status = $4,
                sui_network = $5,
                sui_tx_digest = $6,
                updated_at = NOW()
            WHERE id = $1
            RETURNING
                id,
                medical_case_id,
                donor_display_name,
                donor_email,
                amount_kobo,
                paystack_reference,
                paystack_transaction_reference,
                paystack_access_code,
                paystack_authorization_url,
                paystack_customer_code,
                paystack_dedicated_account_id,
                paystack_dedicated_account_number,
                paystack_dedicated_account_name,
                paystack_dedicated_bank_name,
                paystack_dedicated_bank_slug,
                status,
                paid_at,
                proof_status,
                sui_network,
                sui_tx_digest,
                created_at,
                updated_at
            "#,
        )
        .bind(update.donation_id)
        .bind(update.paystack_transaction_reference)
        .bind(update.paid_at)
        .bind(update.proof_status.as_str())
        .bind(update.sui_network)
        .bind(update.sui_tx_digest)
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;

        donation_from_row(&row).map_err(DonationRepositoryError::Database)
    }

    async fn mark_donation_failed(
        &self,
        update: DonationFailureUpdate,
    ) -> Result<(), DonationRepositoryError> {
        let result = sqlx::query(
            r#"
            UPDATE case_donations
            SET status = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(update.donation_id)
        .bind(update.status.as_str())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DonationRepositoryError::NotFound);
        }

        Ok(())
    }
}

async fn lock_donation_case(
    pool: &PgPool,
    where_clause: &str,
    lookup_value: &str,
) -> Result<Option<DonationCaseLock>, DonationRepositoryError> {
    let mut transaction = pool.begin().await?;
    let query = format!(
        r#"
        SELECT
            id,
            medical_case_id,
            donor_display_name,
            donor_email,
            amount_kobo,
            paystack_reference,
            paystack_transaction_reference,
            paystack_access_code,
            paystack_authorization_url,
            paystack_customer_code,
            paystack_dedicated_account_id,
            paystack_dedicated_account_number,
            paystack_dedicated_account_name,
            paystack_dedicated_bank_name,
            paystack_dedicated_bank_slug,
            status,
            paid_at,
            proof_status,
            sui_network,
            sui_tx_digest,
            created_at,
            updated_at
        FROM case_donations
        {where_clause}
        FOR UPDATE
        "#
    );

    let donation_row = sqlx::query(&query)
        .bind(lookup_value)
        .fetch_optional(&mut *transaction)
        .await?;

    let Some(donation_row) = donation_row else {
        transaction.commit().await?;
        return Ok(None);
    };

    let donation = donation_from_row(&donation_row).map_err(DonationRepositoryError::Database)?;

    let case_row = sqlx::query(
        r#"
        SELECT
            id,
            hospital_id,
            patient_id,
            title,
            public_slug,
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
        FROM medical_cases
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(donation.medical_case_id)
    .fetch_one(&mut *transaction)
    .await?;

    let medical_case = medical_case_from_row(&case_row).map_err(DonationRepositoryError::Database)?;

    transaction.commit().await?;

    Ok(Some(DonationCaseLock {
        donation,
        remaining_amount_kobo: (medical_case.bill_amount_kobo - medical_case.amount_raised_kobo)
            .max(0),
        medical_case,
    }))
}

fn map_donation_error(error: sqlx::Error) -> DonationRepositoryError {
    if let Some(constraint) = error
        .as_database_error()
        .and_then(|database_error| database_error.constraint())
    {
        if constraint == "case_donations_paystack_reference_key"
            || constraint == "idx_case_donations_dedicated_account_number"
        {
            return DonationRepositoryError::DuplicateReference;
        }
    }

    DonationRepositoryError::Database(error)
}

fn donation_from_row(row: &sqlx::postgres::PgRow) -> Result<Donation, sqlx::Error> {
    let status: String = row.try_get("status")?;
    let proof_status: String = row.try_get("proof_status")?;

    Ok(Donation {
        id: row.try_get("id")?,
        medical_case_id: row.try_get("medical_case_id")?,
        donor_display_name: row.try_get("donor_display_name")?,
        donor_email: row.try_get("donor_email")?,
        amount_kobo: row.try_get("amount_kobo")?,
        paystack_reference: row.try_get("paystack_reference")?,
        paystack_transaction_reference: row.try_get("paystack_transaction_reference")?,
        paystack_access_code: row.try_get("paystack_access_code")?,
        paystack_authorization_url: row.try_get("paystack_authorization_url")?,
        paystack_customer_code: row.try_get("paystack_customer_code")?,
        paystack_dedicated_account_id: row.try_get("paystack_dedicated_account_id")?,
        paystack_dedicated_account_number: row.try_get("paystack_dedicated_account_number")?,
        paystack_dedicated_account_name: row.try_get("paystack_dedicated_account_name")?,
        paystack_dedicated_bank_name: row.try_get("paystack_dedicated_bank_name")?,
        paystack_dedicated_bank_slug: row.try_get("paystack_dedicated_bank_slug")?,
        status: DonationStatus::from_str(&status),
        paid_at: row.try_get("paid_at")?,
        proof_status: DonationProofStatus::from_str(&proof_status),
        sui_network: row.try_get("sui_network")?,
        sui_tx_digest: row.try_get("sui_tx_digest")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

fn medical_case_from_row(row: &sqlx::postgres::PgRow) -> Result<MedicalCase, sqlx::Error> {
    let status: String = row.try_get("status")?;

    Ok(MedicalCase {
        id: row.try_get("id")?,
        hospital_id: row.try_get("hospital_id")?,
        patient_id: row.try_get("patient_id")?,
        title: row.try_get("title")?,
        public_slug: row.try_get("public_slug")?,
        diagnosis_summary: row.try_get("diagnosis_summary")?,
        bill_amount_kobo: row.try_get("bill_amount_kobo")?,
        amount_raised_kobo: row.try_get("amount_raised_kobo")?,
        status: MedicalCaseStatus::from_str(&status),
        admitted_at: row.try_get::<Option<chrono::NaiveDate>, _>("admitted_at")?,
        blockchain_network: row.try_get("blockchain_network")?,
        blockchain_tx_digest: row.try_get("blockchain_tx_digest")?,
        blockchain_record_id: row.try_get("blockchain_record_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
