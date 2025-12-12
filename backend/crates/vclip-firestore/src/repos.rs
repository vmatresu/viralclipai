//! Typed repositories for Videos and Clips.

use std::collections::HashMap;

use chrono::Utc;
use metrics::counter;
use tracing::info;

use vclip_models::{ClipMetadata, ClipStatus, SourceVideoStatus, VideoId, VideoMetadata, VideoStatus};

use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{FromFirestoreValue, ToFirestoreValue, Value};

/// Repository for video documents.
pub struct VideoRepository {
    client: FirestoreClient,
    user_id: String,
}

impl VideoRepository {
    /// Create a new video repository.
    pub fn new(client: FirestoreClient, user_id: impl Into<String>) -> Self {
        Self {
            client,
            user_id: user_id.into(),
        }
    }

    /// Collection path for user's videos.
    fn collection(&self) -> String {
        format!("users/{}/videos", self.user_id)
    }

    /// Get a video by ID.
    pub async fn get(&self, video_id: &VideoId) -> FirestoreResult<Option<VideoMetadata>> {
        let doc = self.client.get_document(&self.collection(), video_id.as_str()).await?;

        match doc {
            Some(d) => {
                let meta = document_to_video_metadata(&d, video_id)?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// Create a new video record.
    pub async fn create(&self, video: &VideoMetadata) -> FirestoreResult<()> {
        let fields = video_metadata_to_fields(video);
        self.client
            .create_document(&self.collection(), video.video_id.as_str(), fields)
            .await?;
        info!("Created video record: {}", video.video_id);
        Ok(())
    }

    /// Update video status.
    pub async fn update_status(
        &self,
        video_id: &VideoId,
        status: VideoStatus,
    ) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), status.as_str().to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["status".to_string(), "updated_at".to_string()]),
            )
            .await?;
        Ok(())
    }

    /// Mark video as completed.
    pub async fn complete(&self, video_id: &VideoId, clips_count: u32) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            VideoStatus::Completed.as_str().to_firestore_value(),
        );
        fields.insert("clips_count".to_string(), clips_count.to_firestore_value());
        fields.insert("completed_at".to_string(), Utc::now().to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec![
                    "status".to_string(),
                    "clips_count".to_string(),
                    "completed_at".to_string(),
                    "updated_at".to_string(),
                ]),
            )
            .await?;
        Ok(())
    }

    /// Update the clip count without touching status/timestamps unrelated to completion.
    pub async fn update_clips_count(&self, video_id: &VideoId, clips_count: u32) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert("clips_count".to_string(), clips_count.to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["clips_count".to_string(), "updated_at".to_string()]),
            )
            .await?;
        Ok(())
    }

    /// Set the expected number of clips for orchestration tracking.
    ///
    /// Called by orchestration jobs when fanning out render jobs.
    pub async fn set_expected_clips(&self, video_id: &VideoId, expected_clips: u32) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert("expected_clips".to_string(), expected_clips.to_firestore_value());
        fields.insert("completed_clips".to_string(), 0u32.to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec![
                    "expected_clips".to_string(),
                    "completed_clips".to_string(),
                    "updated_at".to_string(),
                ]),
            )
            .await?;
        Ok(())
    }

    /// Add to the expected clips count (for reprocessing additional scenes).
    pub async fn add_expected_clips(&self, video_id: &VideoId, additional_clips: u32) -> FirestoreResult<()> {
        // Get current expected_clips value from document
        let doc = self.client.get_document(&self.collection(), video_id.as_str()).await?;
        let current_expected = if let Some(ref d) = doc {
            d.fields.as_ref()
                .and_then(|f| f.get("expected_clips"))
                .and_then(|v| u32::from_firestore_value(v))
                .unwrap_or(0)
        } else {
            0
        };

        let mut fields = HashMap::new();
        fields.insert(
            "expected_clips".to_string(),
            (current_expected + additional_clips).to_firestore_value(),
        );
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["expected_clips".to_string(), "updated_at".to_string()]),
            )
            .await?;
        Ok(())
    }

    /// Increment the completed clips count by 1.
    ///
    /// Called by each render job upon successful completion.
    /// Returns the new completed count.
    ///
    /// Note: This is not truly atomic; for high concurrency, consider
    /// using Firestore transactions or Cloud Functions.
    pub async fn increment_completed_clips(&self, video_id: &VideoId) -> FirestoreResult<u32> {
        // Get current completed_clips value
        let doc = self.client.get_document(&self.collection(), video_id.as_str()).await?;
        let current = if let Some(ref d) = doc {
            d.fields.as_ref()
                .and_then(|f| f.get("completed_clips"))
                .and_then(|v| u32::from_firestore_value(v))
                .unwrap_or(0)
        } else {
            0
        };

        let new_count = current + 1;

        let mut fields = HashMap::new();
        fields.insert("completed_clips".to_string(), new_count.to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["completed_clips".to_string(), "updated_at".to_string()]),
            )
            .await?;

        // Also update clips_count for backward compatibility
        self.update_clips_count(video_id, new_count).await.ok();

        Ok(new_count)
    }

    /// Check if video is complete and mark it as such.
    ///
    /// Called after incrementing completed_clips to check if all expected
    /// clips have been processed.
    pub async fn check_and_complete_if_ready(&self, video_id: &VideoId) -> FirestoreResult<bool> {
        let doc = self.client.get_document(&self.collection(), video_id.as_str()).await?;

        let (expected, completed) = if let Some(ref d) = doc {
            let fields = d.fields.as_ref();
            let expected = fields
                .and_then(|f| f.get("expected_clips"))
                .and_then(|v| u32::from_firestore_value(v))
                .unwrap_or(0);
            let completed = fields
                .and_then(|f| f.get("completed_clips"))
                .and_then(|v| u32::from_firestore_value(v))
                .unwrap_or(0);
            (expected, completed)
        } else {
            return Ok(false);
        };

        if expected > 0 && completed >= expected {
            // All clips are done, mark video as completed
            self.complete(video_id, completed).await?;
            info!(
                "Video {} automatically completed: {}/{} clips",
                video_id, completed, expected
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Mark video as failed.
    pub async fn fail(&self, video_id: &VideoId, error: &str) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            VideoStatus::Failed.as_str().to_firestore_value(),
        );
        fields.insert("error_message".to_string(), error.to_firestore_value());
        fields.insert("failed_at".to_string(), Utc::now().to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec![
                    "status".to_string(),
                    "error_message".to_string(),
                    "failed_at".to_string(),
                    "updated_at".to_string(),
                ]),
            )
            .await?;
        Ok(())
    }

    /// Update the total size of all clips for this video.
    pub async fn update_total_size(&self, video_id: &VideoId, total_size_bytes: u64) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert("total_size_bytes".to_string(), total_size_bytes.to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["total_size_bytes".to_string(), "updated_at".to_string()]),
            )
            .await?;
        Ok(())
    }

    /// Maximum retries for optimistic concurrency updates.
    const MAX_SIZE_UPDATE_RETRIES: u32 = 5;

    /// Add to the total size (when a clip is created).
    /// Uses optimistic locking to handle concurrent clip creation safely.
    pub async fn add_clip_size(&self, video_id: &VideoId, size_bytes: u64) -> FirestoreResult<u64> {
        self.update_clip_size_with_retry(video_id, size_bytes as i64).await
    }

    /// Subtract from the total size (when a clip is deleted).
    /// Uses optimistic locking to handle concurrent clip deletion safely.
    pub async fn subtract_clip_size(&self, video_id: &VideoId, size_bytes: u64) -> FirestoreResult<u64> {
        self.update_clip_size_with_retry(video_id, -(size_bytes as i64)).await
    }

    /// Internal helper for concurrency-safe video size updates with retry.
    async fn update_clip_size_with_retry(
        &self,
        video_id: &VideoId,
        size_delta: i64,
    ) -> FirestoreResult<u64> {
        use tracing::{debug, warn};

        let mut last_error = None;

        for attempt in 0..Self::MAX_SIZE_UPDATE_RETRIES {
            // Get current document with update_time
            let doc = self.client.get_document(&self.collection(), video_id.as_str()).await?;

            let (current_size, update_time) = match &doc {
                Some(d) => {
                    let size = d.fields.as_ref()
                        .and_then(|f| f.get("total_size_bytes"))
                        .and_then(|v| u64::from_firestore_value(v))
                        .unwrap_or(0);
                    (size, d.update_time.clone())
                }
                None => {
                    // Video doesn't exist - this shouldn't happen
                    return Err(FirestoreError::not_found(format!(
                        "Video {} not found",
                        video_id.as_str()
                    )));
                }
            };

            // Calculate new size with safe arithmetic
            let new_size = if size_delta >= 0 {
                current_size.saturating_add(size_delta as u64)
            } else {
                current_size.saturating_sub((-size_delta) as u64)
            };

            // Build update fields
            let mut fields = HashMap::new();
            fields.insert("total_size_bytes".to_string(), new_size.to_firestore_value());
            fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

            let update_mask = vec![
                "total_size_bytes".to_string(),
                "updated_at".to_string(),
            ];

            // Attempt update with precondition
            match self.client
                .update_document_with_precondition(
                    &self.collection(),
                    video_id.as_str(),
                    fields,
                    Some(update_mask),
                    update_time.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    return Ok(new_size);
                }
                Err(e) if e.is_precondition_failed() => {
                    // Another writer updated the document; retry
                    debug!(
                        "Video size update precondition failed for {} (attempt {}), retrying",
                        video_id.as_str(), attempt + 1
                    );
                    last_error = Some(e);
                    // Brief backoff before retry
                    tokio::time::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1))).await;
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // All retries exhausted
        warn!(
            "Video size update failed after {} retries for {}: {:?}",
            Self::MAX_SIZE_UPDATE_RETRIES, video_id.as_str(), last_error
        );
        Err(FirestoreError::request_failed(format!(
            "Failed to update video size after {} retries",
            Self::MAX_SIZE_UPDATE_RETRIES
        )))
    }

    /// Recalculate total size from all clips (for consistency/migration).
    pub async fn recalculate_total_size(&self, video_id: &VideoId) -> FirestoreResult<u64> {
        let clip_repo = ClipRepository::new(
            self.client.clone(),
            &self.user_id,
            video_id.clone(),
        );

        let clips = clip_repo.list(None).await?;
        let total_size: u64 = clips.iter().map(|c| c.file_size_bytes).sum();

        self.update_total_size(video_id, total_size).await?;

        Ok(total_size)
    }

    /// Delete a video.
    pub async fn delete(&self, video_id: &VideoId) -> FirestoreResult<bool> {
        self.client
            .delete_document(&self.collection(), video_id.as_str())
            .await?;
        Ok(true)
    }

    /// List all videos for the user.
    pub async fn list(&self, limit: Option<u32>) -> FirestoreResult<Vec<VideoMetadata>> {
        let response = self.client.list_documents(&self.collection(), limit, None).await?;

        let mut videos = Vec::new();
        if let Some(docs) = response.documents {
            for doc in docs {
                if let Some(name) = &doc.name {
                    // Extract video_id from document path
                    let video_id = name.split('/').last().unwrap_or("").to_string();
                    if let Ok(meta) = document_to_video_metadata(&doc, &VideoId::from_string(video_id)) {
                        videos.push(meta);
                    }
                }
            }
        }

        Ok(videos)
    }

    // ========================================================================
    // Source Video Status Methods (Phase 2.2)
    // ========================================================================

    /// Set source video status to Downloading.
    /// Called when starting the background download.
    pub async fn set_source_video_downloading(&self, video_id: &VideoId) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "source_video_status".to_string(),
            SourceVideoStatus::Downloading.as_str().to_firestore_value(),
        );
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["source_video_status".to_string(), "updated_at".to_string()]),
            )
            .await?;

        info!("Set source video status to downloading: {}", video_id);
        Ok(())
    }

    /// Set source video status to Ready with R2 key and expiration.
    /// Called when background download completes successfully.
    pub async fn set_source_video_ready(
        &self,
        video_id: &VideoId,
        r2_key: &str,
        expires_at: chrono::DateTime<Utc>,
    ) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "source_video_status".to_string(),
            SourceVideoStatus::Ready.as_str().to_firestore_value(),
        );
        fields.insert("source_video_r2_key".to_string(), r2_key.to_firestore_value());
        fields.insert("source_video_expires_at".to_string(), expires_at.to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec![
                    "source_video_status".to_string(),
                    "source_video_r2_key".to_string(),
                    "source_video_expires_at".to_string(),
                    "updated_at".to_string(),
                ]),
            )
            .await?;

        info!("Set source video status to ready: {} (key: {})", video_id, r2_key);
        Ok(())
    }

    /// Set source video status to Failed with error message.
    /// Called when background download fails.
    pub async fn set_source_video_failed(
        &self,
        video_id: &VideoId,
        error_message: Option<&str>,
    ) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "source_video_status".to_string(),
            SourceVideoStatus::Failed.as_str().to_firestore_value(),
        );
        if let Some(msg) = error_message {
            fields.insert("source_video_error".to_string(), msg.to_firestore_value());
        }
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        let mut update_mask = vec!["source_video_status".to_string(), "updated_at".to_string()];
        if error_message.is_some() {
            update_mask.push("source_video_error".to_string());
        }

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(update_mask),
            )
            .await?;

        info!("Set source video status to failed: {}", video_id);
        Ok(())
    }

    /// Set source video status to Expired.
    /// Called when cached source video is past its TTL.
    pub async fn set_source_video_expired(&self, video_id: &VideoId) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "source_video_status".to_string(),
            SourceVideoStatus::Expired.as_str().to_firestore_value(),
        );
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                video_id.as_str(),
                fields,
                Some(vec!["source_video_status".to_string(), "updated_at".to_string()]),
            )
            .await?;

        info!("Set source video status to expired: {}", video_id);
        Ok(())
    }
}

