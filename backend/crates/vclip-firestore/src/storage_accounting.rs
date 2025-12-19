//! Storage accounting repository.
//!
//! Provides Firestore operations for per-user storage accounting with
//! per-category breakdown (Phase 5: Quota & Storage Tracking Split).

use std::collections::HashMap;

use chrono::Utc;
use tracing::{debug, info};

use vclip_models::StorageAccounting;

use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{FromFirestoreValue, ToFirestoreValue, Value};

/// Collection path for storage accounting documents.
const STORAGE_ACCOUNTING_COLLECTION: &str = "storage_accounting";

/// Repository for storage accounting documents.
///
/// Each user has a single document at `storage_accounting/{user_id}` that
/// tracks their storage usage across all categories.
pub struct StorageAccountingRepository {
    client: FirestoreClient,
    user_id: String,
}

impl StorageAccountingRepository {
    /// Create a new storage accounting repository.
    pub fn new(client: FirestoreClient, user_id: impl Into<String>) -> Self {
        Self {
            client,
            user_id: user_id.into(),
        }
    }

    /// Get the user's storage accounting.
    ///
    /// Returns `None` if no accounting record exists (new user).
    pub async fn get(&self) -> FirestoreResult<Option<StorageAccounting>> {
        let doc = self
            .client
            .get_document(STORAGE_ACCOUNTING_COLLECTION, &self.user_id)
            .await?;

        match doc {
            Some(d) => {
                let accounting = document_to_storage_accounting(&d)?;
                Ok(Some(accounting))
            }
            None => Ok(None),
        }
    }

    /// Get or create the user's storage accounting.
    ///
    /// Creates a new empty record if none exists.
    pub async fn get_or_create(&self) -> FirestoreResult<StorageAccounting> {
        if let Some(accounting) = self.get().await? {
            return Ok(accounting);
        }

        // Create new empty accounting
        let accounting = StorageAccounting::new();
        self.upsert(&accounting).await?;
        Ok(accounting)
    }

