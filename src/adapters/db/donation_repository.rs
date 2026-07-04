use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::{
        donation::{CaseDva, Donation, DonationMethod, DonationProofStatus, DonationStatus},
        medical_case::{MedicalCase, MedicalCaseStatus},
        public_case::PublicCaseDetails,
    },
    port::donation::{
        DonationCaseLock, DonationFailureUpdate, DonationPaymentUpdate, DonationProofAttemptUpdate,
        DonationProofJob, DonationRepository, DonationRepositoryError, NewDonation, UpsertCaseDva,
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
        insert_donation(&self.pool, donation, DonationStatus::Pending, None).await
    }

    async fn create_paid_donation(
        &self,
        donation: NewDonation,
        paid_at: DateTime<Utc>,
    ) -> Result<Donation, DonationRepositoryError> {
        let mut transaction = self.pool.begin().await?;

        let (bill_amount_kobo, amount_raised_kobo) = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT bill_amount_kobo, amount_raised_kobo
            FROM medical_cases
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(donation.medical_case_id)
        .fetch_one(&mut *transaction)
        .await?;

        let remaining_amount_kobo = (bill_amount_kobo - amount_raised_kobo).max(0);
        if donation.amount_kobo > remaining_amount_kobo {
            let _ = insert_donation_with_executor(
                &mut *transaction,
                donation,
                DonationStatus::RejectedOverflow,
                Some(paid_at),
            )
            .await?;
            transaction.commit().await?;
            return Err(DonationRepositoryError::AmountExceedsRemaining);
        }

        let created = insert_donation_with_executor(
            &mut *transaction,
            donation,
            DonationStatus::Paid,
            Some(paid_at),
        )
        .await?;

        sqlx::query(
            r#"
            UPDATE medical_cases
            SET amount_raised_kobo = amount_raised_kobo + $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(created.medical_case_id)
        .bind(created.amount_kobo)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(created)
    }

    async fn find_donation_by_reference(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<Donation>, DonationRepositoryError> {
        let row = sqlx::query(&donation_select_query("WHERE paystack_reference = $1"))
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
        load_public_case_details(&self.pool, "WHERE public_slug = $1 LIMIT 1", public_slug).await
    }

    async fn get_public_case_details_for_case_id(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<PublicCaseDetails>, DonationRepositoryError> {
        let case_row = sqlx::query(&medical_case_select_query("WHERE id = $1 LIMIT 1"))
            .bind(medical_case_id)
            .fetch_optional(&self.pool)
            .await?;

        let Some(case_row) = case_row else {
            return Ok(None);
        };

        let medical_case =
            medical_case_from_row(&case_row).map_err(DonationRepositoryError::Database)?;
        let donation_rows = sqlx::query(&donation_select_query(
            "WHERE medical_case_id = $1 AND status = 'paid' ORDER BY paid_at DESC, created_at DESC",
        ))
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

    async fn find_case_dva(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<CaseDva>, DonationRepositoryError> {
        let row = sqlx::query(&case_dva_select_query("WHERE medical_case_id = $1"))
            .bind(medical_case_id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| case_dva_from_row(&row))
            .transpose()
            .map_err(DonationRepositoryError::Database)
    }

    async fn find_case_dva_by_account_number(
        &self,
        account_number: &str,
    ) -> Result<Option<CaseDva>, DonationRepositoryError> {
        let row = sqlx::query(&case_dva_select_query("WHERE account_number = $1"))
            .bind(account_number)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| case_dva_from_row(&row))
            .transpose()
            .map_err(DonationRepositoryError::Database)
    }

    async fn upsert_case_dva(
        &self,
        dva: UpsertCaseDva,
    ) -> Result<CaseDva, DonationRepositoryError> {
        let row = sqlx::query(
            r#"
            INSERT INTO case_payment_dvas (
                medical_case_id,
                paystack_reference,
                paystack_customer_code,
                paystack_dedicated_account_id,
                account_number,
                account_name,
                bank_name,
                bank_slug,
                is_active,
                deactivated_at,
                deactivation_error,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE, NULL, NULL, NOW(), NOW())
            ON CONFLICT (medical_case_id)
            DO UPDATE SET
                paystack_reference = EXCLUDED.paystack_reference,
                paystack_customer_code = EXCLUDED.paystack_customer_code,
                paystack_dedicated_account_id = EXCLUDED.paystack_dedicated_account_id,
                account_number = EXCLUDED.account_number,
                account_name = EXCLUDED.account_name,
                bank_name = EXCLUDED.bank_name,
                bank_slug = EXCLUDED.bank_slug,
                is_active = TRUE,
                deactivated_at = NULL,
                deactivation_error = NULL,
                updated_at = NOW()
            RETURNING
                medical_case_id,
                paystack_reference,
                paystack_customer_code,
                paystack_dedicated_account_id,
                account_number,
                account_name,
                bank_name,
                bank_slug,
                is_active,
                deactivated_at,
                deactivation_error,
                created_at,
                updated_at
            "#,
        )
        .bind(dva.medical_case_id)
        .bind(dva.paystack_reference)
        .bind(dva.paystack_customer_code)
        .bind(dva.paystack_dedicated_account_id)
        .bind(dva.account_number)
        .bind(dva.account_name)
        .bind(dva.bank_name)
        .bind(dva.bank_slug)
        .fetch_one(&self.pool)
        .await?;

        case_dva_from_row(&row).map_err(DonationRepositoryError::Database)
    }

    async fn deactivate_case_dva(
        &self,
        medical_case_id: Uuid,
        deactivation_error: Option<String>,
    ) -> Result<Option<CaseDva>, DonationRepositoryError> {
        let row = sqlx::query(
            r#"
            UPDATE case_payment_dvas
            SET is_active = FALSE,
                deactivated_at = NOW(),
                deactivation_error = $2,
                updated_at = NOW()
            WHERE medical_case_id = $1
            RETURNING
                medical_case_id,
                paystack_reference,
                paystack_customer_code,
                paystack_dedicated_account_id,
                account_number,
                account_name,
                bank_name,
                bank_slug,
                is_active,
                deactivated_at,
                deactivation_error,
                created_at,
                updated_at
            "#,
        )
        .bind(medical_case_id)
        .bind(deactivation_error)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| case_dva_from_row(&row))
            .transpose()
            .map_err(DonationRepositoryError::Database)
    }

    async fn lock_pending_donation_for_confirmation(
        &self,
        paystack_reference: &str,
    ) -> Result<Option<DonationCaseLock>, DonationRepositoryError> {
        lock_donation_case(&self.pool, "paystack_reference = $1", paystack_reference).await
    }

    async fn lock_pending_donation_by_account_number(
        &self,
        account_number: &str,
    ) -> Result<Option<DonationCaseLock>, DonationRepositoryError> {
        lock_donation_case(
            &self.pool,
            "paystack_dedicated_account_number = $1",
            account_number,
        )
        .await
    }

    async fn mark_donation_paid(
        &self,
        update: DonationPaymentUpdate,
    ) -> Result<Donation, DonationRepositoryError> {
        let mut transaction = self.pool.begin().await?;

        let donation_row = sqlx::query(&donation_select_query("WHERE id = $1 FOR UPDATE"))
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

        let row = sqlx::query(&donation_returning_update_query(
            r#"
            UPDATE case_donations
            SET status = 'paid',
                paystack_transaction_reference = $2,
                paid_at = $3,
                proof_status = $4,
                sui_network = $5,
                sui_tx_digest = $6,
                proof_attempt_count = $7,
                proof_last_attempt_at = $8,
                proof_next_retry_at = $9,
                proof_last_error = $10,
                proof_published_at = $11,
                updated_at = NOW()
            WHERE id = $1
            "#,
        ))
        .bind(update.donation_id)
        .bind(update.paystack_transaction_reference)
        .bind(update.paid_at)
        .bind(update.proof_status.as_str())
        .bind(update.sui_network)
        .bind(update.sui_tx_digest)
        .bind(update.proof_attempt_count)
        .bind(update.proof_last_attempt_at)
        .bind(update.proof_next_retry_at)
        .bind(update.proof_last_error)
        .bind(update.proof_published_at)
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;

        donation_from_row(&row).map_err(DonationRepositoryError::Database)
    }

    async fn update_donation_proof(
        &self,
        update: DonationProofAttemptUpdate,
    ) -> Result<Donation, DonationRepositoryError> {
        let row = sqlx::query(&donation_returning_update_query(
            r#"
            UPDATE case_donations
            SET proof_status = $2,
                sui_network = $3,
                sui_tx_digest = $4,
                proof_attempt_count = $5,
                proof_last_attempt_at = $6,
                proof_next_retry_at = $7,
                proof_last_error = $8,
                proof_published_at = $9,
                updated_at = NOW()
            WHERE id = $1
            "#,
        ))
        .bind(update.donation_id)
        .bind(update.proof_status.as_str())
        .bind(update.sui_network)
        .bind(update.sui_tx_digest)
        .bind(update.proof_attempt_count)
        .bind(update.proof_last_attempt_at)
        .bind(update.proof_next_retry_at)
        .bind(update.proof_last_error)
        .bind(update.proof_published_at)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Err(DonationRepositoryError::NotFound);
        };

        donation_from_row(&row).map_err(DonationRepositoryError::Database)
    }

    async fn acquire_retryable_proof_jobs(
        &self,
        batch_size: i64,
        now: DateTime<Utc>,
    ) -> Result<Vec<DonationProofJob>, DonationRepositoryError> {
        let mut transaction = self.pool.begin().await?;

        let rows = sqlx::query(&donation_select_query(
            "WHERE status = 'paid' AND proof_status IN ('pending', 'pending_retry') AND sui_tx_digest IS NULL AND (proof_next_retry_at IS NULL OR proof_next_retry_at <= $1) ORDER BY COALESCE(proof_next_retry_at, paid_at, created_at), created_at LIMIT $2 FOR UPDATE SKIP LOCKED",
        ))
        .bind(now)
        .bind(batch_size)
        .fetch_all(&mut *transaction)
        .await?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let donation = donation_from_row(&row).map_err(DonationRepositoryError::Database)?;
            let case_row = sqlx::query(&medical_case_select_query("WHERE id = $1"))
                .bind(donation.medical_case_id)
                .fetch_one(&mut *transaction)
                .await?;
            let medical_case =
                medical_case_from_row(&case_row).map_err(DonationRepositoryError::Database)?;
            jobs.push(DonationProofJob {
                donation,
                medical_case,
            });
        }

        transaction.commit().await?;
        Ok(jobs)
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

async fn insert_donation(
    pool: &PgPool,
    donation: NewDonation,
    status: DonationStatus,
    paid_at: Option<DateTime<Utc>>,
) -> Result<Donation, DonationRepositoryError> {
    insert_donation_with_executor(pool, donation, status, paid_at).await
}

async fn insert_donation_with_executor<'e, E>(
    executor: E,
    donation: NewDonation,
    status: DonationStatus,
    paid_at: Option<DateTime<Utc>>,
) -> Result<Donation, DonationRepositoryError>
where
    E: Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();
    let proof_status = DonationProofStatus::Pending;

    let row = sqlx::query(
        r#"
        INSERT INTO case_donations (
            id,
            medical_case_id,
            donor_display_name,
            donor_email,
            amount_kobo,
            method,
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
            proof_attempt_count,
            proof_last_attempt_at,
            proof_next_retry_at,
            proof_last_error,
            proof_published_at,
            created_at,
            updated_at
        )
        VALUES (
            $1, $2, $3, LOWER($4), $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
            $17, $18, $19, NULL, NULL, 0, NULL, NULL, NULL, NULL, NOW(), NOW()
        )
        RETURNING
            id,
            medical_case_id,
            donor_display_name,
            donor_email,
            amount_kobo,
            method,
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
            proof_attempt_count,
            proof_last_attempt_at,
            proof_next_retry_at,
            proof_last_error,
            proof_published_at,
            created_at,
            updated_at
        "#,
    )
    .bind(id)
    .bind(donation.medical_case_id)
    .bind(donation.donor_display_name)
    .bind(donation.donor_email)
    .bind(donation.amount_kobo)
    .bind(donation.method.as_str())
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
    .bind(status.as_str())
    .bind(paid_at)
    .bind(proof_status.as_str())
    .fetch_one(executor)
    .await
    .map_err(map_donation_error)?;

    donation_from_row(&row).map_err(DonationRepositoryError::Database)
}