/// Repository for clip documents.
pub struct ClipRepository {
    client: FirestoreClient,
    user_id: String,
    video_id: VideoId,
}

impl ClipRepository {
    /// Create a new clip repository.
    pub fn new(
        client: FirestoreClient,
        user_id: impl Into<String>,
        video_id: VideoId,
    ) -> Self {
        Self {
            client,
            user_id: user_id.into(),
            video_id,
        }
    }

    /// Collection path for video's clips.
    fn collection(&self) -> String {
        format!(
            "users/{}/videos/{}/clips",
            self.user_id,
            self.video_id.as_str()
        )
    }

    /// Create a clip record.
    pub async fn create(&self, clip: &ClipMetadata) -> FirestoreResult<()> {
        let fields = clip_metadata_to_fields(clip);
        match self
            .client
            .create_document(&self.collection(), &clip.clip_id, fields.clone())
            .await
        {
            Ok(_) => {
                info!("Created clip record: {}", clip.clip_id);
                Ok(())
            }
            Err(FirestoreError::AlreadyExists(_)) => {
                // Upsert existing clip metadata to keep storage and Firestore in sync
                let update_mask: Vec<String> = fields.keys().cloned().collect();
                self.client
                    .update_document(&self.collection(), &clip.clip_id, fields, Some(update_mask))
                    .await?;
                counter!("clip_metadata_upsert_total", "outcome" => "updated").increment(1);
                info!("Updated existing clip record: {}", clip.clip_id);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Update clip status to completed.
    pub async fn complete(
        &self,
        clip_id: &str,
        file_size_bytes: u64,
        has_thumbnail: bool,
    ) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            ClipStatus::Completed.as_str().to_firestore_value(),
        );
        fields.insert("file_size_bytes".to_string(), file_size_bytes.to_firestore_value());
        fields.insert(
            "file_size_mb".to_string(),
            (file_size_bytes as f64 / (1024.0 * 1024.0)).to_firestore_value(),
        );
        fields.insert("has_thumbnail".to_string(), has_thumbnail.to_firestore_value());
        fields.insert("completed_at".to_string(), Utc::now().to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                clip_id,
                fields,
                Some(vec![
                    "status".to_string(),
                    "file_size_bytes".to_string(),
                    "file_size_mb".to_string(),
                    "has_thumbnail".to_string(),
                    "completed_at".to_string(),
                    "updated_at".to_string(),
                ]),
            )
            .await?;
        Ok(())
    }

    /// Delete a clip by filename.
    pub async fn delete_by_filename(&self, filename: &str) -> FirestoreResult<bool> {
        // First, list all clips to find the one with matching filename
        let clips = self.list(None).await?;

        // Find the clip with the matching filename
        if let Some(clip) = clips.into_iter().find(|c| c.filename == filename) {
            // Delete the document using the clip_id as document ID
            self.client
                .delete_document(&self.collection(), &clip.clip_id)
                .await?;
            info!("Deleted clip record: {}", clip.clip_id);
            Ok(true)
        } else {
            // Clip not found
            Ok(false)
        }
    }

    /// List clips for the video.
    pub async fn list(&self, status: Option<ClipStatus>) -> FirestoreResult<Vec<ClipMetadata>> {
        let response = self.client.list_documents(&self.collection(), None, None).await?;

        let mut clips = Vec::new();
        if let Some(docs) = response.documents {
            for doc in docs {
                if let Ok(meta) = document_to_clip_metadata(&doc) {
                    // Filter by status if specified
                    if let Some(filter_status) = status {
                        if meta.status == filter_status {
                            clips.push(meta);
                        }
                    } else {
                        clips.push(meta);
                    }
                }
            }
        }

        Ok(clips)
    }

    /// Set the raw segment R2 key for a clip.
    /// Called when a raw segment is extracted and uploaded to R2.
    pub async fn set_raw_r2_key(&self, clip_id: &str, raw_r2_key: &str) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert("raw_r2_key".to_string(), raw_r2_key.to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        self.client
            .update_document(
                &self.collection(),
                clip_id,
                fields,
                Some(vec!["raw_r2_key".to_string(), "updated_at".to_string()]),
            )
            .await?;
        info!("Set raw R2 key for clip {}: {}", clip_id, raw_r2_key);
        Ok(())
    }
}

// Helper functions for conversion

fn video_metadata_to_fields(video: &VideoMetadata) -> HashMap<String, Value> {
    let mut fields = HashMap::new();
    fields.insert("video_id".to_string(), video.video_id.as_str().to_firestore_value());
    fields.insert("user_id".to_string(), video.user_id.to_firestore_value());
    fields.insert("video_url".to_string(), video.video_url.to_firestore_value());
    fields.insert("video_title".to_string(), video.video_title.to_firestore_value());
    fields.insert("youtube_id".to_string(), video.youtube_id.to_firestore_value());
    fields.insert("status".to_string(), video.status.as_str().to_firestore_value());
    fields.insert("created_at".to_string(), video.created_at.to_firestore_value());
    fields.insert("updated_at".to_string(), video.updated_at.to_firestore_value());
    fields.insert("completed_at".to_string(), video.completed_at.to_firestore_value());
    fields.insert("failed_at".to_string(), video.failed_at.to_firestore_value());
    fields.insert("error_message".to_string(), video.error_message.to_firestore_value());
    fields.insert("highlights_count".to_string(), video.highlights_count.to_firestore_value());
    fields.insert("custom_prompt".to_string(), video.custom_prompt.to_firestore_value());
    fields.insert("styles_processed".to_string(), video.styles_processed.to_firestore_value());
    fields.insert("crop_mode".to_string(), video.crop_mode.to_firestore_value());
    fields.insert("target_aspect".to_string(), video.target_aspect.to_firestore_value());
    fields.insert("clips_count".to_string(), video.clips_count.to_firestore_value());
    fields.insert("total_size_bytes".to_string(), video.total_size_bytes.to_firestore_value());
    fields.insert("clips_by_style".to_string(), video.clips_by_style.to_firestore_value());
    fields.insert("highlights_json_key".to_string(), video.highlights_json_key.to_firestore_value());
    fields.insert("created_by".to_string(), video.created_by.to_firestore_value());

    // Source video fields (Phase 2.2)
    if let Some(ref key) = video.source_video_r2_key {
        fields.insert("source_video_r2_key".to_string(), key.to_firestore_value());
    }
    if let Some(status) = video.source_video_status {
        fields.insert("source_video_status".to_string(), status.as_str().to_firestore_value());
    }
    if let Some(expires_at) = video.source_video_expires_at {
        fields.insert("source_video_expires_at".to_string(), expires_at.to_firestore_value());
    }
    if let Some(ref error) = video.source_video_error {
        fields.insert("source_video_error".to_string(), error.to_firestore_value());
    }

    fields
}

fn document_to_video_metadata(
    doc: &crate::types::Document,
    video_id: &VideoId,
) -> FirestoreResult<VideoMetadata> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    let get_string = |key: &str| -> String {
        fields
            .get(key)
            .and_then(|v| String::from_firestore_value(v))
            .unwrap_or_default()
    };