    /// Upsert the storage accounting record.
    pub async fn upsert(&self, accounting: &StorageAccounting) -> FirestoreResult<()> {
        let fields = storage_accounting_to_fields(accounting);

        // Try create first, fall back to update
        match self
            .client
            .create_document(STORAGE_ACCOUNTING_COLLECTION, &self.user_id, fields.clone())
            .await
        {
            Ok(_) => {
                info!("Created storage accounting for user: {}", self.user_id);
                Ok(())
            }
            Err(FirestoreError::AlreadyExists(_)) => {
                let update_mask: Vec<String> = fields.keys().cloned().collect();
                self.client
                    .update_document(
                        STORAGE_ACCOUNTING_COLLECTION,
                        &self.user_id,
                        fields,
                        Some(update_mask),
                    )
                    .await?;
                debug!("Updated storage accounting for user: {}", self.user_id);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Maximum retries for atomic updates.
    const MAX_UPDATE_RETRIES: u32 = 5;

    /// Add styled clip storage (billable).
    ///
    /// Uses optimistic concurrency for safe concurrent updates.
    pub async fn add_styled_clip(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.add_styled_clip(bytes))
            .await
    }

    /// Remove styled clip storage (billable).
    pub async fn remove_styled_clip(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.remove_styled_clip(bytes))
            .await
    }

    /// Remove multiple styled clips at once (bulk deletion).
    ///
    /// More efficient than calling `remove_styled_clip` multiple times.
    /// Uses optimistic concurrency for safe concurrent updates.
    pub async fn remove_styled_clips(
        &self,
        bytes: u64,
        count: u32,
    ) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.remove_styled_clips(bytes, count))
            .await
    }

    /// Add source video storage (non-billable).
    pub async fn add_source_video(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.add_source_video(bytes))
            .await
    }

    /// Add raw segment storage (non-billable).
    pub async fn add_raw_segment(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.add_raw_segment(bytes))
            .await
    }

    /// Add neural cache storage (non-billable).
    pub async fn add_neural_cache(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.add_neural_cache(bytes))
            .await
    }

    /// Remove source video storage (non-billable).
    pub async fn remove_source_video(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.remove_source_video(bytes))
            .await
    }

    /// Remove raw segment storage (non-billable).
    pub async fn remove_raw_segment(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.remove_raw_segment(bytes))
            .await
    }

    /// Remove neural cache storage (non-billable).
    pub async fn remove_neural_cache(&self, bytes: u64) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.remove_neural_cache(bytes))
            .await
    }

    /// Clear all non-billable cache storage for a video deletion.
    ///
    /// This zeros out source videos, raw segments, and neural cache bytes.
    /// Used when deleting an entire video to clean up cache accounting.
    pub async fn clear_video_cache(&self) -> FirestoreResult<StorageAccounting> {
        self.update_with_retry(|acc| acc.clear_video_cache())
            .await
    }

    /// Internal helper for concurrency-safe updates with retry.
    async fn update_with_retry<F>(&self, mutator: F) -> FirestoreResult<StorageAccounting>
    where
        F: Fn(&mut StorageAccounting),
    {
        use tracing::warn;

        let mut last_error = None;

        for attempt in 0..Self::MAX_UPDATE_RETRIES {
            // Get current document with update_time
            let doc = self
                .client
                .get_document(STORAGE_ACCOUNTING_COLLECTION, &self.user_id)
                .await?;

            let (mut accounting, update_time) = match doc {
                Some(d) => {
                    let acc = document_to_storage_accounting(&d)?;
                    (acc, d.update_time.clone())
                }
                None => {
                    // No document exists, create new one
                    (StorageAccounting::new(), None)
                }
            };

            // Apply the mutation
            mutator(&mut accounting);
            accounting.updated_at = Some(Utc::now());

            let fields = storage_accounting_to_fields(&accounting);
            let update_mask: Vec<String> = fields.keys().cloned().collect();

            // If no existing document, create it
            if update_time.is_none() {
                match self
                    .client
                    .create_document(STORAGE_ACCOUNTING_COLLECTION, &self.user_id, fields)
                    .await
                {
                    Ok(_) => return Ok(accounting),
                    Err(FirestoreError::AlreadyExists(_)) => {
                        // Race condition, retry
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }

            // Update with precondition
            match self
                .client
                .update_document_with_precondition(
                    STORAGE_ACCOUNTING_COLLECTION,
                    &self.user_id,
                    fields,
                    Some(update_mask),
                    update_time.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    return Ok(accounting);
                }
                Err(e) if e.is_precondition_failed() => {
                    debug!(
                        "Storage accounting update precondition failed for {} (attempt {}), retrying",
                        self.user_id, attempt + 1
                    );
                    last_error = Some(e);
                    // Brief backoff before retry
                    tokio::time::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1)))
                        .await;
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // All retries exhausted
        warn!(
            "Storage accounting update failed after {} retries for {}: {:?}",
            Self::MAX_UPDATE_RETRIES,
            self.user_id,
            last_error
        );
        Err(FirestoreError::request_failed(format!(
            "Failed to update storage accounting after {} retries",
            Self::MAX_UPDATE_RETRIES
        )))
    }

    /// Check if adding bytes would exceed quota.
    pub async fn would_exceed_quota(
        &self,
        additional_bytes: u64,
        limit_bytes: u64,
    ) -> FirestoreResult<bool> {
        let accounting = self.get_or_create().await?;
        Ok(accounting.would_exceed_quota(additional_bytes, limit_bytes))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn storage_accounting_to_fields(accounting: &StorageAccounting) -> HashMap<String, Value> {
    let mut fields = HashMap::new();

    fields.insert(
        "styled_clips_bytes".to_string(),
        accounting.styled_clips_bytes.to_firestore_value(),
    );
    fields.insert(
        "styled_clips_count".to_string(),
        accounting.styled_clips_count.to_firestore_value(),
    );
    fields.insert(
        "source_videos_bytes".to_string(),
        accounting.source_videos_bytes.to_firestore_value(),
    );
    fields.insert(
        "raw_segments_bytes".to_string(),
        accounting.raw_segments_bytes.to_firestore_value(),
    );
    fields.insert(
        "neural_cache_bytes".to_string(),
        accounting.neural_cache_bytes.to_firestore_value(),
    );

    if let Some(updated_at) = accounting.updated_at {
        fields.insert("updated_at".to_string(), updated_at.to_firestore_value());
    } else {
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());
    }

    fields
}

fn document_to_storage_accounting(
    doc: &crate::types::Document,
) -> FirestoreResult<StorageAccounting> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    let get_u64 = |key: &str| -> u64 {
        fields
            .get(key)
            .and_then(|v| u64::from_firestore_value(v))
            .unwrap_or(0)
    };

    let get_u32 = |key: &str| -> u32 {
        fields
            .get(key)
            .and_then(|v| u32::from_firestore_value(v))
            .unwrap_or(0)
    };

    Ok(StorageAccounting {
        styled_clips_bytes: get_u64("styled_clips_bytes"),
        styled_clips_count: get_u32("styled_clips_count"),
        source_videos_bytes: get_u64("source_videos_bytes"),
        raw_segments_bytes: get_u64("raw_segments_bytes"),
        neural_cache_bytes: get_u64("neural_cache_bytes"),
        updated_at: fields
            .get("updated_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_accounting_to_fields() {
        let mut accounting = StorageAccounting::new();
        accounting.styled_clips_bytes = 1024 * 1024; // 1 MB
        accounting.styled_clips_count = 5;
        accounting.source_videos_bytes = 10 * 1024 * 1024; // 10 MB
        accounting.raw_segments_bytes = 5 * 1024 * 1024; // 5 MB
        accounting.neural_cache_bytes = 512 * 1024; // 512 KB

        let fields = storage_accounting_to_fields(&accounting);

        assert!(fields.contains_key("styled_clips_bytes"));
        assert!(fields.contains_key("styled_clips_count"));
        assert!(fields.contains_key("source_videos_bytes"));
        assert!(fields.contains_key("raw_segments_bytes"));
        assert!(fields.contains_key("neural_cache_bytes"));
        assert!(fields.contains_key("updated_at"));
    }
}
