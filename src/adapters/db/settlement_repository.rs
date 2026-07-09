use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::settlement::{HospitalSettlement, HospitalSettlementStatus},
    port::settlement::{
        AdminSettlementListQuery, AdminSettlementOperation, HospitalSettlementRepository,
        NewHospitalSettlement, SettlementRecipientUpdate, SettlementRepositoryError,
        SettlementStatusUpdate, SettlementTransferUpdate,
    },
};

#[derive(Debug, Clone)]
pub struct PostgresHospitalSettlementRepository {
    pool: PgPool,
}

impl PostgresHospitalSettlementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HospitalSettlementRepository for PostgresHospitalSettlementRepository {
    async fn create_or_get_settlement(
        &self,
        settlement: NewHospitalSettlement,
    ) -> Result<HospitalSettlement, SettlementRepositoryError> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO hospital_settlements (
                id,
                hospital_id,
                medical_case_id,
                amount_kobo,
                status,
                settlement_reference,
                bank_name,
                bank_code,
                account_name,
                account_number,
                failure_reason,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW(), NOW())
            ON CONFLICT (medical_case_id) DO NOTHING
            RETURNING
                id,
                hospital_id,
                medical_case_id,
                amount_kobo,
                status,
                settlement_reference,
                bank_name,
                bank_code,
                account_name,
                account_number,
                paystack_recipient_code,
                paystack_transfer_code,
                paystack_transfer_id,
                paystack_status,
                failure_reason,
                initiated_at,
                paid_at,
                failed_at,
                created_at,
                updated_at
            "#,
        )
        .bind(id)
        .bind(settlement.hospital_id)
        .bind(settlement.medical_case_id)
        .bind(settlement.amount_kobo)
        .bind(settlement.status.as_str())
        .bind(settlement.settlement_reference)
        .bind(settlement.bank_name)
        .bind(settlement.bank_code)
        .bind(settlement.account_name)
        .bind(settlement.account_number)
        .bind(settlement.failure_reason)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return settlement_from_row(&row).map_err(SettlementRepositoryError::Database);
        }

        self.find_by_medical_case_id(settlement.medical_case_id)
            .await?
            .ok_or(SettlementRepositoryError::NotFound)
    }

    async fn get_settlement(
        &self,
        settlement_id: Uuid,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError> {
        let sql = settlement_select_sql("WHERE s.id = $1");
        let row = sqlx::query(&sql)
            .bind(settlement_id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)
    }

    async fn find_by_medical_case_id(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError> {
        let sql = settlement_select_sql("WHERE s.medical_case_id = $1");
        let row = sqlx::query(&sql)
            .bind(medical_case_id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)
    }

    async fn find_by_reference(
        &self,
        reference: &str,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError> {
        let sql = settlement_select_sql("WHERE s.settlement_reference = $1");
        let row = sqlx::query(&sql)
            .bind(reference)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)
    }

    async fn find_by_transfer_code(
        &self,
        transfer_code: &str,
    ) -> Result<Option<HospitalSettlement>, SettlementRepositoryError> {
        let sql = settlement_select_sql("WHERE s.paystack_transfer_code = $1");
        let row = sqlx::query(&sql)
            .bind(transfer_code)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)
    }

    async fn update_recipient(
        &self,
        update: SettlementRecipientUpdate,
    ) -> Result<HospitalSettlement, SettlementRepositoryError> {
        let row = sqlx::query(
            r#"
            UPDATE hospital_settlements
            SET status = $2,
                paystack_recipient_code = $3,
                paystack_status = $4,
                failure_reason = NULL,
                failed_at = NULL,
                updated_at = NOW()
            WHERE id = $1
            RETURNING
                id,
                hospital_id,
                medical_case_id,
                amount_kobo,
                status,
                settlement_reference,
                bank_name,
                bank_code,
                account_name,
                account_number,
                paystack_recipient_code,
                paystack_transfer_code,
                paystack_transfer_id,
                paystack_status,
                failure_reason,
                initiated_at,
                paid_at,
                failed_at,
                created_at,
                updated_at
            "#,
        )
        .bind(update.settlement_id)
        .bind(update.status.as_str())
        .bind(update.paystack_recipient_code)
        .bind(update.paystack_status)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)?
            .ok_or(SettlementRepositoryError::NotFound)
    }

    async fn update_transfer(
        &self,
        update: SettlementTransferUpdate,
    ) -> Result<HospitalSettlement, SettlementRepositoryError> {
        let (paid_at, failed_at) = timestamp_columns(&update.status);
        let row = sqlx::query(
            r#"
            UPDATE hospital_settlements
            SET status = $2,
                paystack_transfer_code = COALESCE($3, paystack_transfer_code),
                paystack_transfer_id = COALESCE($4, paystack_transfer_id),
                paystack_status = $5,
                failure_reason = $6,
                initiated_at = COALESCE(initiated_at, NOW()),
                paid_at = COALESCE($7, paid_at),
                failed_at = COALESCE($8, failed_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING
                id,
                hospital_id,
                medical_case_id,
                amount_kobo,
                status,
                settlement_reference,
                bank_name,
                bank_code,
                account_name,
                account_number,
                paystack_recipient_code,
                paystack_transfer_code,
                paystack_transfer_id,
                paystack_status,
                failure_reason,
                initiated_at,
                paid_at,
                failed_at,
                created_at,
                updated_at
            "#,
        )
        .bind(update.settlement_id)
        .bind(update.status.as_str())
        .bind(update.paystack_transfer_code)
        .bind(update.paystack_transfer_id)
        .bind(update.paystack_status)
        .bind(update.failure_reason)
        .bind(paid_at)
        .bind(failed_at)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)?
            .ok_or(SettlementRepositoryError::NotFound)
    }

    async fn update_status(
        &self,
        update: SettlementStatusUpdate,
    ) -> Result<HospitalSettlement, SettlementRepositoryError> {
        let (paid_at, failed_at) = timestamp_columns(&update.status);
        let row = sqlx::query(
            r#"
            UPDATE hospital_settlements
            SET status = $2,
                paystack_status = $3,
                failure_reason = $4,
                paid_at = COALESCE($5, paid_at),
                failed_at = COALESCE($6, failed_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING
                id,
                hospital_id,
                medical_case_id,
                amount_kobo,
                status,
                settlement_reference,
                bank_name,
                bank_code,
                account_name,
                account_number,
                paystack_recipient_code,
                paystack_transfer_code,
                paystack_transfer_id,
                paystack_status,
                failure_reason,
                initiated_at,
                paid_at,
                failed_at,
                created_at,
                updated_at
            "#,
        )
        .bind(update.settlement_id)
        .bind(update.status.as_str())
        .bind(update.paystack_status)
        .bind(update.failure_reason)
        .bind(paid_at)
        .bind(failed_at)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| settlement_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)?
            .ok_or(SettlementRepositoryError::NotFound)
    }

    async fn count_admin_settlements(
        &self,
        query: AdminSettlementListQuery,
    ) -> Result<i64, SettlementRepositoryError> {
        let mut sql = "SELECT COUNT(*) FROM hospital_settlements s WHERE TRUE".to_owned();
        append_admin_filters(&mut sql, &query);
        sqlx::query_scalar::<_, i64>(&sql)
            .bind(query.status.as_ref().map(|status| status.as_str()))
            .bind(query.hospital_id)
            .bind(query.medical_case_id)
            .bind(query.from)
            .bind(query.to)
            .fetch_one(&self.pool)
            .await
            .map_err(SettlementRepositoryError::Database)
    }

    async fn list_admin_settlements(
        &self,
        query: AdminSettlementListQuery,
    ) -> Result<Vec<AdminSettlementOperation>, SettlementRepositoryError> {
        let mut sql = admin_settlement_operation_sql("WHERE TRUE");
        append_admin_filters(&mut sql, &query);
        sql.push_str(" ORDER BY s.created_at DESC, s.id DESC LIMIT $6 OFFSET $7");

        let mut query_builder = sqlx::query(&sql);
        query_builder = bind_admin_filters(query_builder, &query);
        query_builder = query_builder.bind(query.limit).bind(query.offset);

        let rows = query_builder.fetch_all(&self.pool).await?;

        rows.iter()
            .map(admin_settlement_operation_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(SettlementRepositoryError::Database)
    }

    async fn get_admin_settlement(
        &self,
        settlement_id: Uuid,
    ) -> Result<Option<AdminSettlementOperation>, SettlementRepositoryError> {
        let sql = admin_settlement_operation_sql("WHERE s.id = $1");
        let row = sqlx::query(&sql)
            .bind(settlement_id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| admin_settlement_operation_from_row(&row))
            .transpose()
            .map_err(SettlementRepositoryError::Database)
    }
}

