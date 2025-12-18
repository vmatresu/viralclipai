//! Credit service for managing credit transactions and usage tracking.
//!
//! This module handles:
//! - Credit reservation with optimistic locking
//! - Credit transaction recording (async, non-blocking)
//! - Credit history retrieval with pagination
//! - Monthly usage summaries
//!
//! # Architecture
//!
//! The credit system uses optimistic locking via Firestore's `updateTime` precondition
//! to handle concurrent credit reservations safely. Transactions are recorded
//! asynchronously to avoid blocking the main request path.
//!
//! # Security
//!
//! - All credit operations require authenticated user context
//! - Credits are charged upfront and NOT refunded on job failure
//! - Monthly limits are enforced server-side with atomic updates

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Utc};
use tracing::{debug, info, warn};

use vclip_firestore::{CreditTransactionRepository, FirestoreClient, FromFirestoreValue, ToFirestoreValue};
use vclip_models::{CreditContext, CreditTransaction};

use crate::error::{ApiError, ApiResult};

// =============================================================================
// Constants
// =============================================================================

/// Maximum retries for atomic credit reservation (optimistic locking).
const MAX_CREDIT_RESERVE_RETRIES: u32 = 5;

/// Base delay for exponential backoff on retry (milliseconds).
const RETRY_BASE_DELAY_MS: u64 = 50;

/// Timeout for background transaction recording.
const TRANSACTION_RECORD_TIMEOUT: Duration = Duration::from_secs(5);

// =============================================================================
// Credit Service
// =============================================================================

/// Service for managing credit operations.
///
/// Provides methods for:
/// - Checking and reserving credits atomically
/// - Recording credit transactions
/// - Retrieving credit history with pagination
/// - Monthly usage summaries
#[derive(Clone)]
pub struct CreditService {
    firestore: Arc<FirestoreClient>,
}

impl CreditService {
    /// Create a new credit service.
    pub fn new(firestore: Arc<FirestoreClient>) -> Self {
        Self { firestore }
    }

    // =========================================================================
    // Credit Reservation
    // =========================================================================

