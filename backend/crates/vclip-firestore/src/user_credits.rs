//! User credits repository for atomic credit charging.
//!
//! This module provides a shared repository for credit operations, used by both
//! the API (upfront reservation) and worker (post-success charging).
//!
//! # Key Features
//! - Atomic credit increment with optimistic locking
//! - Month-aware usage tracking with automatic reset
//! - Shared across API and worker crates

use std::collections::HashMap;
use std::time::Duration;

use chrono::{Datelike, Utc};
use tracing::{debug, info, warn};

use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{FromFirestoreValue, ToFirestoreValue};

// =============================================================================
// Constants
// =============================================================================

/// Maximum retries for atomic credit operations (optimistic locking).
const MAX_CREDIT_RETRIES: u32 = 5;

/// Base delay for exponential backoff on retry (milliseconds).
const RETRY_BASE_DELAY_MS: u64 = 50;

// =============================================================================
// Public Utilities
// =============================================================================

/// Get current month key in "YYYY-MM" format.
///
/// This is the standard format for tracking monthly usage periods.
///
/// # Example
/// ```ignore
/// assert_eq!(current_month_key(), "2025-12"); // December 2025
/// ```
pub fn current_month_key() -> String {
    let now = Utc::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

// =============================================================================
// User Credits Repository
// =============================================================================

/// Result of a credit charge operation.
#[derive(Debug, Clone)]
pub struct CreditChargeResult {
    /// New total credits used this month after the charge.
    pub credits_used_after: u32,
    /// Whether the month was reset (first charge of the month).
    pub month_reset: bool,
}

/// Repository for user credit operations.
///
/// Provides atomic credit charging with optimistic locking to prevent race conditions.
/// Used by both the API and worker crates.
pub struct UserCreditsRepository {
    client: FirestoreClient,
    user_id: String,
}

impl UserCreditsRepository {
    /// Create a new user credits repository.
    pub fn new(client: FirestoreClient, user_id: impl Into<String>) -> Self {
        Self {
            client,
            user_id: user_id.into(),
        }
    }

    /// Get the user ID this repository operates on.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Atomically charge credits to the user's account.
    ///
    /// Uses optimistic locking with Firestore's `updateTime` precondition
    /// to prevent race conditions where concurrent operations could result
    /// in incorrect credit totals.
    ///
    /// # Arguments
    /// * `credits` - Number of credits to charge
    ///
    /// # Returns
    /// * `Ok(CreditChargeResult)` - The result including new total credits used
    /// * `Err` - If the user is not found or the operation failed after retries
    ///
    /// # Example
    /// ```ignore
    /// let repo = UserCreditsRepository::new(firestore, "user123");
    /// let result = repo.charge_credits(3).await?;
    /// println!("User now has {} credits used this month", result.credits_used_after);
    /// ```
    pub async fn charge_credits(&self, credits: u32) -> FirestoreResult<CreditChargeResult> {
        let current_month = current_month_key();
        let mut last_error = None;

        for attempt in 0..MAX_CREDIT_RETRIES {
            // Fetch current user document to get credits and update_time
            let doc = self
                .client
                .get_document("users", &self.user_id)
                .await?;

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
                    warn!(user_id = %self.user_id, "User not found when charging credits");
                    return Err(FirestoreError::NotFound(format!(
                        "User {} not found",
                        self.user_id
                    )));
                }
            };

            // Determine if this is a new month
            let is_new_month = usage_reset_month.as_deref() != Some(&current_month);

            // Calculate new credit value
            let new_credits = if is_new_month {
                // New month - reset counter and start fresh
                credits
            } else {
                credits_used.saturating_add(credits)
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
                .client
                .update_document_with_precondition(
                    "users",
                    &self.user_id,
                    fields,
                    Some(update_mask),
                    update_time.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    info!(
                        user_id = %self.user_id,
                        credits = credits,
                        total_used = new_credits,
                        month_reset = is_new_month,
                        "Charged credits"
                    );
                    return Ok(CreditChargeResult {
                        credits_used_after: new_credits,
                        month_reset: is_new_month,
                    });
                }
                Err(e) if e.is_precondition_failed() => {
                    // Another writer updated the document; retry with exponential backoff
                    debug!(
                        user_id = %self.user_id,
                        attempt = attempt + 1,
                        "Credit charge precondition failed, retrying"
                    );
                    last_error = Some(e);
                    let delay = Duration::from_millis(RETRY_BASE_DELAY_MS * (attempt as u64 + 1));
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => {
                    warn!(user_id = %self.user_id, error = %e, "Failed to charge credits");
                    return Err(e);
                }
            }
        }

        // Exhausted retries
        warn!(
            user_id = %self.user_id,
            retries = MAX_CREDIT_RETRIES,
            error = ?last_error,
            "Credit charge failed after retries"
        );
        Err(FirestoreError::request_failed(
            "Failed to charge credits due to concurrent updates",
        ))
    }

    /// Get the current credits used this month.
    ///
    /// Returns 0 if the user doesn't exist or if the month has reset.
    pub async fn get_credits_used(&self) -> FirestoreResult<u32> {
        let current_month = current_month_key();

        let doc = self.client.get_document("users", &self.user_id).await?;

        match doc {
            Some(d) => {
                let fields = d.fields.as_ref();

                // Check if we're in the same month
                let usage_month = fields
                    .and_then(|f| f.get("usage_reset_month"))
                    .and_then(|v| String::from_firestore_value(v));

                if usage_month.as_deref() != Some(&current_month) {
                    // Different month - credits are effectively 0
                    return Ok(0);
                }

                let credits = fields
                    .and_then(|f| f.get("credits_used_this_month"))
                    .and_then(|v| u32::from_firestore_value(v))
                    .unwrap_or(0);

                Ok(credits)
            }
            None => Ok(0),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_month_key_format() {
        let key = current_month_key();
        // Should be in "YYYY-MM" format
        assert_eq!(key.len(), 7);
        assert!(key.contains('-'));
        let parts: Vec<&str> = key.split('-').collect();
        assert_eq!(parts.len(), 2);
        // Year should be a valid 4-digit number
        let year: i32 = parts[0].parse().expect("Year should be numeric");
        assert!(year >= 2020 && year <= 2100);
        // Month should be 01-12
        let month: u32 = parts[1].parse().expect("Month should be numeric");
        assert!((1..=12).contains(&month));
    }

    #[test]
    fn test_credit_charge_result() {
        let result = CreditChargeResult {
            credits_used_after: 100,
            month_reset: false,
        };
        assert_eq!(result.credits_used_after, 100);
        assert!(!result.month_reset);
    }
}
