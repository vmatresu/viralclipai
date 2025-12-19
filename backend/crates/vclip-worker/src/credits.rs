//! Credit management utilities for the worker.
//!
//! This module provides functions to charge credits after successful job completion.
//! Credits are charged only on success, not upfront.
//!
//! Uses the shared `UserCreditsRepository` from `vclip-firestore` to avoid
//! duplicating credit logic between API and worker.

use std::collections::HashMap;
use std::time::Duration;

use tracing::{debug, warn};

use vclip_firestore::{
    CreditTransactionRepository, FirestoreClient, UserCreditsRepository,
};
use vclip_models::{CreditOperationType, CreditTransaction, ANALYSIS_CREDIT_COST};

use crate::error::{WorkerError, WorkerResult};

// =============================================================================
// Credit Charging
// =============================================================================

/// Charge credits for a successful analysis.
///
/// This function is called by the worker after an analysis job completes successfully.
/// It atomically increments the user's credit usage and records a transaction.
///
/// # Arguments
/// * `firestore` - Firestore client
/// * `user_id` - User ID to charge
/// * `reference_id` - Video ID (from ProcessVideoJob) or Draft ID (from AnalyzeVideoJob)
/// * `video_title` - Title of the analyzed video
/// * `source_url` - Original video URL (e.g., YouTube URL)
/// * `is_draft` - If true, stores reference_id as draft_id; if false, stores as video_id
///
/// # Returns
/// * `Ok(())` on success
/// * `Err` if charging fails (analysis should still be considered successful)
pub async fn charge_analysis_credits(
    firestore: &FirestoreClient,
    user_id: &str,
    reference_id: &str,
    video_title: &str,
    source_url: &str,
    is_draft: bool,
) -> WorkerResult<()> {
    let credits_to_charge = ANALYSIS_CREDIT_COST;

    // Use shared repository for atomic credit charging
    let credits_repo = UserCreditsRepository::new(firestore.clone(), user_id);

    let result = credits_repo
        .charge_credits(credits_to_charge)
        .await
        .map_err(WorkerError::Firestore)?;

    // Record the transaction asynchronously (fire-and-forget)
    record_analysis_transaction(
        firestore.clone(),
        user_id.to_string(),
        reference_id.to_string(),
        video_title.to_string(),
        source_url.to_string(),
        credits_to_charge,
        result.credits_used_after,
        is_draft,
    );

    Ok(())
}

// =============================================================================
// Transaction Recording
// =============================================================================

/// Record a credit transaction asynchronously (fire-and-forget).
///
/// This spawns a background task to record the transaction to ensure
/// it doesn't block the main job completion path.
///
/// # Arguments
/// * `reference_id` - Either a video_id (from ProcessVideoJob) or draft_id (from AnalyzeVideoJob)
/// * `is_draft` - If true, stores as draft_id; if false, stores as video_id
fn record_analysis_transaction(
    firestore: FirestoreClient,
    user_id: String,
    reference_id: String,
    video_title: String,
    source_url: String,
    credits: u32,
    credits_used_after: u32,
    is_draft: bool,
) {
    tokio::spawn(async move {
        let repo = CreditTransactionRepository::new(firestore, &user_id);

        // Create description with video title for better UX
        let description = if video_title.is_empty() {
            "Video analysis".to_string()
        } else {
            // Truncate very long titles to keep the description reasonable
            let title = if video_title.len() > 50 {
                format!("{}...", &video_title[..47])
            } else {
                video_title.clone()
            };
            format!("Analyzed: {}", title)
        };

        // Store source URL in metadata for "View video" link in frontend
        let mut metadata = HashMap::new();
        if !source_url.is_empty() {
            metadata.insert("source_url".to_string(), source_url);
        }
        if !video_title.is_empty() {
            metadata.insert("video_title".to_string(), video_title);
        }

        // Build transaction with appropriate ID field based on workflow
        let mut tx = CreditTransaction::new(
            uuid::Uuid::new_v4().to_string(),
            user_id.clone(),
            CreditOperationType::Analysis,
            credits,
            description,
            credits_used_after,
        );
        
        if is_draft {
            tx = tx.with_optional_draft_id(Some(reference_id));
        } else {
            tx = tx.with_optional_video_id(Some(reference_id));
        }
        
        tx = tx.with_optional_metadata(if metadata.is_empty() { None } else { Some(metadata) });

        match tokio::time::timeout(Duration::from_secs(5), repo.create(&tx)).await {
            Ok(Ok(())) => {
                debug!(
                    user_id = %user_id,
                    transaction_id = %tx.id,
                    credits = credits,
                    "Recorded analysis credit transaction"
                );
            }
            Ok(Err(e)) => {
                warn!(
                    user_id = %user_id,
                    error = %e,
                    "Failed to record analysis credit transaction"
                );
            }
            Err(_) => {
                warn!(
                    user_id = %user_id,
                    "Analysis credit transaction recording timed out"
                );
            }
        }
    });
}