async fn load_public_case_details(
    pool: &PgPool,
    where_clause: &str,
    public_slug: &str,
) -> Result<Option<PublicCaseDetails>, DonationRepositoryError> {
    let case_row = sqlx::query(&medical_case_select_query(where_clause))
        .bind(public_slug)
        .fetch_optional(pool)
        .await?;

    let Some(case_row) = case_row else {
        return Ok(None);
    };

    let medical_case =
        medical_case_from_row(&case_row).map_err(DonationRepositoryError::Database)?;
    let donation_rows = sqlx::query(&donation_select_query(
        "WHERE medical_case_id = $1 AND status = 'paid' ORDER BY paid_at DESC, created_at DESC",
    ))
    .bind(medical_case.id)
    .fetch_all(pool)
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

async fn lock_donation_case(
    pool: &PgPool,
    where_clause: &str,
    lookup_value: &str,
) -> Result<Option<DonationCaseLock>, DonationRepositoryError> {
    let mut transaction = pool.begin().await?;
    let donation_row = sqlx::query(&donation_select_query(&format!(
        "WHERE {where_clause} FOR UPDATE"
    )))
    .bind(lookup_value)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(donation_row) = donation_row else {
        transaction.commit().await?;
        return Ok(None);
    };

    let donation = donation_from_row(&donation_row).map_err(DonationRepositoryError::Database)?;

    let case_row = sqlx::query(&medical_case_select_query("WHERE id = $1 FOR UPDATE"))
        .bind(donation.medical_case_id)
        .fetch_one(&mut *transaction)
        .await?;

    let medical_case =
        medical_case_from_row(&case_row).map_err(DonationRepositoryError::Database)?;

    transaction.commit().await?;

    Ok(Some(DonationCaseLock {
        donation,
        remaining_amount_kobo: (medical_case.bill_amount_kobo - medical_case.amount_raised_kobo)
            .max(0),
        medical_case,
    }))
}

fn donation_select_query(suffix: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            medical_case_id,
            donor_display_name,
            donor_email,
            amount_kobo,
            method,
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
            proof_attempt_count,
            proof_last_attempt_at,
            proof_next_retry_at,
            proof_last_error,
            proof_published_at,
            created_at,
            updated_at
        FROM case_donations
        {suffix}
        "#
    )
}

