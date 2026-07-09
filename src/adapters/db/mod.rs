// PostgreSQL-specific database adapter.
//
// This module knows how to connect to Postgres using SQLx.
pub mod donation_repository;
pub mod hospital_repository;
pub mod medical_case_repository;
pub mod patient_declaration_repository;
pub mod patient_repository;
pub mod postgres;
pub mod refresh_token_repository;
pub mod settlement_repository;
