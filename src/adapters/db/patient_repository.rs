use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::patient::Patient,
    port::patient::{NewPatient, PatientRepository, PatientRepositoryError},
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
    async fn create_patient(
        &self,
        patient: NewPatient,
    ) -> Result<Patient, PatientRepositoryError> {
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
}

fn patient_from_row(row: &sqlx::postgres::PgRow) -> Result<Patient, sqlx::Error> {
    Ok(Patient {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        first_name: row.try_get("first_name")?,
        last_name: row.try_get("last_name")?,
        full_name: row.try_get("full_name")?,
        email: row.try_get("email")?,
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