    /// Check if user has sufficient credits and reserve them atomically.
    ///
    /// Uses optimistic locking with Firestore's `updateTime` precondition
    /// to prevent race conditions where concurrent requests could overspend.
    ///
    /// # Arguments
    /// * `uid` - User ID
    /// * `credits_needed` - Number of credits to reserve
    /// * `monthly_limit` - User's monthly credit limit from their plan
    ///
    /// # Returns
    /// * `Ok(new_credits_used)` - The new total credits used after reservation
    /// * `Err` - If insufficient credits or reservation failed
    ///
    /// # Important
    /// Credits are charged upfront and NOT refunded if the job fails.
    pub async fn check_and_reserve_credits(
        &self,
        uid: &str,
        credits_needed: u32,
        monthly_limit: u32,
    ) -> ApiResult<u32> {
        let current_month = current_month_key();
        let mut last_error: Option<vclip_firestore::FirestoreError> = None;

        for attempt in 0..MAX_CREDIT_RESERVE_RETRIES {
            // Fetch document directly to get server-side update_time for precondition
            let doc = self
                .firestore
                .get_document("users", uid)
                .await
                .map_err(|e| ApiError::internal(format!("Firestore error: {}", e)))?;

            // Extract credits and update_time from document
            let (credits_used, usage_reset_month, update_time) = match &doc {
                Some(d) => {
                    let fields = d.fields.as_ref();
                    let credits = fields
                        .and_then(|f| f.get("credits_used_this_month"))
                        .and_then(|v| u32::from_firestore_value(v))
                        .unwrap_or(0);
                    let reset_month = fields
                        .and_then(|f| f.get("usage_reset_month"))
                        .and_then(|v| String::from_firestore_value(v));
                    (credits, reset_month, d.update_time.clone())
                }
                None => {
                    // User doesn't exist - this shouldn't happen in normal flow
                    return Err(ApiError::not_found("User not found"));
                }
            };

            // Check if we need to reset for new month
            let effective_credits_used = if usage_reset_month.as_deref() == Some(&current_month) {
                credits_used
            } else {
                0 // Will reset on this write
            };

            // Check if we have enough credits
            let remaining = monthly_limit.saturating_sub(effective_credits_used);
            if credits_needed > remaining {
                return Err(ApiError::forbidden(format!(
                    "Insufficient credits. You need {} credits but only have {} remaining ({} used of {} monthly limit). Please upgrade your plan.",
                    credits_needed, remaining, effective_credits_used, monthly_limit
                )));
            }

            // Calculate new credit value
            let new_credits = if usage_reset_month.as_deref() == Some(&current_month) {
                credits_used.saturating_add(credits_needed)
            } else {
                // New month - reset counter
                credits_needed
            };

            // Build update fields
            let mut fields = HashMap::new();
            fields.insert(
                "credits_used_this_month".to_string(),
                new_credits.to_firestore_value(),
            );
            fields.insert(
                "usage_reset_month".to_string(),
                current_month.to_firestore_value(),
            );
            fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

            let update_mask = vec![
                "credits_used_this_month".to_string(),
                "usage_reset_month".to_string(),
                "updated_at".to_string(),
            ];

            // Attempt atomic update with precondition
            match self
                .firestore
                .update_document_with_precondition(
                    "users",
                    uid,
                    fields,
                    Some(update_mask),
                    update_time.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    info!(
                        user_id = %uid,
                        credits = credits_needed,
                        total_used = new_credits,
                        "Reserved credits"
                    );
                    return Ok(new_credits);
                }
                Err(e) if e.is_precondition_failed() => {
                    // Another writer updated the document; retry with exponential backoff
                    debug!(
                        user_id = %uid,
                        attempt = attempt + 1,
                        "Credit reservation precondition failed, retrying"
                    );
                    last_error = Some(e);
                    let delay = Duration::from_millis(RETRY_BASE_DELAY_MS * (attempt as u64 + 1));
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => {
                    warn!(user_id = %uid, error = %e, "Failed to reserve credits");
                    return Err(ApiError::internal("Failed to reserve credits"));
                }
            }
        }

        // Exhausted retries
        warn!(
            user_id = %uid,
            retries = MAX_CREDIT_RESERVE_RETRIES,
            error = ?last_error,
            "Credit reservation failed after retries"
        );
        Err(ApiError::internal(
            "Failed to reserve credits due to concurrent updates. Please try again.",
        ))
    }

    // =========================================================================
    // Transaction Recording
    // =========================================================================

    /// Record a credit transaction asynchronously (fire-and-forget).
    ///
    /// Spawns a background task to record the transaction without blocking
    /// the main operation. Failures are logged but do not affect the caller.
    ///
    /// # Arguments
    /// * `uid` - User ID
    /// * `credits` - Number of credits charged
    /// * `credits_used_after` - Total credits used this month after the transaction
    /// * `context` - Transaction context (operation type, description, metadata)
    pub fn record_transaction(
        &self,
        uid: &str,
        credits: u32,
        credits_used_after: u32,
        context: CreditContext,
    ) {
        let firestore = Arc::clone(&self.firestore);
        let uid = uid.to_string();

        tokio::spawn(async move {
            let repo = CreditTransactionRepository::new((*firestore).clone(), &uid);

            // Build transaction using builder pattern
            let tx = CreditTransaction::new(
                uuid::Uuid::new_v4().to_string(),
                uid.clone(),
                context.operation_type,
                credits,
                context.description,
                credits_used_after,
            )
            .with_optional_video_id(context.video_id)
            .with_optional_draft_id(context.draft_id)
            .with_optional_metadata(context.metadata);

            // Wrap Firestore write in a timeout to avoid hung tasks
            match tokio::time::timeout(TRANSACTION_RECORD_TIMEOUT, repo.create(&tx)).await {
                Ok(Ok(())) => {
                    debug!(
                        user_id = %uid,
                        transaction_id = %tx.id,
                        credits = credits,
                        "Recorded credit transaction"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        user_id = %uid,
                        error = %e,
                        "Failed to record credit transaction"
                    );
                }
                Err(_) => {
                    warn!(
                        user_id = %uid,
                        timeout_secs = TRANSACTION_RECORD_TIMEOUT.as_secs(),
                        "Credit transaction recording timed out"
                    );
                }
            }
        });
    }

