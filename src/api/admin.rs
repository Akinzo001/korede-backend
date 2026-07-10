use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::{error::ApiError, AppState},
    application::{
        admin_donations::AdminDonationListCommand, admin_hospitals::ReviewHospitalDocumentCommand,
    },
    domain::{
        donation::{Donation, DonationStatus},
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::HospitalDocument,
        patient_declaration::PatientDeclaration,
        settlement::{HospitalSettlement, HospitalSettlementStatus},
    },
    port::{
        auth::AuthenticatedAdmin,
        donation::AdminDonationOperation,
        hospital::HospitalRepositoryError,
        settlement::{AdminSettlementListQuery, AdminSettlementOperation},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/donations", get(list_admin_donations))
        .route("/donations/:donation_id", get(get_admin_donation))
        .route(
            "/donations/:donation_id/proof/retry",
            post(retry_donation_proof),
        )
        .route("/settlements", get(list_admin_settlements))
        .route("/settlements/failed", get(list_failed_admin_settlements))
        .route("/settlements/:settlement_id", get(get_admin_settlement))
        .route(
            "/settlements/:settlement_id/retry",
            post(retry_admin_settlement),
        )
        .route("/hospitals", get(list_hospitals))
        .route("/hospitals/:hospital_id", get(get_hospital))
        .route(
            "/hospitals/:hospital_id/documents",
            get(list_hospital_documents),
        )
        .route(
            "/hospitals/:hospital_id/documents/:document_id/review",
            patch(review_hospital_document),
        )
        .route(
            "/patients/:username/declaration",
            get(get_patient_declaration),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminLoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub phone_number: Option<String>,
    pub official_address: Option<String>,
    pub administrator_name: Option<String>,
    pub cac_registration_number: Option<String>,
    pub medical_license_number: Option<String>,
    pub corporate_account_name: String,
    pub corporate_account_number: String,
    pub corporate_bank_code: Option<String>,
    pub bank_name: String,
    pub verification_status: HospitalVerificationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalsResponse {
    pub hospitals: Vec<AdminHospitalResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalDocumentResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub document_type: String,
    pub storage_provider: String,
    pub storage_key: String,
    pub status: String,
    pub original_filename: String,
    pub mime_type: String,
    pub file_size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_message: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalDocumentsResponse {
    pub documents: Vec<AdminHospitalDocumentResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminReviewHospitalDocumentRequest {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminReviewHospitalDocumentResponse {
    pub document: AdminHospitalDocumentResponse,
    pub hospital_verification_status: HospitalVerificationStatus,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPatientDeclarationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub statement: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminDonationListParams {
    pub status: Option<String>,
    pub method: Option<String>,
    pub proof_status: Option<String>,
    pub hospital_id: Option<Uuid>,
    pub medical_case_id: Option<Uuid>,
    pub paystack_reference: Option<String>,
    pub is_late_payment: Option<bool>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDonationsResponse {
    pub donations: Vec<AdminDonationSummaryResponse>,
    pub pagination: AdminPaginationResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPaginationResponse {
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDonationSummaryResponse {
    pub id: Uuid,
    pub amount_kobo: i64,
    pub status: String,
    pub method: String,
    pub donor_display_name: String,
    pub donor_email: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub reservation_expires_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub is_late_payment: bool,
    pub payment_note: Option<String>,
    pub requires_admin_action: bool,
    pub created_at: DateTime<Utc>,
    pub paystack_reference: String,
    pub paystack_transaction_reference: Option<String>,
    pub proof_status: String,
    pub sui_network: Option<String>,
    pub sui_tx_digest: Option<String>,
    pub sui_transaction_url: Option<String>,
    pub proof_attempt_count: i32,
    pub proof_next_retry_at: Option<DateTime<Utc>>,
    pub proof_last_error: Option<String>,
    pub proof_published_at: Option<DateTime<Utc>>,
    pub medical_case_id: Uuid,
    pub case_title: String,
    pub public_slug: Option<String>,
    pub hospital_id: Uuid,
    pub hospital_name: String,
    pub patient_id: Uuid,
    pub patient_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDonationDetailResponse {
    #[serde(flatten)]
    pub summary: AdminDonationSummaryResponse,
    pub paystack_access_code: Option<String>,
    pub paystack_authorization_url: Option<String>,
    pub paystack_customer_code: Option<String>,
    pub paystack_dedicated_account_id: Option<i64>,
    pub account_number: Option<String>,
    pub account_name: Option<String>,
    pub bank_name: Option<String>,
    pub bank_slug: Option<String>,
    pub bill_amount_kobo: i64,
    pub amount_raised_kobo: i64,
    pub remaining_amount_kobo: i64,
    pub case_status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDonationProofRetryResponse {
    pub message: String,
    pub proof_status: String,
    pub sui_network: Option<String>,
    pub sui_tx_digest: Option<String>,
    pub sui_transaction_url: Option<String>,
    pub proof_attempt_count: i32,
    pub proof_next_retry_at: Option<DateTime<Utc>>,
    pub proof_last_error: Option<String>,
    pub proof_published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminSettlementListParams {
    pub status: Option<String>,
    pub hospital_id: Option<Uuid>,
    pub medical_case_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSettlementsResponse {
    pub settlements: Vec<AdminSettlementResponse>,
    pub pagination: AdminPaginationResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSettlementResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub hospital_name: String,
    pub medical_case_id: Uuid,
    pub case_title: String,
    pub public_slug: Option<String>,
    pub patient_id: Uuid,
    pub patient_name: String,
    pub amount_kobo: i64,
    pub status: String,
    pub settlement_reference: String,
    pub bank_name: String,
    pub bank_code: Option<String>,
    pub account_name: String,
    pub account_number: String,
    pub paystack_recipient_code: Option<String>,
    pub paystack_transfer_code: Option<String>,
    pub paystack_transfer_id: Option<i64>,
    pub paystack_status: Option<String>,
    pub failure_reason: Option<String>,
    pub initiated_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSettlementRetryResponse {
    pub message: String,
    pub settlement: AdminSettlementResponse,
}

pub async fn login_admin(
    State(state): State<AppState>,
    Json(request): Json<AdminLoginRequest>,
) -> Result<Json<AdminLoginResponse>, ApiError> {
    validate_admin_login_request(&request)?;

    let email = request.email.trim().to_lowercase();
    let password = request.password.trim();

    if email != state.super_admin_email
        || !constant_time_eq(password.as_bytes(), state.super_admin_password.as_bytes())
    {
        return Err(invalid_admin_credentials());
    }

    let access_token = state
        .token_service
        .create_admin_access_token(&state.super_admin_email)
        .map_err(|_| ApiError::Internal("failed to create access token".to_owned()))?;

    Ok(Json(AdminLoginResponse {
        access_token,
        token_type: "Bearer".to_owned(),
        expires_in: state.jwt_expires_in_seconds,
        role: "admin".to_owned(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/donations",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(AdminDonationListParams),
    responses(
        (status = 200, description = "Donation operations list.", body = AdminDonationsResponse),
        (status = 400, description = "Invalid donation filter."),
        (status = 401, description = "Missing or invalid admin bearer token.")
    )
)]
pub async fn list_admin_donations(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Query(params): Query<AdminDonationListParams>,
) -> Result<Json<AdminDonationsResponse>, ApiError> {
    let result = state
        .admin_donation_service
        .list_donations(AdminDonationListCommand {
            status: params.status,
            method: params.method,
            proof_status: params.proof_status,
            hospital_id: params.hospital_id,
            medical_case_id: params.medical_case_id,
            paystack_reference: params.paystack_reference,
            is_late_payment: params.is_late_payment,
            from: params.from,
            to: params.to,
            limit: params.limit,
            offset: params.offset,
        })
        .await?;
    let donations = result
        .donations
        .into_iter()
        .map(AdminDonationSummaryResponse::from)
        .collect();

    Ok(Json(AdminDonationsResponse {
        donations,
        pagination: AdminPaginationResponse {
            limit: result.limit,
            offset: result.offset,
            total: result.total,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/donations/{donation_id}",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("donation_id" = Uuid, Path, description = "Donation ID")
    ),
    responses(
        (status = 200, description = "Donation operation detail.", body = AdminDonationDetailResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Donation not found.")
    )
)]
pub async fn get_admin_donation(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(donation_id): Path<Uuid>,
) -> Result<Json<AdminDonationDetailResponse>, ApiError> {
    let donation = state
        .admin_donation_service
        .get_donation(donation_id)
        .await?;

    Ok(Json(AdminDonationDetailResponse::from(donation)))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/donations/{donation_id}/proof/retry",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("donation_id" = Uuid, Path, description = "Donation ID")
    ),
    responses(
        (status = 200, description = "Donation proof retry result.", body = AdminDonationProofRetryResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Donation not found."),
        (status = 409, description = "Donation is not retryable.")
    )
)]
pub async fn retry_donation_proof(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(donation_id): Path<Uuid>,
) -> Result<Json<AdminDonationProofRetryResponse>, ApiError> {
    let result = state
        .admin_donation_service
        .retry_donation_proof(donation_id)
        .await?;
    Ok(Json(AdminDonationProofRetryResponse::from_parts(
        result.message,
        result.donation,
    )))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/settlements",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(AdminSettlementListParams),
    responses(
        (status = 200, description = "Hospital settlement operations list.", body = AdminSettlementsResponse),
        (status = 400, description = "Invalid settlement filter."),
        (status = 401, description = "Missing or invalid admin bearer token.")
    )
)]
pub async fn list_admin_settlements(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Query(params): Query<AdminSettlementListParams>,
) -> Result<Json<AdminSettlementsResponse>, ApiError> {
    let query = admin_settlement_list_query(params, false)?;
    let total = state
        .settlement_repository
        .count_admin_settlements(query.clone())
        .await?;
    let settlements = state
        .settlement_repository
        .list_admin_settlements(query.clone())
        .await?
        .into_iter()
        .map(AdminSettlementResponse::from)
        .collect();

    Ok(Json(AdminSettlementsResponse {
        settlements,
        pagination: AdminPaginationResponse {
            limit: query.limit,
            offset: query.offset,
            total,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/settlements/failed",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(AdminSettlementListParams),
    responses(
        (status = 200, description = "Failed or admin-action hospital settlements.", body = AdminSettlementsResponse),
        (status = 400, description = "Invalid settlement filter."),
        (status = 401, description = "Missing or invalid admin bearer token.")
    )
)]
pub async fn list_failed_admin_settlements(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Query(params): Query<AdminSettlementListParams>,
) -> Result<Json<AdminSettlementsResponse>, ApiError> {
    let query = admin_settlement_list_query(params, true)?;
    let total = state
        .settlement_repository
        .count_admin_settlements(query.clone())
        .await?;
    let settlements = state
        .settlement_repository
        .list_admin_settlements(query.clone())
        .await?
        .into_iter()
        .map(AdminSettlementResponse::from)
        .collect();

    Ok(Json(AdminSettlementsResponse {
        settlements,
        pagination: AdminPaginationResponse {
            limit: query.limit,
            offset: query.offset,
            total,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/settlements/{settlement_id}",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("settlement_id" = Uuid, Path, description = "Hospital settlement ID")
    ),
    responses(
        (status = 200, description = "Hospital settlement operation detail.", body = AdminSettlementResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital settlement not found.")
    )
)]
pub async fn get_admin_settlement(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(settlement_id): Path<Uuid>,
) -> Result<Json<AdminSettlementResponse>, ApiError> {
    let settlement = state
        .settlement_repository
        .get_admin_settlement(settlement_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("hospital settlement not found".to_owned()))?;

    Ok(Json(AdminSettlementResponse::from(settlement)))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/settlements/{settlement_id}/retry",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("settlement_id" = Uuid, Path, description = "Hospital settlement ID")
    ),
    responses(
        (status = 200, description = "Hospital settlement retry result.", body = AdminSettlementRetryResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital settlement not found."),
        (status = 409, description = "Hospital settlement is not retryable.")
    )
)]
pub async fn retry_admin_settlement(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(settlement_id): Path<Uuid>,
) -> Result<Json<AdminSettlementRetryResponse>, ApiError> {
    let settlement = state
        .settlement_repository
        .get_settlement(settlement_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("hospital settlement not found".to_owned()))?;
    validate_settlement_retry_target(&settlement)?;

    let medical_case = state
        .medical_case_repository
        .find_case_by_id(settlement.medical_case_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("medical case not found".to_owned()))?;

    let updated = state
        .payment_service
        .process_hospital_settlement_for_case(&medical_case)
        .await?;
    let operation = state
        .settlement_repository
        .get_admin_settlement(updated.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("hospital settlement not found".to_owned()))?;

    Ok(Json(AdminSettlementRetryResponse {
        message: settlement_retry_message(&updated.status).to_owned(),
        settlement: AdminSettlementResponse::from(operation),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals",
    tag = "Admin",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "All registered hospitals.", body = AdminHospitalsResponse),
        (status = 401, description = "Missing or invalid admin bearer token.")
    )
)]
pub async fn list_hospitals(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
) -> Result<Json<AdminHospitalsResponse>, ApiError> {
    let hospitals = state
        .hospital_repository
        .list_hospitals()
        .await?
        .into_iter()
        .map(AdminHospitalResponse::from)
        .collect();

    Ok(Json(AdminHospitalsResponse { hospitals }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals/{hospital_id}",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("hospital_id" = Uuid, Path, description = "Hospital ID")
    ),
    responses(
        (status = 200, description = "Hospital details.", body = AdminHospitalResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital not found.")
    )
)]
pub async fn get_hospital(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(hospital_id): Path<Uuid>,
) -> Result<Json<AdminHospitalResponse>, ApiError> {
    let hospital = state
        .hospital_repository
        .find_hospital_by_id(hospital_id)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    Ok(Json(AdminHospitalResponse::from(hospital)))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals/{hospital_id}/documents",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("hospital_id" = Uuid, Path, description = "Hospital ID")
    ),
    responses(
        (status = 200, description = "Hospital document metadata.", body = AdminHospitalDocumentsResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital not found.")
    )
)]
pub async fn list_hospital_documents(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(hospital_id): Path<Uuid>,
) -> Result<Json<AdminHospitalDocumentsResponse>, ApiError> {
    state
        .hospital_repository
        .find_hospital_by_id(hospital_id)
        .await?
        .ok_or(HospitalRepositoryError::NotFound)?;

    let documents = state
        .hospital_repository
        .list_hospital_documents(hospital_id)
        .await?
        .into_iter()
        .map(AdminHospitalDocumentResponse::from)
        .collect();

    Ok(Json(AdminHospitalDocumentsResponse { documents }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/hospitals/{hospital_id}/documents/{document_id}/review",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("hospital_id" = Uuid, Path, description = "Hospital ID"),
        ("document_id" = Uuid, Path, description = "Hospital document ID")
    ),
    request_body = AdminReviewHospitalDocumentRequest,
    responses(
        (status = 200, description = "Hospital document reviewed.", body = AdminReviewHospitalDocumentResponse),
        (status = 400, description = "Invalid review request."),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Hospital or document not found.")
    )
)]
pub async fn review_hospital_document(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path((hospital_id, document_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<AdminReviewHospitalDocumentRequest>,
) -> Result<Json<AdminReviewHospitalDocumentResponse>, ApiError> {
    let result = state
        .admin_hospital_service
        .review_hospital_document(
            hospital_id,
            document_id,
            ReviewHospitalDocumentCommand {
                status: request.status,
                message: request.message,
            },
        )
        .await?;

    Ok(Json(AdminReviewHospitalDocumentResponse {
        document: AdminHospitalDocumentResponse::from(result.document),
        hospital_verification_status: result.hospital_verification_status,
        message: result.message,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/patients/{username}/declaration",
    tag = "Admin",
    security(("bearer_auth" = [])),
    params(
        ("username" = String, Path, description = "Patient username")
    ),
    responses(
        (status = 200, description = "Patient declaration by username.", body = AdminPatientDeclarationResponse),
        (status = 401, description = "Missing or invalid admin bearer token."),
        (status = 404, description = "Patient declaration was not found.")
    )
)]
pub async fn get_patient_declaration(
    _admin: AuthenticatedAdmin,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<AdminPatientDeclarationResponse>, ApiError> {
    let declaration = state
        .patient_declaration_repository
        .find_current_patient_declaration_by_username(&username)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    Ok(Json(AdminPatientDeclarationResponse::from(declaration)))
}

fn admin_settlement_list_query(
    params: AdminSettlementListParams,
    admin_action_required_only: bool,
) -> Result<AdminSettlementListQuery, ApiError> {
    let status = parse_optional_settlement_status(params.status.as_deref())?;
    if admin_action_required_only
        && status
            .as_ref()
            .is_some_and(|status| !status.requires_admin_action())
    {
        return Err(ApiError::BadRequest(
            "status must be one of failed, failed_config, bank_details_required, reversed, or otp_required"
                .to_owned(),
        ));
    }

    Ok(AdminSettlementListQuery {
        status,
        hospital_id: params.hospital_id,
        medical_case_id: params.medical_case_id,
        from: params.from,
        to: params.to,
        admin_action_required_only,
        limit: normalize_limit(params.limit)?,
        offset: normalize_offset(params.offset)?,
    })
}

fn normalize_limit(limit: Option<i64>) -> Result<i64, ApiError> {
    match limit {
        Some(value) if value < 1 => Err(ApiError::BadRequest(
            "limit must be greater than zero".to_owned(),
        )),
        Some(value) => Ok(value.min(100)),
        None => Ok(50),
    }
}

fn normalize_offset(offset: Option<i64>) -> Result<i64, ApiError> {
    match offset {
        Some(value) if value < 0 => Err(ApiError::BadRequest(
            "offset must be zero or greater".to_owned(),
        )),
        Some(value) => Ok(value),
        None => Ok(0),
    }
}

fn parse_optional_settlement_status(
    value: Option<&str>,
) -> Result<Option<HospitalSettlementStatus>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let status = match value {
        "pending" => HospitalSettlementStatus::Pending,
        "recipient_created" => HospitalSettlementStatus::RecipientCreated,
        "processing" => HospitalSettlementStatus::Processing,
        "otp_required" => HospitalSettlementStatus::OtpRequired,
        "paid" => HospitalSettlementStatus::Paid,
        "failed" => HospitalSettlementStatus::Failed,
        "reversed" => HospitalSettlementStatus::Reversed,
        "failed_config" => HospitalSettlementStatus::FailedConfig,
        "bank_details_required" => HospitalSettlementStatus::BankDetailsRequired,
        _ => {
            return Err(ApiError::BadRequest(
                "invalid settlement status filter".to_owned(),
            ))
        }
    };

    Ok(Some(status))
}

fn validate_settlement_retry_target(settlement: &HospitalSettlement) -> Result<(), ApiError> {
    match settlement.status {
        HospitalSettlementStatus::Paid => {
            return Err(ApiError::Conflict(
                "hospital settlement is already paid".to_owned(),
            ))
        }
        HospitalSettlementStatus::Processing => {
            return Err(ApiError::Conflict(
                "hospital settlement is still processing".to_owned(),
            ))
        }
        HospitalSettlementStatus::OtpRequired => {
            return Err(ApiError::Conflict(
                "hospital settlement requires Paystack OTP finalization".to_owned(),
            ))
        }
        _ => {}
    }

    if !settlement.status.can_retry() {
        return Err(ApiError::Conflict(
            "hospital settlement is not retryable right now".to_owned(),
        ));
    }

    Ok(())
}

fn settlement_retry_message(status: &HospitalSettlementStatus) -> &'static str {
    match status {
        HospitalSettlementStatus::Paid => "paid",
        HospitalSettlementStatus::Processing => "processing",
        HospitalSettlementStatus::OtpRequired => "otp_required",
        HospitalSettlementStatus::FailedConfig => "failed_config",
        HospitalSettlementStatus::BankDetailsRequired => "bank_details_required",
        HospitalSettlementStatus::Failed => "failed",
        HospitalSettlementStatus::Reversed => "reversed",
        HospitalSettlementStatus::Pending | HospitalSettlementStatus::RecipientCreated => "pending",
    }
}

fn suiscan_transaction_url(network: &str, tx_digest: &str) -> String {
    format!("https://suiscan.xyz/{network}/tx/{tx_digest}")
}

fn validate_admin_login_request(request: &AdminLoginRequest) -> Result<(), ApiError> {
    if request.email.trim().is_empty() || !request.email.contains('@') {
        return Err(ApiError::BadRequest("email is invalid".to_owned()));
    }

    if request.password.trim().is_empty() {
        return Err(ApiError::BadRequest("password is required".to_owned()));
    }

    Ok(())
}

fn invalid_admin_credentials() -> ApiError {
    ApiError::Unauthorized("invalid admin credentials".to_owned())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    let max_len = left.len().max(right.len());

    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        diff |= (left_byte ^ right_byte) as usize;
    }

    diff == 0
}

impl From<Hospital> for AdminHospitalResponse {
    fn from(hospital: Hospital) -> Self {
        Self {
            id: hospital.id,
            name: hospital.name,
            email: hospital.email,
            email_verified: hospital.email_verified,
            email_verified_at: hospital.email_verified_at,
            phone_number: hospital.phone_number,
            official_address: hospital.official_address,
            administrator_name: hospital.administrator_name,
            cac_registration_number: hospital.cac_registration_number,
            medical_license_number: hospital.medical_license_number,
            corporate_account_name: hospital.corporate_account_name,
            corporate_account_number: hospital.corporate_account_number,
            corporate_bank_code: hospital.corporate_bank_code,
            bank_name: hospital.bank_name,
            verification_status: hospital.verification_status,
            created_at: hospital.created_at,
            updated_at: hospital.updated_at,
        }
    }
}

impl From<HospitalDocument> for AdminHospitalDocumentResponse {
    fn from(document: HospitalDocument) -> Self {
        Self {
            id: document.id,
            hospital_id: document.hospital_id,
            document_type: document.document_type.as_str().to_owned(),
            storage_provider: document.storage_provider.as_str().to_owned(),
            storage_key: document.storage_key,
            status: document.status.as_str().to_owned(),
            original_filename: document.original_filename,
            mime_type: document.mime_type,
            file_size_bytes: document.file_size_bytes,
            uploaded_at: document.uploaded_at,
            reviewed_at: document.reviewed_at,
            review_message: document.review_message,
        }
    }
}

impl From<PatientDeclaration> for AdminPatientDeclarationResponse {
    fn from(declaration: PatientDeclaration) -> Self {
        Self {
            id: declaration.id,
            patient_id: declaration.patient_id,
            statement: declaration.statement,
            created_at: declaration.created_at,
            updated_at: declaration.updated_at,
        }
    }
}

impl From<AdminDonationOperation> for AdminDonationSummaryResponse {
    fn from(operation: AdminDonationOperation) -> Self {
        let donation = operation.donation;
        let sui_transaction_url = donation
            .sui_network
            .as_deref()
            .zip(donation.sui_tx_digest.as_deref())
            .map(|(network, digest)| suiscan_transaction_url(network, digest));

        Self {
            id: donation.id,
            amount_kobo: donation.amount_kobo,
            status: donation.status.as_str().to_owned(),
            method: donation.method.as_str().to_owned(),
            donor_display_name: donation.donor_display_name,
            donor_email: donation.donor_email,
            paid_at: donation.paid_at,
            reservation_expires_at: donation.reservation_expires_at,
            expired_at: donation.expired_at,
            is_late_payment: donation.is_late_payment,
            payment_note: donation.payment_note,
            requires_admin_action: donation.status == DonationStatus::RejectedOverflow
                && donation.paystack_transaction_reference.is_some(),
            created_at: donation.created_at,
            paystack_reference: donation.paystack_reference,
            paystack_transaction_reference: donation.paystack_transaction_reference,
            proof_status: donation.proof_status.as_str().to_owned(),
            sui_network: donation.sui_network,
            sui_tx_digest: donation.sui_tx_digest,
            sui_transaction_url,
            proof_attempt_count: donation.proof_attempt_count,
            proof_next_retry_at: donation.proof_next_retry_at,
            proof_last_error: donation.proof_last_error,
            proof_published_at: donation.proof_published_at,
            medical_case_id: donation.medical_case_id,
            case_title: operation.case_title,
            public_slug: operation.public_slug,
            hospital_id: operation.hospital_id,
            hospital_name: operation.hospital_name,
            patient_id: operation.patient_id,
            patient_name: operation.patient_name,
        }
    }
}

impl From<AdminDonationOperation> for AdminDonationDetailResponse {
    fn from(operation: AdminDonationOperation) -> Self {
        let paystack_access_code = operation.donation.paystack_access_code.clone();
        let paystack_authorization_url = operation.donation.paystack_authorization_url.clone();
        let paystack_customer_code = operation.donation.paystack_customer_code.clone();
        let paystack_dedicated_account_id = operation.donation.paystack_dedicated_account_id;
        let account_number = operation.donation.paystack_dedicated_account_number.clone();
        let account_name = operation.donation.paystack_dedicated_account_name.clone();
        let bank_name = operation.donation.paystack_dedicated_bank_name.clone();
        let bank_slug = operation.donation.paystack_dedicated_bank_slug.clone();
        let bill_amount_kobo = operation.bill_amount_kobo;
        let amount_raised_kobo = operation.amount_raised_kobo;
        let remaining_amount_kobo = (bill_amount_kobo - amount_raised_kobo).max(0);
        let case_status = operation.case_status.clone();

        Self {
            summary: AdminDonationSummaryResponse::from(operation),
            paystack_access_code,
            paystack_authorization_url,
            paystack_customer_code,
            paystack_dedicated_account_id,
            account_number,
            account_name,
            bank_name,
            bank_slug,
            bill_amount_kobo,
            amount_raised_kobo,
            remaining_amount_kobo,
            case_status,
        }
    }
}

impl From<AdminSettlementOperation> for AdminSettlementResponse {
    fn from(operation: AdminSettlementOperation) -> Self {
        let settlement = operation.settlement;
        Self {
            id: settlement.id,
            hospital_id: settlement.hospital_id,
            hospital_name: operation.hospital_name,
            medical_case_id: settlement.medical_case_id,
            case_title: operation.case_title,
            public_slug: operation.public_slug,
            patient_id: operation.patient_id,
            patient_name: operation.patient_name,
            amount_kobo: settlement.amount_kobo,
            status: settlement.status.as_str().to_owned(),
            settlement_reference: settlement.settlement_reference,
            bank_name: settlement.bank_name,
            bank_code: settlement.bank_code,
            account_name: settlement.account_name,
            account_number: settlement.account_number,
            paystack_recipient_code: settlement.paystack_recipient_code,
            paystack_transfer_code: settlement.paystack_transfer_code,
            paystack_transfer_id: settlement.paystack_transfer_id,
            paystack_status: settlement.paystack_status,
            failure_reason: settlement.failure_reason,
            initiated_at: settlement.initiated_at,
            paid_at: settlement.paid_at,
            failed_at: settlement.failed_at,
            created_at: settlement.created_at,
            updated_at: settlement.updated_at,
        }
    }
}

impl AdminDonationProofRetryResponse {
    fn from_parts(message: String, donation: Donation) -> Self {
        let sui_transaction_url = donation
            .sui_network
            .as_deref()
            .zip(donation.sui_tx_digest.as_deref())
            .map(|(network, digest)| suiscan_transaction_url(network, digest));

        Self {
            message,
            proof_status: donation.proof_status.as_str().to_owned(),
            sui_network: donation.sui_network,
            sui_tx_digest: donation.sui_tx_digest,
            sui_transaction_url,
            proof_attempt_count: donation.proof_attempt_count,
            proof_next_retry_at: donation.proof_next_retry_at,
            proof_last_error: donation.proof_last_error,
            proof_published_at: donation.proof_published_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::hospital_document::{
        HospitalDocumentStatus, HospitalDocumentType, StorageProvider,
    };
    use chrono::TimeZone;

    #[test]
    fn validates_admin_login_request() {
        let request = AdminLoginRequest {
            email: "admin@example.com".to_owned(),
            password: "password".to_owned(),
        };

        assert!(validate_admin_login_request(&request).is_ok());
    }

    #[test]
    fn rejects_invalid_admin_email() {
        let request = AdminLoginRequest {
            email: "admin".to_owned(),
            password: "password".to_owned(),
        };

        assert!(matches!(
            validate_admin_login_request(&request),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn constant_time_eq_matches_equal_values() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"wrong"));
        assert!(!constant_time_eq(b"secret", b"secret1"));
    }

    #[test]
    fn admin_failed_settlement_query_accepts_only_admin_action_statuses() {
        let query = admin_settlement_list_query(
            AdminSettlementListParams {
                status: Some("failed_config".to_owned()),
                hospital_id: None,
                medical_case_id: None,
                from: None,
                to: None,
                limit: None,
                offset: None,
            },
            true,
        )
        .expect("failed_config should be valid for failed settlement list");

        assert!(query.admin_action_required_only);
        assert_eq!(query.status, Some(HospitalSettlementStatus::FailedConfig));

        let error = admin_settlement_list_query(
            AdminSettlementListParams {
                status: Some("paid".to_owned()),
                hospital_id: None,
                medical_case_id: None,
                from: None,
                to: None,
                limit: None,
                offset: None,
            },
            true,
        )
        .expect_err("paid is not a failed/admin-action status");

        assert!(matches!(error, ApiError::BadRequest(_)));
    }

    #[test]
    fn admin_settlement_pagination_defaults_and_clamps_limit() {
        let default_query = admin_settlement_list_query(
            AdminSettlementListParams {
                status: None,
                hospital_id: None,
                medical_case_id: None,
                from: None,
                to: None,
                limit: None,
                offset: None,
            },
            false,
        )
        .expect("default pagination should be valid");

        assert_eq!(default_query.limit, 50);
        assert_eq!(default_query.offset, 0);

        let clamped_query = admin_settlement_list_query(
            AdminSettlementListParams {
                status: None,
                hospital_id: None,
                medical_case_id: None,
                from: None,
                to: None,
                limit: Some(500),
                offset: Some(10),
            },
            true,
        )
        .expect("large limit should be clamped");

        assert_eq!(clamped_query.limit, 100);
        assert_eq!(clamped_query.offset, 10);
    }

    #[test]
    fn admin_suiscan_url_uses_network_and_digest() {
        assert_eq!(
            suiscan_transaction_url("testnet", "abc123"),
            "https://suiscan.xyz/testnet/tx/abc123"
        );
    }

    #[test]
    fn settlement_retry_rejects_paid_processing_and_otp_required() {
        for status in [
            HospitalSettlementStatus::Paid,
            HospitalSettlementStatus::Processing,
            HospitalSettlementStatus::OtpRequired,
        ] {
            let settlement = test_settlement(status);

            assert!(matches!(
                validate_settlement_retry_target(&settlement),
                Err(ApiError::Conflict(_))
            ));
        }
    }

    #[test]
    fn settlement_retry_accepts_failed_admin_action_statuses() {
        for status in [
            HospitalSettlementStatus::Failed,
            HospitalSettlementStatus::FailedConfig,
            HospitalSettlementStatus::BankDetailsRequired,
            HospitalSettlementStatus::Reversed,
        ] {
            let settlement = test_settlement(status);

            assert!(validate_settlement_retry_target(&settlement).is_ok());
        }
    }

    #[test]
    fn admin_document_response_includes_review_message() {
        let document = test_hospital_document(
            Uuid::new_v4(),
            HospitalDocumentType::MedicalLicense,
            HospitalDocumentStatus::Rejected,
            Some("blurry image".to_owned()),
        );

        let response = AdminHospitalDocumentResponse::from(document);

        assert_eq!(response.review_message.as_deref(), Some("blurry image"));
    }

    fn test_settlement(status: HospitalSettlementStatus) -> HospitalSettlement {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        HospitalSettlement {
            id: Uuid::new_v4(),
            hospital_id: Uuid::new_v4(),
            medical_case_id: Uuid::new_v4(),
            amount_kobo: 500_000,
            status,
            settlement_reference: "korede-settlement-test".to_owned(),
            bank_name: "Wema Bank".to_owned(),
            bank_code: Some("035".to_owned()),
            account_name: "Lagoon Hospital".to_owned(),
            account_number: "0123456789".to_owned(),
            paystack_recipient_code: Some("RCP_test".to_owned()),
            paystack_transfer_code: Some("TRF_test".to_owned()),
            paystack_transfer_id: Some(123),
            paystack_status: None,
            failure_reason: None,
            initiated_at: Some(now),
            paid_at: None,
            failed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn test_hospital_document(
        hospital_id: Uuid,
        document_type: HospitalDocumentType,
        status: HospitalDocumentStatus,
        review_message: Option<String>,
    ) -> HospitalDocument {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        HospitalDocument {
            id: Uuid::new_v4(),
            hospital_id,
            document_type,
            storage_provider: StorageProvider::Local,
            storage_key: "hospital/doc.pdf".to_owned(),
            original_filename: "doc.pdf".to_owned(),
            mime_type: "application/pdf".to_owned(),
            file_size_bytes: 1024,
            status,
            uploaded_at: now,
            reviewed_at: Some(now),
            review_message,
        }
    }
}
