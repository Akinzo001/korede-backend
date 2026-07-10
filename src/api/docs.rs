// `OpenApi` is a derive macro from utoipa.
//
// It generates an OpenAPI specification from the routes and schemas we list.
use utoipa::OpenApi;

// These response structs are included as schemas in the generated API docs.
use crate::api::{
    admin::{
        AdminDonationDetailResponse, AdminDonationProofRetryResponse, AdminDonationSummaryResponse,
        AdminDonationsResponse, AdminHospitalDocumentResponse, AdminHospitalDocumentsResponse,
        AdminHospitalResponse, AdminHospitalsResponse, AdminPaginationResponse,
        AdminPatientDeclarationResponse, AdminReviewHospitalDocumentRequest,
        AdminReviewHospitalDocumentResponse, AdminSettlementResponse, AdminSettlementRetryResponse,
        AdminSettlementsResponse,
    },
    auth::{
        ForgotPasswordRequest, ForgotPasswordResponse, LoginRequest, LoginResponse,
        PatientLoginMedicalCaseResponse, RefreshTokenRequest, RefreshTokenResponse,
        ResetPasswordRequest, ResetPasswordResponse,
    },
    cases::{InitializeDonationRequest, InitializeDonationResponse, PublicMedicalCaseResponse},
    health::{DatabaseHealthResponse, HealthResponse},
    hospital_settlements::{
        HospitalSettlementHistoryItemResponse, HospitalSettlementHistoryPaginationResponse,
        HospitalSettlementHistoryResponse,
    },
    hospitals::{
        Base64DocumentRequest, CreateHospitalCaseBillingItemRequest,
        CreateHospitalCaseDocumentRequest, CreateHospitalCaseRequest, CreateHospitalCaseResponse,
        HospitalActiveCaseResponse, HospitalActiveCasesResponse, HospitalCaseBillingItemResponse,
        HospitalCaseDocumentResponse, HospitalCaseResponse, HospitalCompletedCaseResponse,
        HospitalCompletedCasesResponse, HospitalDocumentResponse, HospitalDocumentsResponse,
        HospitalPatientDeclarationResponse, HospitalPatientLookupDeclarationResponse,
        HospitalPatientLookupPatientResponse, HospitalPatientLookupResponse, HospitalResponse,
        HospitalSummaryResponse, RegisterHospitalRequest, RegisterHospitalResponse,
        ResendHospitalEmailOtpRequest, ResendHospitalEmailOtpResponse, VerifyHospitalEmailRequest,
        VerifyHospitalEmailResponse, VerifyLoginOtpRequest, VerifyLoginOtpResponse,
    },
    patients::{
        PatientCaseShareLinkResponse, PatientDeclarationResponse,
        PatientDonationProgressCaseResponse, PatientDonationProgressDonorResponse,
        PatientDonationProgressResponse, PatientResponse, RegisterPatientRequest,
        RegisterPatientResponse, ResendPatientEmailOtpRequest, ResendPatientEmailOtpResponse,
        UpsertPatientDeclarationRequest, VerifyPatientEmailRequest, VerifyPatientEmailResponse,
    },
    payments::{PaystackVerificationResponse, PaystackWebhookResponse},
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Korede Backend API",
        version = "0.1.0",
        description = "API documentation for the Korede backend. Korede helps donors fund verified hospital bills while keeping transactions transparent and auditable."
    ),
    paths(
        crate::api::health::health_check,
        crate::api::health::database_health_check,
        crate::api::cases::get_case_by_public_slug,
        crate::api::cases::initialize_case_donation,
        crate::api::payments::handle_paystack_webhook,
        crate::api::payments::verify_paystack_payment,
        crate::api::auth::login,
        crate::api::auth::verify_login_otp,
        crate::api::auth::refresh_token,
        crate::api::auth::request_password_reset,
        crate::api::auth::reset_password,
        crate::api::admin::list_hospitals,
        crate::api::admin::get_hospital,
        crate::api::admin::list_hospital_documents,
        crate::api::admin::review_hospital_document,
        crate::api::admin::get_patient_declaration,
        crate::api::admin::list_admin_donations,
        crate::api::admin::get_admin_donation,
        crate::api::admin::retry_donation_proof,
        crate::api::admin::list_admin_settlements,
        crate::api::admin::list_failed_admin_settlements,
        crate::api::admin::get_admin_settlement,
        crate::api::admin::retry_admin_settlement,
        crate::api::hospitals::register_hospital,
        crate::api::hospitals::verify_hospital_email,
        crate::api::hospitals::resend_hospital_email_otp,
        crate::api::hospitals::current_hospital,
        crate::api::hospitals::list_documents,
        crate::api::hospitals::find_patient,
        crate::api::hospitals::get_patient_declaration,
        crate::api::hospitals::list_active_cases,
        crate::api::hospitals::list_completed_cases,
        crate::api::hospital_settlements::list_settlement_history,
        crate::api::hospitals::create_case,
        crate::api::patients::register_patient,
        crate::api::patients::verify_patient_email,
        crate::api::patients::resend_patient_email_otp,
        crate::api::patients::create_declaration,
        crate::api::patients::update_declaration,
        crate::api::patients::get_declaration,
        crate::api::patients::get_current_case_donation_progress,
        crate::api::patients::get_case_donation_progress,
        crate::api::patients::get_current_case_share_link,
        crate::api::patients::get_case_share_link
    ),
    components(
        schemas(
            HealthResponse,
            DatabaseHealthResponse,
            PublicMedicalCaseResponse,
            InitializeDonationRequest,
            InitializeDonationResponse,
            PaystackWebhookResponse,
            PaystackVerificationResponse,
            LoginRequest,
            LoginResponse,
            PatientLoginMedicalCaseResponse,
            RefreshTokenRequest,
            RefreshTokenResponse,
            ForgotPasswordRequest,
            ForgotPasswordResponse,
            ResetPasswordRequest,
            ResetPasswordResponse,
            AdminHospitalResponse,
            AdminHospitalsResponse,
            AdminHospitalDocumentResponse,
            AdminHospitalDocumentsResponse,
            AdminReviewHospitalDocumentRequest,
            AdminReviewHospitalDocumentResponse,
            AdminPatientDeclarationResponse,
            AdminDonationsResponse,
            AdminPaginationResponse,
            AdminDonationSummaryResponse,
            AdminDonationDetailResponse,
            AdminDonationProofRetryResponse,
            AdminSettlementsResponse,
            AdminSettlementResponse,
            AdminSettlementRetryResponse,
            Base64DocumentRequest,
            RegisterHospitalRequest,
            RegisterHospitalResponse,
            VerifyHospitalEmailRequest,
            VerifyHospitalEmailResponse,
            ResendHospitalEmailOtpRequest,
            ResendHospitalEmailOtpResponse,
            HospitalResponse,
            VerifyLoginOtpRequest,
            VerifyLoginOtpResponse,
            HospitalSummaryResponse,
            HospitalDocumentResponse,
            HospitalDocumentsResponse,
            HospitalPatientDeclarationResponse,
            HospitalPatientLookupResponse,
            HospitalPatientLookupPatientResponse,
            HospitalPatientLookupDeclarationResponse,
            HospitalActiveCasesResponse,
            HospitalActiveCaseResponse,
            HospitalCompletedCasesResponse,
            HospitalCompletedCaseResponse,
            HospitalSettlementHistoryResponse,
            HospitalSettlementHistoryPaginationResponse,
            HospitalSettlementHistoryItemResponse,
            CreateHospitalCaseRequest,
            CreateHospitalCaseBillingItemRequest,
            CreateHospitalCaseDocumentRequest,
            CreateHospitalCaseResponse,
            HospitalCaseResponse,
            HospitalCaseBillingItemResponse,
            HospitalCaseDocumentResponse,
            RegisterPatientRequest,
            RegisterPatientResponse,
            VerifyPatientEmailRequest,
            VerifyPatientEmailResponse,
            ResendPatientEmailOtpRequest,
            ResendPatientEmailOtpResponse,
            UpsertPatientDeclarationRequest,
            PatientDeclarationResponse,
            PatientDonationProgressResponse,
            PatientDonationProgressCaseResponse,
            PatientDonationProgressDonorResponse,
            PatientCaseShareLinkResponse,
            PatientResponse
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Health", description = "Endpoints for checking whether the API and database are working."),
        (name = "Cases", description = "Public medical case pages for donor-facing case links."),
        (name = "Payments", description = "Public payment and payment provider webhook endpoints."),
        (name = "Auth", description = "Centralized login endpoints for platform admins and hospitals."),
        (name = "Admin", description = "Super-admin authentication and platform administration endpoints."),
        (name = "Hospitals", description = "Hospital registration, authentication, and KYC endpoints."),
        (name = "Patients", description = "Patient registration endpoints.")
    )
)]
pub struct ApiDoc;

pub struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            let mut bearer = utoipa::openapi::security::Http::new(
                utoipa::openapi::security::HttpAuthScheme::Bearer,
            );
            bearer.bearer_format = Some("JWT".to_owned());

            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(bearer),
            );
        }
    }
}
