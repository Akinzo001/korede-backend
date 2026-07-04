use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    adapters::donation_proof_retry::next_retry_at,
    api::{error::ApiError, AppState},
    domain::{
        donation::{Donation, DonationProofStatus, DonationStatus},
        hospital::{Hospital, HospitalVerificationStatus},
        hospital_document::HospitalDocument,
        patient_declaration::PatientDeclaration,
    },
    port::{
        auth::AuthenticatedAdmin,
        donation::{
            AdminDonationFilters, AdminDonationListQuery, AdminDonationOperation,
            DonationProofAttemptUpdate,
        },
        hospital::HospitalRepositoryError,
        sui::{DonationProofError, DonationProofReceipt, DonationProofRequest},
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
        .route("/hospitals", get(list_hospitals))
        .route("/hospitals/:hospital_id", get(get_hospital))
        .route(
            "/hospitals/:hospital_id/documents",
            get(list_hospital_documents),
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminHospitalDocumentsResponse {
    pub documents: Vec<AdminHospitalDocumentResponse>,
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
    let query = admin_donation_list_query(params)?;
    let total = state
        .donation_repository
        .count_admin_donations(query.filters.clone())
        .await?;
    let donations = state
        .donation_repository
        .list_admin_donations(query.clone())
        .await?
        .into_iter()
        .map(AdminDonationSummaryResponse::from)
        .collect();

    Ok(Json(AdminDonationsResponse {
        donations,
        pagination: AdminPaginationResponse {
            limit: query.limit,
            offset: query.offset,
            total,
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
        .donation_repository
        .get_admin_donation(donation_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("donation not found".to_owned()))?;

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
    let operation = state
        .donation_repository
        .get_admin_donation(donation_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("donation not found".to_owned()))?;
    validate_manual_retry_target(&operation.donation)?;

    let now = Utc::now();
    let attempt_count = operation.donation.proof_attempt_count + 1;
    let proof_result = state
        .donation_proof_publisher
        .publish_donation_proof(DonationProofRequest {
            case_id: operation.donation.medical_case_id.to_string(),
            hospital_id: operation.hospital_id.to_string(),
            amount_kobo: operation.donation.amount_kobo as u64,
            payment_reference: operation.donation.paystack_reference.clone(),
        })
        .await;

    let (message, update) = match proof_result {
        Ok(receipt) => (
            "published".to_owned(),
            proof_retry_success_update(donation_id, attempt_count, receipt, now),
        ),
        Err(DonationProofError::MissingConfig(key)) => {
            return Err(DonationProofError::MissingConfig(key).into());
        }
        Err(error) => {
            let update =
                proof_retry_failure_update(donation_id, attempt_count, error.to_string(), now);
            let message = if update.proof_status == DonationProofStatus::Failed {
                "failed".to_owned()
            } else {
                "scheduled_for_retry".to_owned()
            };
            (message, update)
        }
    };

    let donation = state
        .donation_repository
        .update_donation_proof(update)
        .await?;
    Ok(Json(AdminDonationProofRetryResponse::from_parts(
        message, donation,
    )))
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
        .find_patient_declaration_by_username(&username)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    Ok(Json(AdminPatientDeclarationResponse::from(declaration)))
}

fn admin_donation_list_query(
    params: AdminDonationListParams,
) -> Result<AdminDonationListQuery, ApiError> {
    Ok(AdminDonationListQuery {
        filters: AdminDonationFilters {
            status: parse_optional_donation_status(params.status.as_deref())?,
            method: parse_optional_donation_method(params.method.as_deref())?,
            proof_status: parse_optional_proof_status(params.proof_status.as_deref())?,
            hospital_id: params.hospital_id,
            medical_case_id: params.medical_case_id,
            paystack_reference: params
                .paystack_reference
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            from: params.from,
            to: params.to,
        },
        limit: normalize_limit(params.limit)?,
        offset: normalize_offset(params.offset)?,
    })
}

fn parse_optional_donation_status(value: Option<&str>) -> Result<Option<DonationStatus>, ApiError> {
    let Some(value) = normalized_optional_filter(value) else {
        return Ok(None);
    };

    let status = match value.as_str() {
        "pending" => DonationStatus::Pending,
        "paid" => DonationStatus::Paid,
        "failed" => DonationStatus::Failed,
        "rejected_overflow" => DonationStatus::RejectedOverflow,
        _ => {
            return Err(ApiError::BadRequest(
                "status must be pending, paid, failed, or rejected_overflow".to_owned(),
            ))
        }
    };

    Ok(Some(status))
}

fn parse_optional_donation_method(
    value: Option<&str>,
) -> Result<Option<crate::domain::donation::DonationMethod>, ApiError> {
    let Some(value) = normalized_optional_filter(value) else {
        return Ok(None);
    };

    let method = match value.as_str() {
        "checkout" => crate::domain::donation::DonationMethod::Checkout,
        "dva_transfer" => crate::domain::donation::DonationMethod::DvaTransfer,
        _ => {
            return Err(ApiError::BadRequest(
                "method must be checkout or dva_transfer".to_owned(),
            ))
        }
    };

    Ok(Some(method))
}

fn parse_optional_proof_status(
    value: Option<&str>,
) -> Result<Option<DonationProofStatus>, ApiError> {
    let Some(value) = normalized_optional_filter(value) else {
        return Ok(None);
    };

    let status = match value.as_str() {
        "pending" => DonationProofStatus::Pending,
        "pending_retry" => DonationProofStatus::PendingRetry,
        "published" => DonationProofStatus::Published,
        "failed" => DonationProofStatus::Failed,
        _ => {
            return Err(ApiError::BadRequest(
                "proof_status must be pending, pending_retry, published, or failed".to_owned(),
            ))
        }
    };

    Ok(Some(status))
}

fn normalized_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
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

fn validate_manual_retry_target(donation: &Donation) -> Result<(), ApiError> {
    if donation.status != DonationStatus::Paid {
        return Err(ApiError::Conflict(
            "only paid donations can publish proof".to_owned(),
        ));
    }

    if donation.proof_status == DonationProofStatus::Published {
        return Err(ApiError::Conflict(
            "donation proof is already published".to_owned(),
        ));
    }

    Ok(())
}

fn proof_retry_success_update(
    donation_id: Uuid,
    attempt_count: i32,
    receipt: DonationProofReceipt,
    now: DateTime<Utc>,
) -> DonationProofAttemptUpdate {
    DonationProofAttemptUpdate {
        donation_id,
        proof_status: DonationProofStatus::Published,
        sui_network: Some(receipt.network),
        sui_tx_digest: Some(receipt.tx_digest),
        proof_attempt_count: attempt_count,
        proof_last_attempt_at: now,
        proof_next_retry_at: None,
        proof_last_error: None,
        proof_published_at: Some(now),
    }
}

fn proof_retry_failure_update(
    donation_id: Uuid,
    attempt_count: i32,
    error: String,
    now: DateTime<Utc>,
) -> DonationProofAttemptUpdate {
    let proof_next_retry_at = next_retry_at(attempt_count, now);
    DonationProofAttemptUpdate {
        donation_id,
        proof_status: if proof_next_retry_at.is_some() {
            DonationProofStatus::PendingRetry
        } else {
            DonationProofStatus::Failed
        },
        sui_network: None,
        sui_tx_digest: None,
        proof_attempt_count: attempt_count,
        proof_last_attempt_at: now,
        proof_next_retry_at,
        proof_last_error: Some(error),
        proof_published_at: None,
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
    use crate::domain::donation::DonationMethod;
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
    fn admin_donation_filters_reject_invalid_values() {
        let params = AdminDonationListParams {
            status: Some("complete".to_owned()),
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            from: None,
            to: None,
            limit: None,
            offset: None,
        };

        assert!(matches!(
            admin_donation_list_query(params),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn admin_donation_pagination_defaults_and_clamps_limit() {
        let default_query = admin_donation_list_query(AdminDonationListParams {
            status: None,
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            from: None,
            to: None,
            limit: None,
            offset: None,
        })
        .expect("default pagination should be valid");

        assert_eq!(default_query.limit, 50);
        assert_eq!(default_query.offset, 0);

        let clamped_query = admin_donation_list_query(AdminDonationListParams {
            status: None,
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            from: None,
            to: None,
            limit: Some(500),
            offset: Some(10),
        })
        .expect("large limit should be clamped");

        assert_eq!(clamped_query.limit, 100);
        assert_eq!(clamped_query.offset, 10);
    }

    #[test]
    fn admin_donation_pagination_rejects_invalid_offset() {
        let params = AdminDonationListParams {
            status: None,
            method: None,
            proof_status: None,
            hospital_id: None,
            medical_case_id: None,
            paystack_reference: None,
            from: None,
            to: None,
            limit: Some(25),
            offset: Some(-1),
        };

        assert!(matches!(
            admin_donation_list_query(params),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn admin_suiscan_url_uses_network_and_digest() {
        assert_eq!(
            suiscan_transaction_url("testnet", "abc123"),
            "https://suiscan.xyz/testnet/tx/abc123"
        );
    }

    #[test]
    fn manual_retry_rejects_unpaid_donation() {
        let mut donation = test_donation(DonationStatus::Pending, DonationProofStatus::Pending);

        assert!(matches!(
            validate_manual_retry_target(&donation),
            Err(ApiError::Conflict(_))
        ));

        donation.status = DonationStatus::Paid;
        donation.proof_status = DonationProofStatus::Published;

        assert!(matches!(
            validate_manual_retry_target(&donation),
            Err(ApiError::Conflict(_))
        ));
    }

    #[test]
    fn proof_retry_success_maps_receipt_into_published_state() {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        let donation_id = Uuid::new_v4();

        let update = proof_retry_success_update(
            donation_id,
            2,
            DonationProofReceipt {
                network: "testnet".to_owned(),
                tx_digest: "digest-1".to_owned(),
            },
            now,
        );

        assert_eq!(update.donation_id, donation_id);
        assert_eq!(update.proof_status, DonationProofStatus::Published);
        assert_eq!(update.sui_network.as_deref(), Some("testnet"));
        assert_eq!(update.sui_tx_digest.as_deref(), Some("digest-1"));
        assert_eq!(update.proof_attempt_count, 2);
        assert_eq!(update.proof_published_at, Some(now));
        assert!(update.proof_next_retry_at.is_none());
        assert!(update.proof_last_error.is_none());
    }

    #[test]
    fn proof_retry_failure_schedules_retry_then_terminal_failure() {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        let donation_id = Uuid::new_v4();

        let retry_update =
            proof_retry_failure_update(donation_id, 1, "temporary sui error".to_owned(), now);

        assert_eq!(retry_update.proof_status, DonationProofStatus::PendingRetry);
        assert!(retry_update.proof_next_retry_at.is_some());
        assert_eq!(
            retry_update.proof_last_error.as_deref(),
            Some("temporary sui error")
        );

        let failed_update =
            proof_retry_failure_update(donation_id, 7, "permanent sui error".to_owned(), now);

        assert_eq!(failed_update.proof_status, DonationProofStatus::Failed);
        assert!(failed_update.proof_next_retry_at.is_none());
    }

    fn test_donation(status: DonationStatus, proof_status: DonationProofStatus) -> Donation {
        let now = Utc.with_ymd_and_hms(2026, 7, 4, 12, 0, 0).unwrap();
        Donation {
            id: Uuid::new_v4(),
            medical_case_id: Uuid::new_v4(),
            donor_display_name: "Ada".to_owned(),
            donor_email: "ada@example.com".to_owned(),
            amount_kobo: 10_000,
            method: DonationMethod::Checkout,
            paystack_reference: "ref-1".to_owned(),
            paystack_transaction_reference: None,
            paystack_access_code: None,
            paystack_authorization_url: None,
            paystack_customer_code: None,
            paystack_dedicated_account_id: None,
            paystack_dedicated_account_number: None,
            paystack_dedicated_account_name: None,
            paystack_dedicated_bank_name: None,
            paystack_dedicated_bank_slug: None,
            status,
            paid_at: None,
            proof_status,
            sui_network: None,
            sui_tx_digest: None,
            proof_attempt_count: 0,
            proof_last_attempt_at: None,
            proof_next_retry_at: None,
            proof_last_error: None,
            proof_published_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}
