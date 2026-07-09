// The adapters layer contains infrastructure implementations.
//
// In hexagonal architecture, adapters are things like:
// - databases
// - payment gateways
// - blockchain clients
// - email providers
//
// `pub mod db;` makes the db module available to the rest of the app.
pub mod auth;
pub mod checkout_reservation;
pub mod db;
pub mod donation_proof;
pub mod donation_proof_retry;
pub mod email;
pub mod payment;
pub mod storage;
