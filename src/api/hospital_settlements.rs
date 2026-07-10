use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    api::{error::ApiError, AppState},
    domain::settlement::HospitalSettlementStatus,
    port::{
        auth::AuthenticatedHospital,
        settlement::{AdminSettlementListQuery, AdminSettlementOperation},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/history", get(list_settlement_history))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct HospitalSettlementHistoryParams {
    pub status: Option<String>,
    pub medical_case_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalSettlementHistoryResponse {
    pub settlements: Vec<HospitalSettlementHistoryItemResponse>,
    pub pagination: HospitalSettlementHistoryPaginationResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalSettlementHistoryPaginationResponse {
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HospitalSettlementHistoryItemResponse {
    pub id: Uuid,
    pub medical_case_id: Uuid,
    pub case_title: String,
    pub public_slug: Option<String>,
    pub public_link: Option<String>,
    pub patient_id: Uuid,
    pub patient_name: String,
    pub amount_kobo: i64,
    pub status: String,
    pub settlement_reference: String,
    pub bank_name: String,
    pub bank_code: Option<String>,
    pub account_name: String,
    pub account_number: String,
    pub paystack_transfer_code: Option<String>,
    pub failure_reason: Option<String>,
    pub initiated_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[utoipa::path(
    get,
    path = "/api/v1/hospitals/settlements/history",
    tag = "Hospitals",
    security(("bearer_auth" = [])),
    params(HospitalSettlementHistoryParams),
    responses(
        (status = 200, description = "Settlement history for the authenticated hospital.", body = HospitalSettlementHistoryResponse),
        (status = 400, description = "Invalid settlement history filter."),
        (status = 401, description = "Missing or invalid hospital bearer token.")
    )
)]
pub async fn list_settlement_history(
    authenticated: AuthenticatedHospital,
    State(state): State<AppState>,
    Query(params): Query<HospitalSettlementHistoryParams>,
) -> Result<Json<HospitalSettlementHistoryResponse>, ApiError> {
    let query = hospital_settlement_history_query(authenticated.hospital_id, params)?;
    let total = state
        .settlement_repository
        .count_admin_settlements(query.clone())
        .await?;
    let settlements = state
        .settlement_repository
        .list_admin_settlements(query.clone())
        .await?
        .into_iter()
        .map(HospitalSettlementHistoryItemResponse::from)
        .collect();

    Ok(Json(HospitalSettlementHistoryResponse {
        settlements,
        pagination: HospitalSettlementHistoryPaginationResponse {
            limit: query.limit,
            offset: query.offset,
            total,
        },
    }))
}

fn hospital_settlement_history_query(
    hospital_id: Uuid,
    params: HospitalSettlementHistoryParams,
) -> Result<AdminSettlementListQuery, ApiError> {
    Ok(AdminSettlementListQuery {
        status: parse_optional_settlement_status(params.status.as_deref())?,
        hospital_id: Some(hospital_id),
        medical_case_id: params.medical_case_id,
        from: params.from,
        to: params.to,
        admin_action_required_only: false,
        limit: normalize_settlement_history_limit(params.limit)?,
        offset: normalize_settlement_history_offset(params.offset)?,
    })
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

fn normalize_settlement_history_limit(limit: Option<i64>) -> Result<i64, ApiError> {
    match limit {
        Some(value) if value < 1 => Err(ApiError::BadRequest(
            "limit must be greater than zero".to_owned(),
        )),
        Some(value) => Ok(value.min(100)),
        None => Ok(50),
    }
}

fn normalize_settlement_history_offset(offset: Option<i64>) -> Result<i64, ApiError> {
    match offset {
        Some(value) if value < 0 => Err(ApiError::BadRequest(
            "offset must be zero or greater".to_owned(),
        )),
        Some(value) => Ok(value),
        None => Ok(0),
    }
}

fn public_case_link(public_slug: &str) -> String {
    format!("/cases/{public_slug}")
}

impl From<AdminSettlementOperation> for HospitalSettlementHistoryItemResponse {
    fn from(operation: AdminSettlementOperation) -> Self {
        let settlement = operation.settlement;
        let public_link = operation.public_slug.as_deref().map(public_case_link);

        Self {
            id: settlement.id,
            medical_case_id: settlement.medical_case_id,
            case_title: operation.case_title,
            public_slug: operation.public_slug,
            public_link,
            patient_id: operation.patient_id,
            patient_name: operation.patient_name,
            amount_kobo: settlement.amount_kobo,
            status: settlement.status.as_str().to_owned(),
            settlement_reference: settlement.settlement_reference,
            bank_name: settlement.bank_name,
            bank_code: settlement.bank_code,
            account_name: settlement.account_name,
            account_number: settlement.account_number,
            paystack_transfer_code: settlement.paystack_transfer_code,
            failure_reason: settlement.failure_reason,
            initiated_at: settlement.initiated_at,
            paid_at: settlement.paid_at,
            failed_at: settlement.failed_at,
            created_at: settlement.created_at,
            updated_at: settlement.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::settlement::HospitalSettlement;

    fn settlement_operation_with_status(
        status: HospitalSettlementStatus,
    ) -> AdminSettlementOperation {
        let now = Utc::now();
        let hospital_id = Uuid::new_v4();
        let medical_case_id = Uuid::new_v4();
        let patient_id = Uuid::new_v4();

        AdminSettlementOperation {
            settlement: HospitalSettlement {
                id: Uuid::new_v4(),
                hospital_id,
                medical_case_id,
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
                paystack_status: Some("success".to_owned()),
                failure_reason: None,
                initiated_at: Some(now),
                paid_at: Some(now),
                failed_at: None,
                created_at: now,
                updated_at: now,
            },
            hospital_name: "Lagoon Hospital".to_owned(),
            case_title: "Surgery Support".to_owned(),
            public_slug: Some("andrew-surgery-12345678".to_owned()),
            patient_id,
            patient_name: "Andrew Andrew".to_owned(),
        }
    }

    #[test]
    fn hospital_settlement_history_query_scopes_to_authenticated_hospital() {
        let hospital_id = Uuid::new_v4();
        let medical_case_id = Uuid::new_v4();

        let query = hospital_settlement_history_query(
            hospital_id,
            HospitalSettlementHistoryParams {
                status: Some("paid".to_owned()),
                medical_case_id: Some(medical_case_id),
                from: None,
                to: None,
                limit: Some(500),
                offset: Some(10),
            },
        )
        .expect("settlement history query should be valid");

        assert_eq!(query.hospital_id, Some(hospital_id));
        assert_eq!(query.medical_case_id, Some(medical_case_id));
        assert_eq!(query.status, Some(HospitalSettlementStatus::Paid));
        assert_eq!(query.limit, 100);
        assert_eq!(query.offset, 10);
        assert!(!query.admin_action_required_only);
    }

    #[test]
    fn hospital_settlement_history_query_rejects_invalid_filters() {
        let invalid_status = hospital_settlement_history_query(
            Uuid::new_v4(),
            HospitalSettlementHistoryParams {
                status: Some("settled".to_owned()),
                medical_case_id: None,
                from: None,
                to: None,
                limit: None,
                offset: None,
            },
        );

        assert!(matches!(invalid_status, Err(ApiError::BadRequest(_))));

        let invalid_offset = hospital_settlement_history_query(
            Uuid::new_v4(),
            HospitalSettlementHistoryParams {
                status: None,
                medical_case_id: None,
                from: None,
                to: None,
                limit: Some(25),
                offset: Some(-1),
            },
        );

        assert!(matches!(invalid_offset, Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn hospital_settlement_history_response_maps_public_case_and_transfer_fields() {
        let response = HospitalSettlementHistoryItemResponse::from(
            settlement_operation_with_status(HospitalSettlementStatus::Paid),
        );

        assert_eq!(response.case_title, "Surgery Support");
        assert_eq!(
            response.public_slug.as_deref(),
            Some("andrew-surgery-12345678")
        );
        assert_eq!(
            response.public_link.as_deref(),
            Some("/cases/andrew-surgery-12345678")
        );
        assert_eq!(response.patient_name, "Andrew Andrew");
        assert_eq!(response.amount_kobo, 500_000);
        assert_eq!(response.status, "paid");
        assert_eq!(response.bank_code.as_deref(), Some("035"));
        assert_eq!(response.paystack_transfer_code.as_deref(), Some("TRF_test"));
    }
}