fn settlement_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            s.id,
            s.hospital_id,
            s.medical_case_id,
            s.amount_kobo,
            s.status,
            s.settlement_reference,
            s.bank_name,
            s.bank_code,
            s.account_name,
            s.account_number,
            s.paystack_recipient_code,
            s.paystack_transfer_code,
            s.paystack_transfer_id,
            s.paystack_status,
            s.failure_reason,
            s.initiated_at,
            s.paid_at,
            s.failed_at,
            s.created_at,
            s.updated_at
        FROM hospital_settlements s
        {where_clause}
        "#
    )
}

fn admin_settlement_operation_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            s.id,
            s.hospital_id,
            s.medical_case_id,
            s.amount_kobo,
            s.status,
            s.settlement_reference,
            s.bank_name,
            s.bank_code,
            s.account_name,
            s.account_number,
            s.paystack_recipient_code,
            s.paystack_transfer_code,
            s.paystack_transfer_id,
            s.paystack_status,
            s.failure_reason,
            s.initiated_at,
            s.paid_at,
            s.failed_at,
            s.created_at,
            s.updated_at,
            h.name AS hospital_name,
            m.title AS case_title,
            m.public_slug AS public_slug,
            p.id AS patient_id,
            p.full_name AS patient_name
        FROM hospital_settlements s
        JOIN hospitals h ON h.id = s.hospital_id
        JOIN medical_cases m ON m.id = s.medical_case_id
        JOIN patients p ON p.id = m.patient_id
        {where_clause}
        "#
    )
}