    let get_u32 = |key: &str| -> u32 {
        fields
            .get(key)
            .and_then(|v| u32::from_firestore_value(v))
            .unwrap_or(0)
    };

    let get_u64 = |key: &str| -> u64 {
        fields
            .get(key)
            .and_then(|v| u64::from_firestore_value(v))
            .unwrap_or(0)
    };

    Ok(VideoMetadata {
        video_id: video_id.clone(),
        user_id: get_string("user_id"),
        video_url: get_string("video_url"),
        video_title: get_string("video_title"),
        youtube_id: get_string("youtube_id"),
        status: match get_string("status").as_str() {
            "completed" => VideoStatus::Completed,
            "analyzed" => VideoStatus::Analyzed,
            "failed" => VideoStatus::Failed,
            _ => VideoStatus::Processing,
        },
        created_at: fields
            .get("created_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
        updated_at: fields
            .get("updated_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
        completed_at: fields
            .get("completed_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v)),
        failed_at: fields
            .get("failed_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v)),
        error_message: fields
            .get("error_message")
            .and_then(|v| String::from_firestore_value(v)),
        highlights_count: get_u32("highlights_count"),
        custom_prompt: fields
            .get("custom_prompt")
            .and_then(|v| String::from_firestore_value(v)),
        styles_processed: fields
            .get("styles_processed")
            .and_then(|v| match v {
                Value::ArrayValue(arr) => arr.values.as_ref().map(|vals| {
                    vals.iter()
                        .filter_map(|vv| String::from_firestore_value(vv))
                        .collect::<Vec<String>>()
                }),
                _ => None,
            })
            .unwrap_or_default(),
        crop_mode: get_string("crop_mode"),
        target_aspect: get_string("target_aspect"),
        clips_count: get_u32("clips_count"),
        total_size_bytes: get_u64("total_size_bytes"),
        clips_by_style: fields
            .get("clips_by_style")
            .and_then(|v| match v {
                Value::MapValue(map) => map.fields.as_ref().map(|m| {
                    m.iter()
                        .filter_map(|(k, vv)| {
                            u32::from_firestore_value(vv).map(|n| (k.clone(), n))
                        })
                        .collect::<HashMap<String, u32>>()
                }),
                _ => None,
            })
            .unwrap_or_default(),
        highlights_json_key: get_string("highlights_json_key"),
        created_by: get_string("created_by"),
        // Source video fields (Phase 2.2)
        source_video_r2_key: fields
            .get("source_video_r2_key")
            .and_then(|v| String::from_firestore_value(v)),
        source_video_status: fields
            .get("source_video_status")
            .and_then(|v| String::from_firestore_value(v))
            .and_then(|s| match s.as_str() {
                "pending" => Some(SourceVideoStatus::Pending),
                "downloading" => Some(SourceVideoStatus::Downloading),
                "ready" => Some(SourceVideoStatus::Ready),
                "expired" => Some(SourceVideoStatus::Expired),
                "failed" => Some(SourceVideoStatus::Failed),
                _ => None,
            }),
        source_video_expires_at: fields
            .get("source_video_expires_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v)),
        source_video_error: fields
            .get("source_video_error")
            .and_then(|v| String::from_firestore_value(v)),
    })
}

fn clip_metadata_to_fields(clip: &ClipMetadata) -> HashMap<String, Value> {
    let mut fields = HashMap::new();
    fields.insert("clip_id".to_string(), clip.clip_id.to_firestore_value());
    fields.insert("video_id".to_string(), clip.video_id.as_str().to_firestore_value());
    fields.insert("user_id".to_string(), clip.user_id.to_firestore_value());
    fields.insert("scene_id".to_string(), clip.scene_id.to_firestore_value());
    fields.insert("scene_title".to_string(), clip.scene_title.to_firestore_value());
    if let Some(ref desc) = clip.scene_description {
        fields.insert("scene_description".to_string(), desc.to_firestore_value());
    }
    fields.insert("filename".to_string(), clip.filename.to_firestore_value());
    fields.insert("style".to_string(), clip.style.to_firestore_value());
    fields.insert("priority".to_string(), clip.priority.to_firestore_value());
    fields.insert("start_time".to_string(), clip.start_time.to_firestore_value());
    fields.insert("end_time".to_string(), clip.end_time.to_firestore_value());
    fields.insert("duration_seconds".to_string(), clip.duration_seconds.to_firestore_value());
    fields.insert("file_size_bytes".to_string(), clip.file_size_bytes.to_firestore_value());
    fields.insert("file_size_mb".to_string(), clip.file_size_mb.to_firestore_value());
    fields.insert("has_thumbnail".to_string(), clip.has_thumbnail.to_firestore_value());
    fields.insert("r2_key".to_string(), clip.r2_key.to_firestore_value());
    if let Some(ref thumb_key) = clip.thumbnail_r2_key {
        fields.insert("thumbnail_r2_key".to_string(), thumb_key.to_firestore_value());
    }
    if let Some(ref raw_key) = clip.raw_r2_key {
        fields.insert("raw_r2_key".to_string(), raw_key.to_firestore_value());
    }
    fields.insert("status".to_string(), clip.status.as_str().to_firestore_value());
    fields.insert("created_at".to_string(), clip.created_at.to_firestore_value());
    if let Some(completed_at) = clip.completed_at {
        fields.insert("completed_at".to_string(), completed_at.to_firestore_value());
    }
    // Always persist an updated_at to support deterministic upserts
    let updated_at = clip.updated_at.unwrap_or_else(Utc::now);
    fields.insert("updated_at".to_string(), updated_at.to_firestore_value());
    fields.insert("created_by".to_string(), clip.created_by.to_firestore_value());
    fields
}

fn document_to_clip_metadata(doc: &crate::types::Document) -> FirestoreResult<ClipMetadata> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    let get_string = |key: &str| -> String {
        fields
            .get(key)
            .and_then(|v| String::from_firestore_value(v))
            .unwrap_or_default()
    };

    let get_u32 = |key: &str| -> u32 {
        fields
            .get(key)
            .and_then(|v| u32::from_firestore_value(v))
            .unwrap_or(0)
    };

    let get_f64 = |key: &str| -> f64 {
        fields
            .get(key)
            .and_then(|v| f64::from_firestore_value(v))
            .unwrap_or(0.0)
    };

    Ok(ClipMetadata {
        clip_id: get_string("clip_id"),
        video_id: VideoId::from_string(get_string("video_id")),
        user_id: get_string("user_id"),
        scene_id: get_u32("scene_id"),
        scene_title: get_string("scene_title"),
        scene_description: fields
            .get("scene_description")
            .and_then(|v| String::from_firestore_value(v)),
        filename: get_string("filename"),
        style: get_string("style"),
        priority: get_u32("priority"),
        start_time: get_string("start_time"),
        end_time: get_string("end_time"),
        duration_seconds: get_f64("duration_seconds"),
        file_size_bytes: fields
            .get("file_size_bytes")
            .and_then(|v| u64::from_firestore_value(v))
            .unwrap_or(0),
        file_size_mb: get_f64("file_size_mb"),
        has_thumbnail: fields
            .get("has_thumbnail")
            .and_then(|v| bool::from_firestore_value(v))
            .unwrap_or(false),
        r2_key: get_string("r2_key"),
        thumbnail_r2_key: fields
            .get("thumbnail_r2_key")
            .and_then(|v| String::from_firestore_value(v)),
        raw_r2_key: fields
            .get("raw_r2_key")
            .and_then(|v| String::from_firestore_value(v)),
        status: match get_string("status").as_str() {
            "completed" => ClipStatus::Completed,
            "failed" => ClipStatus::Failed,
            _ => ClipStatus::Processing,
        },
        created_at: fields
            .get("created_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
        completed_at: fields
            .get("completed_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v)),
        updated_at: fields
            .get("updated_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v)),
        created_by: get_string("created_by"),
    })
}

// ============================================================================
// Share Repository
// ============================================================================

use vclip_models::share::{ShareAccessLevel, ShareConfig};
use crate::types::{Document, Precondition, Write};

/// Minimal slug index document for fast lookup.
#[derive(Debug, Clone)]
pub struct ShareSlugIndex {
    pub share_slug: String,
    pub user_id: String,
    pub video_id: String,
    pub clip_id: String,
    pub access_level: ShareAccessLevel,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub disabled_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

/// Repository for share documents (dual-document pattern).
pub struct ShareRepository {
    client: FirestoreClient,
}

impl ShareRepository {
    /// Create a new share repository.
    pub fn new(client: FirestoreClient) -> Self {
        Self { client }
    }

    /// Config document path: users/{user_id}/videos/{video_id}/clips/{clip_id}/shares/config
    fn config_path(user_id: &str, video_id: &str, clip_id: &str) -> String {
        format!(
            "users/{}/videos/{}/clips/{}/shares",
            user_id, video_id, clip_id
        )
    }

    /// Slug index collection path: share_slugs
    fn slug_collection() -> &'static str {
        "share_slugs"
    }

    /// Create or update a share config with atomic dual-write pattern.
    ///
    /// This performs an atomic batch write of two documents:
    /// 1. Config doc at users/{uid}/videos/{vid}/clips/{cid}/shares/config
    /// 2. Slug index at share_slugs/{slug}
    ///
    /// Both writes succeed or fail together, preventing "zombie shares"
    /// (config exists but no slug index, or vice versa).
    pub async fn create_share(&self, config: &ShareConfig) -> FirestoreResult<()> {
        let config_fields = share_config_to_fields(config);
        let slug_fields = share_slug_index_to_fields(config);

        // Build full document names for batch write
        let config_collection = Self::config_path(&config.user_id, &config.video_id, &config.clip_id);
        let config_doc_name = self.client.full_document_name(&config_collection, "config");
        let slug_doc_name = self.client.full_document_name(Self::slug_collection(), &config.share_slug);

        // Create batch writes (upsert semantics)
        let writes = vec![
            Write {
                update: Some(Document {
                    name: Some(config_doc_name),
                    fields: Some(config_fields),
                    create_time: None,
                    update_time: None,
                }),
                delete: None,
                update_mask: None, // No mask = full document upsert
                current_document: None,
            },
            Write {
                update: Some(Document {
                    name: Some(slug_doc_name),
                    fields: Some(slug_fields),
                    create_time: None,
                    update_time: None,
                }),
                delete: None,
                update_mask: None,
                current_document: None,
            },
        ];

        // Execute atomic batch write
        self.client.batch_write(writes).await?;

        info!(
            "Created share (atomic): slug={}, clip_id={}, user_id={}",
            config.share_slug, config.clip_id, config.user_id
        );

        Ok(())
    }

    /// Disable a share atomically (update config and delete slug index).
    ///
    /// This performs an atomic batch write:
    /// 1. Update config doc with disabled_at timestamp
    /// 2. Delete slug index document
    ///
    /// Both operations succeed or fail together.
    pub async fn disable_share(
        &self,
        user_id: &str,
        video_id: &str,
        clip_id: &str,
        share_slug: &str,
    ) -> FirestoreResult<()> {
        let now = Utc::now();

        // Build config update fields
        let mut config_fields = HashMap::new();
        config_fields.insert("disabled_at".to_string(), now.to_firestore_value());
        config_fields.insert("access_level".to_string(), ShareAccessLevel::None.as_str().to_firestore_value());
        config_fields.insert("updated_at".to_string(), now.to_firestore_value());

        // Build full document names
        let config_collection = Self::config_path(user_id, video_id, clip_id);
        let config_doc_name = self.client.full_document_name(&config_collection, "config");
        let slug_doc_name = self.client.full_document_name(Self::slug_collection(), share_slug);

        // Create batch writes: update config + delete slug
        let writes = vec![
            Write {
                update: Some(Document {
                    name: Some(config_doc_name),
                    fields: Some(config_fields),
                    create_time: None,
                    update_time: None,
                }),
                delete: None,
                update_mask: Some(crate::types::DocumentMask {
                    field_paths: vec![
                        "disabled_at".to_string(),
                        "access_level".to_string(),
                        "updated_at".to_string(),
                    ],
                }),
                current_document: Some(Precondition {
                    exists: Some(true),
                    update_time: None,
                }),
            },
            Write {
                update: None,
                delete: Some(slug_doc_name),
                update_mask: None,
                current_document: None, // Idempotent delete
            },
        ];

        // Execute atomic batch write
        self.client.batch_write(writes).await?;

        info!(
            "Disabled share (atomic): slug={}, clip_id={}, user_id={}",
            share_slug, clip_id, user_id
        );

        Ok(())
    }

    /// Get share config by looking up the slug index.
    pub async fn get_by_slug(&self, slug: &str) -> FirestoreResult<Option<ShareSlugIndex>> {
        let doc = self.client.get_document(Self::slug_collection(), slug).await?;

        match doc {
            Some(d) => {
                let index = document_to_share_slug_index(&d)?;
                Ok(Some(index))
            }
            None => Ok(None),
        }
    }

    /// Get share config for a clip.
    pub async fn get_config(
        &self,
        user_id: &str,
        video_id: &str,
        clip_id: &str,
    ) -> FirestoreResult<Option<ShareConfig>> {
        let config_path = Self::config_path(user_id, video_id, clip_id);
        let doc = self.client.get_document(&config_path, "config").await?;

        match doc {
            Some(d) => {
                let config = document_to_share_config(&d)?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }
}

fn share_config_to_fields(config: &ShareConfig) -> HashMap<String, Value> {
    let mut fields = HashMap::new();
    fields.insert("share_slug".to_string(), config.share_slug.to_firestore_value());
    fields.insert("clip_id".to_string(), config.clip_id.to_firestore_value());
    fields.insert("user_id".to_string(), config.user_id.to_firestore_value());
    fields.insert("video_id".to_string(), config.video_id.to_firestore_value());
    fields.insert("access_level".to_string(), config.access_level.as_str().to_firestore_value());
    fields.insert("watermark_enabled".to_string(), config.watermark_enabled.to_firestore_value());
    fields.insert("created_at".to_string(), config.created_at.to_firestore_value());

    if let Some(expires) = config.expires_at {
        fields.insert("expires_at".to_string(), expires.to_firestore_value());
    }
    if let Some(updated) = config.updated_at {
        fields.insert("updated_at".to_string(), updated.to_firestore_value());
    }
    if let Some(disabled) = config.disabled_at {
        fields.insert("disabled_at".to_string(), disabled.to_firestore_value());
    }

    fields
}

fn share_slug_index_to_fields(config: &ShareConfig) -> HashMap<String, Value> {
    let mut fields = HashMap::new();
    fields.insert("share_slug".to_string(), config.share_slug.to_firestore_value());
    fields.insert("user_id".to_string(), config.user_id.to_firestore_value());
    fields.insert("video_id".to_string(), config.video_id.to_firestore_value());
    fields.insert("clip_id".to_string(), config.clip_id.to_firestore_value());
    fields.insert("access_level".to_string(), config.access_level.as_str().to_firestore_value());
    fields.insert("created_at".to_string(), config.created_at.to_firestore_value());

    if let Some(expires) = config.expires_at {
        fields.insert("expires_at".to_string(), expires.to_firestore_value());
    }
    if let Some(disabled) = config.disabled_at {
        fields.insert("disabled_at".to_string(), disabled.to_firestore_value());
    }

    fields
}

fn document_to_share_slug_index(doc: &crate::types::Document) -> FirestoreResult<ShareSlugIndex> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    let get_string = |key: &str| -> String {
        fields.get(key).and_then(|v| String::from_firestore_value(v)).unwrap_or_default()
    };

    Ok(ShareSlugIndex {
        share_slug: get_string("share_slug"),
        user_id: get_string("user_id"),
        video_id: get_string("video_id"),
        clip_id: get_string("clip_id"),
        access_level: ShareAccessLevel::from_str(&get_string("access_level")),
        expires_at: fields.get("expires_at").and_then(|v| chrono::DateTime::from_firestore_value(v)),
        disabled_at: fields.get("disabled_at").and_then(|v| chrono::DateTime::from_firestore_value(v)),
        created_at: fields.get("created_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
    })
}

fn document_to_share_config(doc: &crate::types::Document) -> FirestoreResult<ShareConfig> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    let get_string = |key: &str| -> String {
        fields.get(key).and_then(|v| String::from_firestore_value(v)).unwrap_or_default()
    };

    let get_bool = |key: &str| -> bool {
        fields.get(key).and_then(|v| bool::from_firestore_value(v)).unwrap_or(false)
    };

    Ok(ShareConfig {
        share_slug: get_string("share_slug"),
        clip_id: get_string("clip_id"),
        user_id: get_string("user_id"),
        video_id: get_string("video_id"),
        access_level: ShareAccessLevel::from_str(&get_string("access_level")),
        expires_at: fields.get("expires_at").and_then(|v| chrono::DateTime::from_firestore_value(v)),
        watermark_enabled: get_bool("watermark_enabled"),
        created_at: fields.get("created_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
        updated_at: fields.get("updated_at").and_then(|v| chrono::DateTime::from_firestore_value(v)),
        disabled_at: fields.get("disabled_at").and_then(|v| chrono::DateTime::from_firestore_value(v)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vclip_models::{ClipStatus, VideoId};

    fn sample_clip() -> ClipMetadata {
        ClipMetadata {
            clip_id: "clip-1".to_string(),
            video_id: VideoId::from_string("video-1"),
            user_id: "user-1".to_string(),
            scene_id: 1,
            scene_title: "Scene".to_string(),
            scene_description: None,
            filename: "clip_01_1_scene_intelligent.mp4".to_string(),
            style: "intelligent".to_string(),
            priority: 1,
            start_time: "00:00:00".to_string(),
            end_time: "00:00:10".to_string(),
            duration_seconds: 10.0,
            file_size_bytes: 1_000,
            file_size_mb: 1.0,
            has_thumbnail: true,
            r2_key: "r2/key".to_string(),
            thumbnail_r2_key: Some("r2/thumb".to_string()),
            raw_r2_key: None,
            status: ClipStatus::Completed,
            created_at: Utc::now(),
            completed_at: None,
            updated_at: None,
            created_by: "user-1".to_string(),
        }
    }

    #[test]
    fn clip_fields_include_updated_at_even_when_missing() {
        let clip = sample_clip();
        let fields = clip_metadata_to_fields(&clip);
        assert!(
            fields.contains_key("updated_at"),
            "updated_at should be set for upserts"
        );
    }

    #[test]
    fn clip_fields_include_completed_at_when_present() {
        let mut clip = sample_clip();
        let now = Utc::now();
        clip.completed_at = Some(now);
        let fields = clip_metadata_to_fields(&clip);
        assert!(
            fields.get("completed_at").is_some(),
            "completed_at should be included when provided"
        );
    }
}
