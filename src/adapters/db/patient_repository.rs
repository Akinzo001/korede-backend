use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::{
        patient::Patient, patient_email_otp::PatientEmailOtp,
        patient_password_reset_otp::PatientPasswordResetOtp,
    },
    port::patient::{
        NewPatient, NewPatientEmailOtp, NewPatientPasswordResetOtp, PatientRepository,
        PatientRepositoryError,
    },
};

#[derive(Debug, Clone)]
pub struct PostgresPatientRepository {
    pool: PgPool,
}

impl PostgresPatientRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PatientRepository for PostgresPatientRepository {
    async fn create_patient(&self, patient: NewPatient) -> Result<Patient, PatientRepositoryError> {
        let id = Uuid::new_v4();
        let full_name = format!("{} {}", patient.first_name, patient.last_name);

        let result = sqlx::query(
            r#"
            INSERT INTO patients (
                id,
                username,
                first_name,
                last_name,
                full_name,
                email,
                email_verified,
                email_verified_at,
                password_hash,
                date_of_birth,
                age,
                gender,
                phone_number,
                consent_given,
                created_at,
                updated_at
            )
            VALUES (
                $1,
                LOWER($2),
                $3,
                $4,
                $5,
                LOWER($6),
                FALSE,
                NULL,
                $7,
                $8,
                NULL,
                $9,
                $10,
                TRUE,
                NOW(),
                NOW()
            )
            RETURNING
                id,
                username,
                first_name,
                last_name,
                full_name,
                email,
                email_verified,
                email_verified_at,
                password_hash,
                date_of_birth,
                age,
                gender,
                phone_number,
                consent_given,
                created_at,
                updated_at
            "#,
        )
        .bind(id)
        .bind(patient.username)
        .bind(patient.first_name)
        .bind(patient.last_name)
        .bind(full_name)
        .bind(patient.email)
        .bind(patient.password_hash)
        .bind(patient.date_of_birth)
        .bind(patient.gender)
        .bind(patient.phone_number)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(row) => patient_from_row(&row).map_err(PatientRepositoryError::Database),
            Err(error) if is_duplicate_email(&error) => Err(PatientRepositoryError::DuplicateEmail),
            Err(error) if is_unique_violation(&error) => {
                Err(PatientRepositoryError::DuplicateUsername)
            }
            Err(error) => Err(PatientRepositoryError::Database(error)),
        }
    }

    async fn find_patient_by_username(
        &self,
        username: &str,
    ) -> Result<Option<Patient>, PatientRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                username,
                first_name,
                last_name,
                full_name,
                email,
                email_verified,
                email_verified_at,
                password_hash,
                date_of_birth,
                age,
                gender,
                phone_number,
                consent_given,
                created_at,
                updated_at
            FROM patients
            WHERE LOWER(username) = LOWER($1)
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| patient_from_row(&row))
            .transpose()
            .map_err(PatientRepositoryError::Database)
    }

    async fn find_patient_by_id(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<Patient>, PatientRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                username,
                first_name,
                last_name,
                full_name,
                email,
                email_verified,
                email_verified_at,
                password_hash,
                date_of_birth,
                age,
                gender,
                phone_number,
                consent_given,
                created_at,
                updated_at
            FROM patients
            WHERE id = $1
            "#,
        )
        .bind(patient_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| patient_from_row(&row))
            .transpose()
            .map_err(PatientRepositoryError::Database)
    }

    async fn find_patient_by_email(
        &self,
        email: &str,
    ) -> Result<Option<Patient>, PatientRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                username,
                first_name,
                last_name,
                full_name,
                email,
                email_verified,
                email_verified_at,
                password_hash,
                date_of_birth,
                age,
                gender,
                phone_number,
                consent_given,
                created_at,
                updated_at
            FROM patients
            WHERE LOWER(email) = LOWER($1)
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| patient_from_row(&row))
            .transpose()
            .map_err(PatientRepositoryError::Database)
    }

    async fn create_email_otp(
        &self,
        otp: NewPatientEmailOtp,
    ) -> Result<PatientEmailOtp, PatientRepositoryError> {
        let id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO patient_email_otps (
                id,
                patient_id,
                email,
                otp_hash,
                expires_at,
                used_at,
                attempt_count,
                created_at
            )
            VALUES ($1, $2, LOWER($3), $4, $5, NULL, 0, NOW())
            RETURNING
                id,
                patient_id,
                email,
                otp_hash,
                expires_at,
                used_at,
                attempt_count,
                created_at
            "#,
        )
        .bind(id)
        .bind(otp.patient_id)
        .bind(otp.email)
        .bind(otp.otp_hash)
        .bind(otp.expires_at)
        .fetch_one(&self.pool)
        .await?;

        email_otp_from_row(&row).map_err(PatientRepositoryError::Database)
    }

    async fn find_latest_email_otp(
        &self,
        email: &str,
    ) -> Result<Option<PatientEmailOtp>, PatientRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                patient_id,
                email,
                otp_hash,
                expires_at,
                used_at,
                attempt_count,
                created_at
            FROM patient_email_otps
            WHERE LOWER(email) = LOWER($1)
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| email_otp_from_row(&row))
            .transpose()
            .map_err(PatientRepositoryError::Database)
    }

    async fn increment_email_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patient_email_otps
            SET attempt_count = attempt_count + 1
            WHERE id = $1
            "#,
        )
        .bind(otp_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_email_otp_used(&self, otp_id: Uuid) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patient_email_otps
            SET used_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(otp_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_patient_email_verified(
        &self,
        patient_id: Uuid,
    ) -> Result<Patient, PatientRepositoryError> {
        let row = sqlx::query(
            r#"
            UPDATE patients
            SET email_verified = TRUE,
                email_verified_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING
                id,
                username,
                first_name,
                last_name,
                full_name,
                email,
                email_verified,
                email_verified_at,
                password_hash,
                date_of_birth,
                age,
                gender,
                phone_number,
                consent_given,
                created_at,
                updated_at
            "#,
        )
        .bind(patient_id)
        .fetch_one(&self.pool)
        .await?;

        patient_from_row(&row).map_err(PatientRepositoryError::Database)
    }

    async fn invalidate_active_email_otps(
        &self,
        patient_id: Uuid,
    ) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patient_email_otps
            SET used_at = NOW()
            WHERE patient_id = $1
              AND used_at IS NULL
            "#,
        )
        .bind(patient_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn latest_email_otp_created_at(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, PatientRepositoryError> {
        let created_at = sqlx::query_scalar(
            r#"
            SELECT created_at
            FROM patient_email_otps
            WHERE patient_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(patient_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(created_at)
    }

    async fn create_password_reset_otp(
        &self,
        otp: NewPatientPasswordResetOtp,
    ) -> Result<PatientPasswordResetOtp, PatientRepositoryError> {
        let id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO patient_password_reset_otps (
                id,
                patient_id,
                email,
                otp_hash,
                expires_at,
                used_at,
                attempt_count,
                created_at
            )
            VALUES ($1, $2, LOWER($3), $4, $5, NULL, 0, NOW())
            RETURNING
                id,
                patient_id,
                email,
                otp_hash,
                expires_at,
                used_at,
                attempt_count,
                created_at
            "#,
        )
        .bind(id)
        .bind(otp.patient_id)
        .bind(otp.email)
        .bind(otp.otp_hash)
        .bind(otp.expires_at)
        .fetch_one(&self.pool)
        .await?;

        password_reset_otp_from_row(&row).map_err(PatientRepositoryError::Database)
    }

    async fn find_latest_password_reset_otp(
        &self,
        email: &str,
    ) -> Result<Option<PatientPasswordResetOtp>, PatientRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                patient_id,
                email,
                otp_hash,
                expires_at,
                used_at,
                attempt_count,
                created_at
            FROM patient_password_reset_otps
            WHERE LOWER(email) = LOWER($1)
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| password_reset_otp_from_row(&row))
            .transpose()
            .map_err(PatientRepositoryError::Database)
    }

    async fn increment_password_reset_otp_attempts(
        &self,
        otp_id: Uuid,
    ) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patient_password_reset_otps
            SET attempt_count = attempt_count + 1
            WHERE id = $1
            "#,
        )
        .bind(otp_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_password_reset_otp_used(&self, otp_id: Uuid) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patient_password_reset_otps
            SET used_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(otp_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn invalidate_active_password_reset_otps(
        &self,
        patient_id: Uuid,
    ) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patient_password_reset_otps
            SET used_at = NOW()
            WHERE patient_id = $1
              AND used_at IS NULL
            "#,
        )
        .bind(patient_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn latest_password_reset_otp_created_at(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, PatientRepositoryError> {
        let created_at = sqlx::query_scalar(
            r#"
            SELECT created_at
            FROM patient_password_reset_otps
            WHERE patient_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(patient_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(created_at)
    }

    async fn update_patient_password(
        &self,
        patient_id: Uuid,
        password_hash: String,
    ) -> Result<(), PatientRepositoryError> {
        sqlx::query(
            r#"
            UPDATE patients
            SET password_hash = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(patient_id)
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

fn patient_from_row(row: &sqlx::postgres::PgRow) -> Result<Patient, sqlx::Error> {
    Ok(Patient {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        first_name: row.try_get("first_name")?,
        last_name: row.try_get("last_name")?,
        full_name: row.try_get("full_name")?,
        email: row.try_get("email")?,
        email_verified: row.try_get("email_verified")?,
        email_verified_at: row.try_get("email_verified_at")?,
        password_hash: row.try_get("password_hash")?,
        date_of_birth: row.try_get::<Option<NaiveDate>, _>("date_of_birth")?,
        age: row.try_get("age")?,
        gender: row.try_get("gender")?,
        phone_number: row.try_get("phone_number")?,
        consent_given: row.try_get("consent_given")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}

fn email_otp_from_row(row: &sqlx::postgres::PgRow) -> Result<PatientEmailOtp, sqlx::Error> {
    Ok(PatientEmailOtp {
        id: row.try_get("id")?,
        patient_id: row.try_get("patient_id")?,
        email: row.try_get("email")?,
        otp_hash: row.try_get("otp_hash")?,
        expires_at: row.try_get("expires_at")?,
        used_at: row.try_get("used_at")?,
        attempt_count: row.try_get("attempt_count")?,
        created_at: row.try_get("created_at")?,
    })
}

fn password_reset_otp_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<PatientPasswordResetOtp, sqlx::Error> {
    Ok(PatientPasswordResetOtp {
        id: row.try_get("id")?,
        patient_id: row.try_get("patient_id")?,
        email: row.try_get("email")?,
        otp_hash: row.try_get("otp_hash")?,
        expires_at: row.try_get("expires_at")?,
        used_at: row.try_get("used_at")?,
        attempt_count: row.try_get("attempt_count")?,
        created_at: row.try_get("created_at")?,
    })
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| code == "23505")
}

fn is_duplicate_email(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.constraint())
        .is_some_and(|constraint| constraint == "idx_patients_email_lower")
}