fn append_admin_filters(sql: &mut String, query: &AdminSettlementListQuery) {
    sql.push_str(" AND ($1::text IS NULL OR s.status = $1)");
    sql.push_str(" AND ($2::uuid IS NULL OR s.hospital_id = $2)");
    sql.push_str(" AND ($3::uuid IS NULL OR s.medical_case_id = $3)");
    sql.push_str(" AND ($4::timestamptz IS NULL OR s.created_at >= $4)");
    sql.push_str(" AND ($5::timestamptz IS NULL OR s.created_at <= $5)");
    if query.admin_action_required_only {
        sql.push_str(
            " AND s.status IN ('failed', 'failed_config', 'bank_details_required', 'reversed', 'otp_required')",
        );
    }
}

fn bind_admin_filters<'q>(
    mut query_builder: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    query: &'q AdminSettlementListQuery,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query_builder = query_builder.bind(query.status.as_ref().map(|status| status.as_str()));
    query_builder = query_builder.bind(query.hospital_id);
    query_builder = query_builder.bind(query.medical_case_id);
    query_builder = query_builder.bind(query.from);
    query_builder = query_builder.bind(query.to);
    query_builder
}

fn timestamp_columns(
    status: &HospitalSettlementStatus,
) -> (
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
) {
    let now = chrono::Utc::now();
    match status {
        HospitalSettlementStatus::Paid => (Some(now), None),
        HospitalSettlementStatus::Failed
        | HospitalSettlementStatus::FailedConfig
        | HospitalSettlementStatus::BankDetailsRequired
        | HospitalSettlementStatus::Reversed => (None, Some(now)),
        _ => (None, None),
    }
}

fn settlement_from_row(row: &sqlx::postgres::PgRow) -> Result<HospitalSettlement, sqlx::Error> {
    let status: String = row.try_get("status")?;
    Ok(HospitalSettlement {
        id: row.try_get("id")?,
        hospital_id: row.try_get("hospital_id")?,
        medical_case_id: row.try_get("medical_case_id")?,
        amount_kobo: row.try_get("amount_kobo")?,
        status: HospitalSettlementStatus::from_str(&status),
        settlement_reference: row.try_get("settlement_reference")?,
        bank_name: row.try_get("bank_name")?,
        bank_code: row.try_get("bank_code")?,
        account_name: row.try_get("account_name")?,
        account_number: row.try_get("account_number")?,
        paystack_recipient_code: row.try_get("paystack_recipient_code")?,
        paystack_transfer_code: row.try_get("paystack_transfer_code")?,
        paystack_transfer_id: row.try_get("paystack_transfer_id")?,
        paystack_status: row.try_get("paystack_status")?,
        failure_reason: row.try_get("failure_reason")?,
        initiated_at: row.try_get("initiated_at")?,
        paid_at: row.try_get("paid_at")?,
        failed_at: row.try_get("failed_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn admin_settlement_operation_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AdminSettlementOperation, sqlx::Error> {
    Ok(AdminSettlementOperation {
        settlement: settlement_from_row(row)?,
        hospital_name: row.try_get("hospital_name")?,
        case_title: row.try_get("case_title")?,
        public_slug: row.try_get("public_slug")?,
        patient_id: row.try_get("patient_id")?,
        patient_name: row.try_get("patient_name")?,
    })
}
