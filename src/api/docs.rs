// `OpenApi` is a derive macro from utoipa.
//
// It generates an OpenAPI specification from the routes and schemas we list.
use utoipa::OpenApi;

// These response structs are included as schemas in the generated API docs.
use crate::api::{
    admin::{
        AdminHospitalDocumentResponse, AdminHospitalDocumentsResponse, AdminHospitalResponse,
        AdminHospitalsResponse,
    },
    auth::{
        ForgotPasswordRequest, ForgotPasswordResponse, LoginRequest, LoginResponse,
        RefreshTokenRequest, RefreshTokenResponse, ResetPasswordRequest, ResetPasswordResponse,
    },
    health::{DatabaseHealthResponse, HealthResponse},
    hospitals::{
        Base64DocumentRequest, HospitalDocumentResponse, HospitalDocumentsResponse,
        HospitalResponse, HospitalSummaryResponse, RegisterHospitalRequest,
        RegisterHospitalResponse, ResendHospitalEmailOtpRequest, ResendHospitalEmailOtpResponse,
        VerifyHospitalEmailRequest, VerifyHospitalEmailResponse, VerifyLoginOtpRequest,
        VerifyLoginOtpResponse,
    },
    patients::{
        PatientResponse, RegisterPatientRequest, RegisterPatientResponse,
        ResendPatientEmailOtpRequest, ResendPatientEmailOtpResponse, VerifyPatientEmailRequest,
        VerifyPatientEmailResponse,
    },
};

// Generate an OpenAPI document for the backend.
//
// `derive(OpenApi)` tells utoipa to create the documentation object for us.
#[derive(OpenApi)]
#[openapi(
    // General metadata shown in Swagger UI.
    info(
        title = "Korede Backend API",
        version = "0.1.0",
        description = "API documentation for the Korede backend. Korede helps donors fund verified hospital bills while keeping transactions transparent and auditable."
    ),
    // List every handler function that should appear in the docs.
    paths(
        crate::api::health::health_check,
        crate::api::health::database_health_check,
        crate::api::auth::login,
        crate::api::auth::verify_login_otp,
        crate::api::auth::refresh_token,
        crate::api::auth::request_password_reset,
        crate::api::auth::reset_password,
        crate::api::admin::list_hospitals,
        crate::api::admin::get_hospital,
        crate::api::admin::list_hospital_documents,
        crate::api::hospitals::register_hospital,
        crate::api::hospitals::verify_hospital_email,
        crate::api::hospitals::resend_hospital_email_otp,
        crate::api::hospitals::current_hospital,
        crate::api::hospitals::list_documents,
        crate::api::patients::register_patient,
        crate::api::patients::verify_patient_email,
        crate::api::patients::resend_patient_email_otp
    ),
    // List every response/request type that should appear as a schema.
    components(
        schemas(
            HealthResponse,
            DatabaseHealthResponse,
            LoginRequest,
            LoginResponse,
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
            RegisterPatientRequest,
            RegisterPatientResponse,
            VerifyPatientEmailRequest,
            VerifyPatientEmailResponse,
            ResendPatientEmailOtpRequest,
            ResendPatientEmailOtpResponse,
            PatientResponse
        )
    ),
    modifiers(&SecurityAddon),
    // Group endpoints into named sections in Swagger UI.
    tags(
        (name = "Health", description = "Endpoints for checking whether the API and database are working."),
        (name = "Auth", description = "Centralized login endpoints for platform admins and hospitals."),
        (name = "Admin", description = "Super-admin authentication and platform administration endpoints."),
        (name = "Hospitals", description = "Hospital registration, authentication, and KYC endpoints."),
        (name = "Patients", description = "Patient registration endpoints.")
    )
)]
// Empty struct used only as a type that owns the generated OpenAPI document.
//
// You call `ApiDoc::openapi()` in `api/mod.rs`.
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
