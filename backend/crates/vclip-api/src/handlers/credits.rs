//! Credit history API handlers.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use vclip_models::{CreditOperationType, CreditTransaction};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Maximum allowed limit for credit history queries.
const MAX_LIMIT: u32 = 100;

/// Query parameters for credit history endpoint.
#[derive(Debug, Deserialize)]
pub struct CreditHistoryQuery {
    /// Maximum number of transactions to return (clamped to 1..100).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Cursor timestamp for pagination (ISO8601 format).
    /// This is the timestamp of the last item from the previous page.
    pub cursor: Option<String>,
    /// Filter by operation type (optional).
    /// Must be one of: analysis, scene_processing, reprocessing, silent_remover,
    /// object_detection, scene_originals, admin_adjustment.
    pub operation_type: Option<String>,
}

fn default_limit() -> u32 {
    50
}

/// Validate operation_type against known values.
fn validate_operation_type(op_type: &str) -> Result<(), ApiError> {
    if CreditOperationType::from_str(op_type).is_none() {
        return Err(ApiError::bad_request(format!(
            "Invalid operation_type '{}'. Must be one of: analysis, scene_processing, reprocessing, silent_remover, object_detection, scene_originals, admin_adjustment",
            op_type
        )));
    }
    Ok(())
}

/// Credit transaction response (serializable version).
#[derive(Serialize)]
pub struct CreditTransactionResponse {
    pub id: String,
    pub timestamp: String,
    pub operation_type: String,
    pub credits_amount: u32,
    pub description: String,
    pub balance_after: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl From<CreditTransaction> for CreditTransactionResponse {
    fn from(tx: CreditTransaction) -> Self {
        Self {
            id: tx.id,
            timestamp: tx.timestamp.to_rfc3339(),
            operation_type: tx.operation_type.as_str().to_string(),
            credits_amount: tx.credits_amount,
            description: tx.description,
            balance_after: tx.balance_after,
            video_id: tx.video_id,
            draft_id: tx.draft_id,
            metadata: tx.metadata,
        }
    }
}

/// Month summary for the current billing period.
#[derive(Serialize)]
pub struct MonthSummaryResponse {
    /// Current month in YYYY-MM format.
    pub month: String,
    /// Total credits used this month.
    pub total_used: u32,
    /// Monthly credit limit.
    pub monthly_limit: u32,
    /// Remaining credits.
    pub remaining: u32,
    /// Breakdown by operation type.
    pub by_operation: HashMap<String, u32>,
}

/// Credit history response.
#[derive(Serialize)]
pub struct CreditHistoryResponse {
    /// List of credit transactions.
    pub transactions: Vec<CreditTransactionResponse>,
    /// Next page token for pagination (if more results exist).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    /// Summary for the current month.
    pub summary: MonthSummaryResponse,
}

/// Get credit history for the authenticated user.
///
/// Returns a paginated list of credit transactions with a summary
/// of the current month's usage.
pub async fn get_credit_history(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<CreditHistoryQuery>,
) -> ApiResult<Json<CreditHistoryResponse>> {
    // Fix #6: Validate operation_type if provided
    if let Some(ref op_type) = query.operation_type {
        // Trim and limit length to prevent abuse
        let op_type_trimmed = op_type.trim();
        if op_type_trimmed.len() > 50 {
            return Err(ApiError::bad_request("operation_type too long"));
        }
        validate_operation_type(op_type_trimmed)?;
    }

    // Fix #6: Clamp limit to safe range (1..=100)
    let effective_limit = query.limit.clamp(1, MAX_LIMIT);

    // Get plan limits for monthly credit info
    let limits = state.user_service.get_plan_limits(&user.uid).await?;

    // Get current month's usage from user record
    let user_record = state.user_service.get_or_create_user(&user.uid, None).await?;
    let credits_used = user_record.credits_used_this_month;

    // Get credit transactions with pagination and server-side filtering
    // Fix #1: Now uses cursor-based pagination with server-side ordering and filtering
    let (transactions, next_cursor) = state
        .user_service
        .get_credit_history(
            &user.uid,
            Some(effective_limit),
            query.cursor.as_deref(),
            query.operation_type.as_deref(),
        )
        .await?;

    // Convert to response format
    let transactions: Vec<CreditTransactionResponse> =
        transactions.into_iter().map(Into::into).collect();

    // Get month summary
    let by_operation = state
        .user_service
        .get_credit_month_summary(&user.uid)
        .await?;

    // Get current month key
    let current_month = chrono::Utc::now().format("%Y-%m").to_string();

    let summary = MonthSummaryResponse {
        month: current_month,
        total_used: credits_used,
        monthly_limit: limits.monthly_credits_included,
        remaining: limits.monthly_credits_included.saturating_sub(credits_used),
        by_operation,
    };

    Ok(Json(CreditHistoryResponse {
        transactions,
        next_page_token: next_cursor,
        summary,
    }))
}
