use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::patient_declaration::PatientDeclaration,
    port::patient_declaration::{
        NewPatientDeclaration, PatientDeclarationRepository, PatientDeclarationRepositoryError,
        UpdatePatientDeclaration,
    },
};

#[derive(Debug, Clone)]
pub struct PostgresPatientDeclarationRepository {
    pool: PgPool,
}

impl PostgresPatientDeclarationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PatientDeclarationRepository for PostgresPatientDeclarationRepository {
    async fn create_patient_declaration(
        &self,
        declaration: NewPatientDeclaration,
    ) -> Result<PatientDeclaration, PatientDeclarationRepositoryError> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO patient_declarations (
                id,
                patient_id,
                statement,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, NOW(), NOW())
            RETURNING
                id,
                patient_id,
                statement,
                created_at,
                updated_at
            "#,
        )
        .bind(id)
        .bind(declaration.patient_id)
        .bind(declaration.statement)
        .fetch_one(&self.pool)
        .await
        .map_err(map_declaration_write_error)?;

        declaration_from_row(&row).map_err(PatientDeclarationRepositoryError::Database)
    }

    async fn update_patient_declaration(
        &self,
        declaration: UpdatePatientDeclaration,
    ) -> Result<PatientDeclaration, PatientDeclarationRepositoryError> {
        let row = sqlx::query(
            r#"
            UPDATE patient_declarations
            SET
                statement = $2,
                updated_at = NOW()
            WHERE patient_id = $1
            RETURNING
                id,
                patient_id,
                statement,
                created_at,
                updated_at
            "#,
        )
        .bind(declaration.patient_id)
        .bind(declaration.statement)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| declaration_from_row(&row))
            .transpose()
            .map_err(PatientDeclarationRepositoryError::Database)?
            .ok_or(PatientDeclarationRepositoryError::NotFound)
    }

    async fn find_current_patient_declaration(
        &self,
        patient_id: Uuid,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                patient_id,
                statement,
                created_at,
                updated_at
            FROM patient_declarations
            WHERE patient_id = $1
            "#,
        )
        .bind(patient_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| declaration_from_row(&row))
            .transpose()
            .map_err(PatientDeclarationRepositoryError::Database)
    }

    async fn find_current_patient_declaration_by_username(
        &self,
        username: &str,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                declaration.id,
                declaration.patient_id,
                declaration.statement,
                declaration.created_at,
                declaration.updated_at
            FROM patient_declarations declaration
            INNER JOIN patients patient
                ON patient.id = declaration.patient_id
            WHERE LOWER(patient.username) = LOWER($1)
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| declaration_from_row(&row))
            .transpose()
            .map_err(PatientDeclarationRepositoryError::Database)
    }

    async fn find_case_declaration(
        &self,
        medical_case_id: Uuid,
    ) -> Result<Option<PatientDeclaration>, PatientDeclarationRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                patient_id,
                statement,
                created_at,
                updated_at
            FROM patient_case_declarations
            WHERE medical_case_id = $1
            "#,
        )
        .bind(medical_case_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| declaration_from_row(&row))
            .transpose()
            .map_err(PatientDeclarationRepositoryError::Database)
    }
}

fn map_declaration_write_error(error: sqlx::Error) -> PatientDeclarationRepositoryError {
    if is_duplicate_current_declaration(&error) {
        PatientDeclarationRepositoryError::DuplicateDeclaration
    } else {
        PatientDeclarationRepositoryError::Database(error)
    }
}

fn is_duplicate_current_declaration(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.constraint())
        .is_some_and(|constraint| constraint == "patient_declarations_patient_id_key")
}

fn declaration_from_row(row: &sqlx::postgres::PgRow) -> Result<PatientDeclaration, sqlx::Error> {
    Ok(PatientDeclaration {
        id: row.try_get("id")?,
        patient_id: row.try_get("patient_id")?,
        statement: row.try_get("statement")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