    /// Reserve credits and record the transaction in one operation.
    ///
    /// This is the primary method for credit-consuming operations.
    /// It atomically reserves credits, then records the transaction asynchronously.
    ///
    /// # Arguments
    /// * `uid` - User ID
    /// * `credits_needed` - Number of credits to charge
    /// * `monthly_limit` - User's monthly credit limit from their plan
    /// * `context` - Transaction context (operation type, description, metadata)
    pub async fn reserve_and_record(
        &self,
        uid: &str,
        credits_needed: u32,
        monthly_limit: u32,
        context: CreditContext,
    ) -> ApiResult<()> {
        // Reserve credits atomically
        let credits_used_after = self
            .check_and_reserve_credits(uid, credits_needed, monthly_limit)
            .await?;

        // Record the transaction (non-blocking)
        self.record_transaction(uid, credits_needed, credits_used_after, context);

        Ok(())
    }

    // =========================================================================
    // Credit History
    // =========================================================================

    /// Get credit transactions for a user with pagination.
    ///
    /// Uses cursor-based pagination with server-side ordering (newest first)
    /// and optional operation type filtering.
    ///
    /// # Arguments
    /// * `uid` - User ID
    /// * `limit` - Maximum number of transactions to return (clamped to 1..100)
    /// * `cursor_timestamp` - ISO8601 timestamp to start after (for pagination)
    /// * `operation_type` - Optional filter by operation type
    ///
    /// # Returns
    /// Tuple of (transactions, next_cursor)
    pub async fn get_history(
        &self,
        uid: &str,
        limit: Option<u32>,
        cursor_timestamp: Option<&str>,
        operation_type: Option<&str>,
    ) -> ApiResult<(Vec<CreditTransaction>, Option<String>)> {
        let repo = CreditTransactionRepository::new((*self.firestore).clone(), uid);
        repo.list_page(limit, cursor_timestamp, operation_type)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get credit history: {}", e)))
    }

    /// Get credit usage summary for a specific month.
    ///
    /// Returns a breakdown of credits used by operation type.
    ///
    /// # Arguments
    /// * `uid` - User ID
    /// * `month_key` - Month in "YYYY-MM" format (defaults to current month)
    pub async fn get_month_summary(
        &self,
        uid: &str,
        month_key: Option<&str>,
    ) -> ApiResult<HashMap<String, u32>> {
        let repo = CreditTransactionRepository::new((*self.firestore).clone(), uid);
        let key = month_key
            .map(|s| s.to_string())
            .unwrap_or_else(current_month_key);

        repo.get_month_summary(&key)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get credit summary: {}", e)))
    }

    /// Get total credits used in a specific month.
    ///
    /// # Arguments
    /// * `uid` - User ID
    /// * `month_key` - Month in "YYYY-MM" format (defaults to current month)
    pub async fn get_month_total(
        &self,
        uid: &str,
        month_key: Option<&str>,
    ) -> ApiResult<u32> {
        let repo = CreditTransactionRepository::new((*self.firestore).clone(), uid);
        let key = month_key
            .map(|s| s.to_string())
            .unwrap_or_else(current_month_key);

        repo.get_month_total(&key)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get credit total: {}", e)))
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get current month key in "YYYY-MM" format.
pub fn current_month_key() -> String {
    let now = Utc::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_month_key_format() {
        let key = current_month_key();
        assert!(key.len() == 7);
        assert!(key.contains('-'));
        let parts: Vec<&str> = key.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].parse::<i32>().is_ok());
        assert!(parts[1].parse::<u32>().is_ok());
    }
}