fn case_dva_select_query(suffix: &str) -> String {
    format!(
        r#"
        SELECT
            medical_case_id,
            paystack_reference,
            paystack_customer_code,
            paystack_dedicated_account_id,
            account_number,
            account_name,
            bank_name,
            bank_slug,
            is_active,
            deactivated_at,
            deactivation_error,
            created_at,
            updated_at
        FROM case_payment_dvas
        {suffix}
        "#
    )
}

fn medical_case_select_query(suffix: &str) -> String {
    format!(
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
        {suffix}
        "#
    )
}

fn donation_returning_update_query(update_sql: &str) -> String {
    format!(
        r#"
        {update_sql}
        RETURNING
            id,
            medical_case_id,
            donor_display_name,
            donor_email,
            amount_kobo,
            method,
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
            proof_attempt_count,
            proof_last_attempt_at,
            proof_next_retry_at,
            proof_last_error,
            proof_published_at,
            created_at,
            updated_at
        "#
    )
}

fn map_donation_error(error: sqlx::Error) -> DonationRepositoryError {
    if let Some(constraint) = error
        .as_database_error()
        .and_then(|database_error| database_error.constraint())
    {
        if constraint == "case_donations_paystack_reference_key"
            || constraint == "idx_case_donations_dedicated_account_number"
            || constraint == "case_payment_dvas_account_number_key"
            || constraint == "case_payment_dvas_paystack_reference_key"
        {
            return DonationRepositoryError::DuplicateReference;
        }
    }

    DonationRepositoryError::Database(error)
}

