//! Typed repositories for Videos and Clips.

use std::collections::HashMap;

use chrono::Utc;
use metrics::counter;
use tracing::info;

use vclip_models::{ClipMetadata, ClipStatus, VideoId, VideoMetadata, VideoStatus};

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
                counter!("clip_metadata_upsert_total", "outcome" => "updated");
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
    fields.insert("clips_count".to_string(), video.clips_count.to_firestore_value());
    fields.insert("highlights_json_key".to_string(), video.highlights_json_key.to_firestore_value());
    fields.insert("created_by".to_string(), video.created_by.to_firestore_value());
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

    Ok(VideoMetadata {
        video_id: video_id.clone(),
        user_id: get_string("user_id"),
        video_url: get_string("video_url"),
        video_title: get_string("video_title"),
        youtube_id: get_string("youtube_id"),
        status: match get_string("status").as_str() {
            "completed" => VideoStatus::Completed,
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
        styles_processed: Vec::new(), // TODO: parse array
        crop_mode: get_string("crop_mode"),
        target_aspect: get_string("target_aspect"),
        clips_count: get_u32("clips_count"),
        clips_by_style: HashMap::new(), // TODO: parse map
        highlights_json_key: get_string("highlights_json_key"),
        created_by: get_string("created_by"),
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
