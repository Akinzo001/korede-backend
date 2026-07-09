// This project has both:
// 1. a library crate: `src/lib.rs`
// 2. a binary crate: `src/main.rs`
//
// `src/lib.rs` publicly exposes modules like `adapters` and `api`.
// That is why `main.rs` can import them using the package name:
// `korede_backend`.
use korede_backend::{
    adapters::{
        auth::{Argon2PasswordHasher, JwtTokenService},
        checkout_reservation::run_checkout_reservation_expiry_worker,
        db::{
            donation_repository::PostgresDonationRepository,
            hospital_repository::PostgresHospitalRepository,
            medical_case_repository::PostgresMedicalCaseRepository,
            patient_declaration_repository::PostgresPatientDeclarationRepository,
            patient_repository::PostgresPatientRepository,
            postgres::{connect, run_migrations},
            refresh_token_repository::PostgresRefreshTokenRepository,
        },
        donation_proof::{DisabledDonationProofPublisher, SuiDonationProofPublisher},
        donation_proof_retry::run_donation_proof_retry_worker,
        email::{BrevoEmailService, DisabledEmailService, ResendEmailService},
        payment::{DisabledPaymentGateway, PaystackPaymentGateway},
        storage::{BackblazeDocumentStorage, LocalDocumentStorage},
    },
    api::{app, AppState},
    infrastructure::{config::AppConfig, logging::init_tracing},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    let config = AppConfig::from_env()?;
    let db_pool = connect(&config.database.url).await?;
    run_migrations(&db_pool).await?;

    let hospital_repository = Arc::new(PostgresHospitalRepository::new(db_pool.clone()));
    let medical_case_repository = Arc::new(PostgresMedicalCaseRepository::new(db_pool.clone()));
    let donation_repository = Arc::new(PostgresDonationRepository::new(db_pool.clone()));
    let patient_repository = Arc::new(PostgresPatientRepository::new(db_pool.clone()));
    let patient_declaration_repository =
        Arc::new(PostgresPatientDeclarationRepository::new(db_pool.clone()));
    let refresh_token_repository = Arc::new(PostgresRefreshTokenRepository::new(db_pool.clone()));
    let password_hasher = Arc::new(Argon2PasswordHasher);
    let token_service = Arc::new(JwtTokenService::new(
        config.auth.jwt_secret.clone(),
        config.auth.jwt_expires_in_seconds,
    ));
    let document_storage = match config.storage.provider.as_str() {
        "backblaze" => Arc::new(BackblazeDocumentStorage::from_config(
            &config.storage.backblaze,
        )?) as Arc<dyn korede_backend::port::storage::DocumentStorage>,
        _ => Arc::new(LocalDocumentStorage::new(config.storage.local_root.clone()))
            as Arc<dyn korede_backend::port::storage::DocumentStorage>,
    };
    let email_service = match config.email.provider.as_str() {
        "brevo" => Arc::new(BrevoEmailService::from_config(
            &config.email,
            &config.email.brevo,
        )?) as Arc<dyn korede_backend::port::email::EmailService>,
        "resend" => Arc::new(ResendEmailService::from_config(
            &config.email,
            &config.email.resend,
        )?) as Arc<dyn korede_backend::port::email::EmailService>,
        _ => Arc::new(DisabledEmailService) as Arc<dyn korede_backend::port::email::EmailService>,
    };
    let payment_gateway = if PaystackPaymentGateway::is_configured(&config.payments) {
        Arc::new(PaystackPaymentGateway::from_config(&config.payments)?)
            as Arc<dyn korede_backend::port::payment::PaymentGateway>
    } else {
        tracing::warn!(
            "Paystack is not configured; donation payment endpoints will be unavailable"
        );
        Arc::new(DisabledPaymentGateway) as Arc<dyn korede_backend::port::payment::PaymentGateway>
    };
    let donation_proof_publisher = if SuiDonationProofPublisher::is_configured(&config.sui) {
        Arc::new(SuiDonationProofPublisher::from_config(&config.sui)?)
            as Arc<dyn korede_backend::port::sui::DonationProofPublisher>
    } else {
        tracing::warn!(
            "Sui proof publishing is not configured; donation proofs will be queued for retry"
        );
        Arc::new(DisabledDonationProofPublisher)
            as Arc<dyn korede_backend::port::sui::DonationProofPublisher>
    };

    let retry_repository = donation_repository.clone();
    let retry_publisher = donation_proof_publisher.clone();
    tokio::spawn(async move {
        run_donation_proof_retry_worker(retry_repository, retry_publisher).await;
    });

    let reservation_repository = donation_repository.clone();
    tokio::spawn(async move {
        run_checkout_reservation_expiry_worker(reservation_repository).await;
    });

    let state = AppState {
        db_pool,
        hospital_repository,
        medical_case_repository,
        donation_repository,
        patient_repository,
        patient_declaration_repository,
        refresh_token_repository,
        password_hasher,
        token_service,
        payment_gateway,
        donation_proof_publisher,
        document_storage,
        email_service,
        jwt_expires_in_seconds: config.auth.jwt_expires_in_seconds,
        refresh_token_expires_in_seconds: config.auth.refresh_token_expires_in_seconds,
        max_upload_bytes: config.storage.max_upload_bytes,
        super_admin_email: config.admin.email.clone(),
        super_admin_password: config.admin.password.clone(),
        app_base_url: config.payments.base_url.clone(),
        app_name: config.payments.app_name.clone(),
        paystack_webhook_secret: config.payments.paystack_webhook_secret.clone(),
        sui_network: config.sui.network.clone(),
    };

    let router = app(state);
    let address = config.server_addr()?;
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!("Korede backend listening on http://{address}");
    tracing::info!("Korede swagger docs listening on http://localhost:4000");

    axum::serve(listener, router).await?;
    Ok(())
}