fn donation_from_row(row: &sqlx::postgres::PgRow) -> Result<Donation, sqlx::Error> {
    let status: String = row.try_get("status")?;
    let proof_status: String = row.try_get("proof_status")?;
    let method: String = row.try_get("method")?;

    Ok(Donation {
        id: row.try_get("id")?,
        medical_case_id: row.try_get("medical_case_id")?,
        donor_display_name: row.try_get("donor_display_name")?,
        donor_email: row.try_get("donor_email")?,
        amount_kobo: row.try_get("amount_kobo")?,
        method: DonationMethod::from_str(&method),
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
        proof_attempt_count: row.try_get("proof_attempt_count")?,
        proof_last_attempt_at: row.try_get("proof_last_attempt_at")?,
        proof_next_retry_at: row.try_get("proof_next_retry_at")?,
        proof_last_error: row.try_get("proof_last_error")?,
        proof_published_at: row.try_get("proof_published_at")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

fn case_dva_from_row(row: &sqlx::postgres::PgRow) -> Result<CaseDva, sqlx::Error> {
    Ok(CaseDva {
        medical_case_id: row.try_get("medical_case_id")?,
        paystack_reference: row.try_get("paystack_reference")?,
        paystack_customer_code: row.try_get("paystack_customer_code")?,
        paystack_dedicated_account_id: row.try_get("paystack_dedicated_account_id")?,
        account_number: row.try_get("account_number")?,
        account_name: row.try_get("account_name")?,
        bank_name: row.try_get("bank_name")?,
        bank_slug: row.try_get("bank_slug")?,
        is_active: row.try_get("is_active")?,
        deactivated_at: row.try_get("deactivated_at")?,
        deactivation_error: row.try_get("deactivation_error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
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
