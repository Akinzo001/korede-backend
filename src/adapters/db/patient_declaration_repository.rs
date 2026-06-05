use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    domain::patient_declaration::PatientDeclaration,
    port::patient_declaration::{
        PatientDeclarationRepository, PatientDeclarationRepositoryError, UpsertPatientDeclaration,
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
    async fn upsert_patient_declaration(
        &self,
        declaration: UpsertPatientDeclaration,
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
            ON CONFLICT (patient_id)
            DO UPDATE SET
                statement = EXCLUDED.statement,
                updated_at = NOW()
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
        .await?;

        declaration_from_row(&row).map_err(PatientDeclarationRepositoryError::Database)
    }

    async fn find_patient_declaration(
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

    async fn find_patient_declaration_by_username(
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
