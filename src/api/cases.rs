use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{error::ApiError, AppState},
    domain::{
        donation::{CaseDva, Donation},
        patient_declaration::PatientDeclaration,
        public_case::PublicCaseDetails,
    },
    port::{
        donation::{NewDonation, UpsertCaseDva},
        payment::{CheckoutInitializationRequest, DvaAssignmentRequest, PaymentMethod},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/:public_slug", get(get_case_by_public_slug))
        .route(
            "/:public_slug/donations/initialize",
            post(initialize_case_donation),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicMedicalCaseResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub hospital_name: String,
    pub hospital_address: Option<String>,
    pub patient_id: Uuid,
    pub patient_declaration: PublicPatientDeclarationResponse,
    pub donors: Vec<PublicDonationResponse>,
    pub donation_options: DonationOptionsResponse,
    pub title: String,
    pub public_slug: String,
    pub public_link: String,
    pub diagnosis_summary: String,
    pub bill_amount_kobo: i64,
    pub amount_raised_kobo: i64,
    pub remaining_amount_kobo: i64,
    pub status: String,
    pub admitted_at: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DonationOptionsResponse {
    pub checkout_enabled: bool,
    pub dva_enabled: bool,
    pub donations_closed: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicPatientDeclarationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub statement: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicDonationResponse {
    pub id: Uuid,
    pub display_name: String,
    pub amount_kobo: i64,
    pub method: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub sui_transaction_url: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InitializeDonationRequest {
    pub payment_method: String,
    pub amount_kobo: Option<i64>,
    pub donor_email: Option<String>,
    pub donor_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InitializeDonationResponse {
    pub payment_method: String,
    pub checkout_enabled: bool,
    pub dva_enabled: bool,
    pub donations_closed: bool,
    pub checkout: Option<CheckoutInitializationResponse>,
    pub dva_transfer: Option<DvaInitializationResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckoutInitializationResponse {
    pub donation_id: Uuid,
    pub paystack_reference: String,
    pub authorization_url: String,
    pub access_code: String,
    pub amount_kobo: i64,
    pub donor_display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DvaInitializationResponse {
    pub bank_name: String,
    pub account_name: String,
    pub account_number: String,
    pub donor_visibility_note: String,
}

#[utoipa::path(
    get,
    path = "/cases/{public_slug}",
    tag = "Cases",
    params(
        ("public_slug" = String, Path, description = "Public medical case slug")
    ),
    responses(
        (status = 200, description = "Public medical case details.", body = PublicMedicalCaseResponse),
        (status = 404, description = "Medical case was not found.")
    )
)]
pub async fn get_case_by_public_slug(
    State(state): State<AppState>,
    Path(public_slug): Path<String>,
) -> Result<Json<PublicMedicalCaseResponse>, ApiError> {
    let public_slug = public_slug.trim();
    if public_slug.is_empty() {
        return Err(ApiError::NotFound("medical case not found".to_owned()));
    }

    let public_case = state
        .donation_repository
        .get_public_case_details(public_slug)
        .await?
        .ok_or_else(|| ApiError::NotFound("medical case not found".to_owned()))?;

    let declaration = state
        .patient_declaration_repository
        .find_case_declaration(public_case.medical_case.id)
        .await?
        .ok_or_else(|| ApiError::NotFound("patient declaration not found".to_owned()))?;

    let hospital = state
        .hospital_repository
        .find_hospital_by_id(public_case.medical_case.hospital_id)
        .await?
        .ok_or(crate::port::hospital::HospitalRepositoryError::NotFound)?;

    let case_dva = state
        .donation_repository
        .find_case_dva(public_case.medical_case.id)
        .await?;

    Ok(Json(PublicMedicalCaseResponse::from_parts(
        public_case,
        declaration,
        hospital.name,
        hospital.official_address,
        case_dva,
        &state.sui_network,
    )))
}

#[utoipa::path(
    post,
    path = "/cases/{public_slug}/donations/initialize",
    tag = "Cases",
    params(
        ("public_slug" = String, Path, description = "Public medical case slug")
    ),
    request_body = InitializeDonationRequest,
    responses(
        (status = 200, description = "Donation payment initialized.", body = InitializeDonationResponse),
        (status = 400, description = "Invalid donation request."),
        (status = 404, description = "Medical case was not found."),
        (status = 409, description = "Donation amount exceeds remaining amount or donations are closed.")
    )
)]
pub async fn initialize_case_donation(
    State(state): State<AppState>,
    Path(public_slug): Path<String>,
    Json(request): Json<InitializeDonationRequest>,
) -> Result<Json<InitializeDonationResponse>, ApiError> {
    let public_slug = public_slug.trim();
    if public_slug.is_empty() {
        return Err(ApiError::NotFound("medical case not found".to_owned()));
    }

    let public_case = state
        .donation_repository
        .get_public_case_details(public_slug)
        .await?
        .ok_or_else(|| ApiError::NotFound("medical case not found".to_owned()))?;

    let hospital = state
        .hospital_repository
        .find_hospital_by_id(public_case.medical_case.hospital_id)
        .await?
        .ok_or(crate::port::hospital::HospitalRepositoryError::NotFound)?;
    let patient = state
        .patient_repository
        .find_patient_by_id(public_case.medical_case.patient_id)
        .await?
        .ok_or(crate::port::patient::PatientRepositoryError::NotFound)?;

    let remaining_amount_kobo = remaining_amount_kobo(&public_case);
    let donations_closed = remaining_amount_kobo == 0;
    if donations_closed {
        return Err(ApiError::Conflict(
            "donations are closed because the hospital bill has been met".to_owned(),
        ));
    }

    let payment_method =
        PaymentMethod::from_str(request.payment_method.trim()).ok_or_else(|| {
            ApiError::BadRequest("payment_method must be 'checkout' or 'dva_transfer'".to_owned())
        })?;

    let existing_case_dva = state
        .donation_repository
        .find_case_dva(public_case.medical_case.id)
        .await?;

    match payment_method {
        PaymentMethod::Checkout => {
            let amount_kobo = request.amount_kobo.ok_or_else(|| {
                ApiError::BadRequest("amount_kobo is required for checkout donations".to_owned())
            })?;
            let donor_email = request.donor_email.as_deref().ok_or_else(|| {
                ApiError::BadRequest("donor_email is required for checkout donations".to_owned())
            })?;
            validate_checkout_request(amount_kobo, donor_email, remaining_amount_kobo)?;

            let donor_display_name = normalize_donor_name(request.donor_name.as_deref());
            let reference = state.payment_gateway.generate_reference();
            let callback_url = format!("{}/cases/{}", state.app_base_url, public_slug);
            let checkout = state
                .payment_gateway
                .initialize_checkout(CheckoutInitializationRequest {
                    donor_email: donor_email.trim().to_lowercase(),
                    donor_display_name: donor_display_name.clone(),
                    amount_kobo,
                    reference: reference.clone(),
                    callback_url,
                    case_public_slug: public_slug.to_owned(),
                    case_title: public_case.medical_case.title.clone(),
                })
                .await?;

            let donation = state
                .donation_repository
                .create_pending_donation(NewDonation {
                    medical_case_id: public_case.medical_case.id,
                    donor_display_name: donor_display_name.clone(),
                    donor_email: donor_email.trim().to_lowercase(),
                    amount_kobo,
                    method: crate::domain::donation::DonationMethod::Checkout,
                    paystack_reference: checkout.provider_reference.clone(),
                    paystack_transaction_reference: None,
                    paystack_access_code: Some(checkout.access_code.clone()),
                    paystack_authorization_url: Some(checkout.authorization_url.clone()),
                    paystack_customer_code: None,
                    paystack_dedicated_account_id: None,
                    paystack_dedicated_account_number: None,
                    paystack_dedicated_account_name: None,
                    paystack_dedicated_bank_name: None,
                    paystack_dedicated_bank_slug: None,
                })
                .await?;

            Ok(Json(InitializeDonationResponse {
                payment_method: payment_method.as_str().to_owned(),
                checkout_enabled: true,
                dva_enabled: existing_case_dva.as_ref().is_none_or(|dva| dva.is_active),
                donations_closed: false,
                checkout: Some(CheckoutInitializationResponse {
                    donation_id: donation.id,
                    paystack_reference: donation.paystack_reference,
                    authorization_url: checkout.authorization_url,
                    access_code: checkout.access_code,
                    amount_kobo: donation.amount_kobo,
                    donor_display_name,
                }),
                dva_transfer: None,
            }))
        }
        PaymentMethod::DvaTransfer => {
            let case_dva = if let Some(case_dva) = existing_case_dva {
                if case_dva.is_active {
                    case_dva
                } else {
                    return Err(ApiError::Conflict(
                        "dva transfers are closed because the hospital bill has been met"
                            .to_owned(),
                    ));
                }
            } else {
                let reference = state.payment_gateway.generate_reference();
                let assignment = state
                    .payment_gateway
                    .ensure_case_dva(DvaAssignmentRequest {
                        customer_email: format!(
                            "anonymous+{}@korede.local",
                            public_case.medical_case.id
                        ),
                        payment_label: bank_transfer_label(&hospital.name, &patient.full_name),
                        case_public_slug: public_slug.to_owned(),
                        case_title: public_case.medical_case.title.clone(),
                        reference,
                    })
                    .await?;

                state
                    .donation_repository
                    .upsert_case_dva(UpsertCaseDva {
                        medical_case_id: public_case.medical_case.id,
                        paystack_reference: assignment.provider_reference,
                        paystack_customer_code: assignment.customer_code,
                        paystack_dedicated_account_id: assignment.dedicated_account_id,
                        account_number: assignment.account_number,
                        account_name: assignment.account_name,
                        bank_name: assignment.bank_name,
                        bank_slug: assignment.bank_slug,
                    })
                    .await?
            };

            Ok(Json(InitializeDonationResponse {
                payment_method: payment_method.as_str().to_owned(),
                checkout_enabled: true,
                dva_enabled: case_dva.is_active,
                donations_closed: false,
                checkout: None,
                dva_transfer: Some(DvaInitializationResponse {
                    bank_name: case_dva.bank_name,
                    account_name: case_dva.account_name,
                    account_number: case_dva.account_number,
                    donor_visibility_note:
                        "DVA transfers are shown publicly as Anonymous by default".to_owned(),
                }),
            }))
        }
    }
}

fn public_case_link(public_slug: &str) -> String {
    format!("/cases/{public_slug}")
}

impl PublicMedicalCaseResponse {
    fn from_parts(
        public_case: PublicCaseDetails,
        declaration: PatientDeclaration,
        hospital_name: String,
        hospital_address: Option<String>,
        case_dva: Option<CaseDva>,
        sui_network: &str,
    ) -> Self {
        let medical_case = public_case.medical_case;
        let public_slug = medical_case.public_slug.clone().unwrap_or_default();
        let remaining_amount_kobo =
            (medical_case.bill_amount_kobo - medical_case.amount_raised_kobo).max(0);
        let donations_closed = remaining_amount_kobo == 0;

        Self {
            id: medical_case.id,
            hospital_id: medical_case.hospital_id,
            hospital_name,
            hospital_address,
            patient_id: medical_case.patient_id,
            patient_declaration: PublicPatientDeclarationResponse::from(declaration),
            donors: public_case
                .donations
                .into_iter()
                .map(|donation| PublicDonationResponse::from_parts(donation, sui_network))
                .collect(),
            donation_options: DonationOptionsResponse {
                checkout_enabled: !donations_closed,
                dva_enabled: !donations_closed && case_dva.as_ref().is_none_or(|dva| dva.is_active),
                donations_closed,
            },
            title: medical_case.title,
            public_slug: public_slug.clone(),
            public_link: public_case_link(&public_slug),
            diagnosis_summary: medical_case.diagnosis_summary,
            bill_amount_kobo: medical_case.bill_amount_kobo,
            amount_raised_kobo: medical_case.amount_raised_kobo,
            remaining_amount_kobo,
            status: medical_case.status.as_str().to_owned(),
            admitted_at: medical_case.admitted_at,
            created_at: medical_case.created_at,
            updated_at: medical_case.updated_at,
        }
    }
}

impl PublicDonationResponse {
    fn from_parts(donation: Donation, sui_network: &str) -> Self {
        let sui_transaction_url = donation
            .sui_tx_digest
            .as_deref()
            .map(|digest| suiscan_transaction_url(sui_network, digest));

        Self {
            id: donation.id,
            display_name: donation.donor_display_name,
            amount_kobo: donation.amount_kobo,
            method: donation.method.as_str().to_owned(),
            paid_at: donation.paid_at,
            sui_transaction_url,
        }
    }
}

impl From<PatientDeclaration> for PublicPatientDeclarationResponse {
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

fn validate_checkout_request(
    amount_kobo: i64,
    donor_email: &str,
    remaining_amount_kobo: i64,
) -> Result<(), ApiError> {
    if amount_kobo <= 0 {
        return Err(ApiError::BadRequest(
            "donation amount must be greater than zero".to_owned(),
        ));
    }

    if donor_email.trim().is_empty() || !donor_email.contains('@') || !donor_email.contains('.') {
        return Err(ApiError::BadRequest("donor email is invalid".to_owned()));
    }

    if amount_kobo > remaining_amount_kobo {
        return Err(ApiError::Conflict(
            "payment amount exceeds the remaining case amount".to_owned(),
        ));
    }

    Ok(())
}

fn normalize_donor_name(name: Option<&str>) -> String {
    let normalized = name.unwrap_or("").trim();
    if normalized.is_empty() {
        "Anonymous".to_owned()
    } else {
        normalized.to_owned()
    }
}

fn bank_transfer_label(hospital_name: &str, patient_name: &str) -> String {
    format!("{} - {}", hospital_name.trim(), patient_name.trim())
}

fn remaining_amount_kobo(public_case: &PublicCaseDetails) -> i64 {
    (public_case.medical_case.bill_amount_kobo - public_case.medical_case.amount_raised_kobo).max(0)
}

fn suiscan_transaction_url(network: &str, tx_digest: &str) -> String {
    format!("https://suiscan.xyz/{network}/tx/{tx_digest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::medical_case::{MedicalCase, MedicalCaseStatus};

    fn patient_declaration(now: DateTime<Utc>) -> PatientDeclaration {
        PatientDeclaration {
            id: Uuid::new_v4(),
            patient_id: Uuid::new_v4(),
            statement: "I consent to having this case shared for treatment funding.".to_owned(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn public_medical_case_response_includes_remaining_amount() {
        let now = Utc::now();
        let declaration = patient_declaration(now);
        let medical_case = MedicalCase {
            id: Uuid::new_v4(),
            hospital_id: Uuid::new_v4(),
            patient_id: declaration.patient_id,
            title: "Right femur fracture surgery".to_owned(),
            public_slug: Some("oluwaseun34-case-12345678".to_owned()),
            diagnosis_summary: "Patient sustained a severe comminuted fracture.".to_owned(),
            bill_amount_kobo: 150_000_000,
            amount_raised_kobo: 45_000_000,
            status: MedicalCaseStatus::Active,
            admitted_at: NaiveDate::from_ymd_opt(2026, 6, 1),
            blockchain_network: None,
            blockchain_tx_digest: None,
            blockchain_record_id: None,
            created_at: now,
            updated_at: now,
        };

        let response = PublicMedicalCaseResponse::from_parts(
            PublicCaseDetails {
                medical_case,
                donations: vec![],
            },
            declaration,
            "Lagoon Hospital".to_owned(),
            Some("1 Hospital Road, Lagos".to_owned()),
            None,
            "testnet",
        );

        assert_eq!(response.public_slug, "oluwaseun34-case-12345678");
        assert_eq!(response.public_link, "/cases/oluwaseun34-case-12345678");
        assert_eq!(response.hospital_name, "Lagoon Hospital");
        assert_eq!(
            response.hospital_address.as_deref(),
            Some("1 Hospital Road, Lagos")
        );
        assert_eq!(response.amount_raised_kobo, 45_000_000);
        assert_eq!(response.remaining_amount_kobo, 105_000_000);
        assert!(response.donation_options.checkout_enabled);
        assert!(response.donation_options.dva_enabled);
        assert_eq!(
            response.patient_declaration.statement,
            "I consent to having this case shared for treatment funding."
        );
    }

    #[test]
    fn public_medical_case_response_clamps_remaining_amount_at_zero() {
        let now = Utc::now();
        let declaration = patient_declaration(now);
        let medical_case = MedicalCase {
            id: Uuid::new_v4(),
            hospital_id: Uuid::new_v4(),
            patient_id: declaration.patient_id,
            title: "Emergency surgery".to_owned(),
            public_slug: Some("case-12345678".to_owned()),
            diagnosis_summary: "Urgent care needed.".to_owned(),
            bill_amount_kobo: 1_000,
            amount_raised_kobo: 1_500,
            status: MedicalCaseStatus::Funded,
            admitted_at: None,
            blockchain_network: None,
            blockchain_tx_digest: None,
            blockchain_record_id: None,
            created_at: now,
            updated_at: now,
        };

        let response = PublicMedicalCaseResponse::from_parts(
            PublicCaseDetails {
                medical_case,
                donations: vec![],
            },
            declaration,
            "Lagoon Hospital".to_owned(),
            Some("1 Hospital Road, Lagos".to_owned()),
            None,
            "testnet",
        );

        assert_eq!(response.remaining_amount_kobo, 0);
        assert!(response.donation_options.donations_closed);
    }

    #[test]
    fn normalize_donor_name_defaults_blank_to_anonymous() {
        assert_eq!(normalize_donor_name(None), "Anonymous");
        assert_eq!(normalize_donor_name(Some("   ")), "Anonymous");
        assert_eq!(normalize_donor_name(Some("Ada")), "Ada");
    }

    #[test]
    fn validate_checkout_request_rejects_amount_above_remaining() {
        let error = validate_checkout_request(15_000, "donor@example.com", 10_000)
            .expect_err("amount above remaining should fail");

        match error {
            ApiError::Conflict(message) => {
                assert_eq!(message, "payment amount exceeds the remaining case amount")
            }
            other => panic!("expected conflict error, got {other:?}"),
        }
    }

    #[test]
    fn public_medical_case_response_disables_dva_when_case_dva_is_inactive() {
        let now = Utc::now();
        let declaration = patient_declaration(now);
        let medical_case = MedicalCase {
            id: Uuid::new_v4(),
            hospital_id: Uuid::new_v4(),
            patient_id: declaration.patient_id,
            title: "Orthopedic surgery".to_owned(),
            public_slug: Some("orthopedic-surgery-case".to_owned()),
            diagnosis_summary: "Major surgery needed".to_owned(),
            bill_amount_kobo: 100_000,
            amount_raised_kobo: 10_000,
            status: MedicalCaseStatus::Active,
            admitted_at: None,
            blockchain_network: None,
            blockchain_tx_digest: None,
            blockchain_record_id: None,
            created_at: now,
            updated_at: now,
        };

        let response = PublicMedicalCaseResponse::from_parts(
            PublicCaseDetails {
                medical_case,
                donations: vec![],
            },
            declaration,
            "Lagoon Hospital".to_owned(),
            Some("1 Hospital Road, Lagos".to_owned()),
            Some(CaseDva {
                medical_case_id: Uuid::new_v4(),
                paystack_reference: "ref-1".to_owned(),
                paystack_customer_code: None,
                paystack_dedicated_account_id: 1,
                account_number: "1234567890".to_owned(),
                account_name: "Hospital - Patient".to_owned(),
                bank_name: "Bank".to_owned(),
                bank_slug: None,
                is_active: false,
                deactivated_at: Some(now),
                deactivation_error: None,
                created_at: now,
                updated_at: now,
            }),
            "testnet",
        );

        assert!(response.donation_options.checkout_enabled);
        assert!(!response.donation_options.dva_enabled);
        assert!(!response.donation_options.donations_closed);
    }

    #[test]
    fn bank_transfer_label_uses_hospital_and_patient_names() {
        assert_eq!(
            bank_transfer_label("Lagoon Hospital", "John Doe"),
            "Lagoon Hospital - John Doe"
        );
    }
}
