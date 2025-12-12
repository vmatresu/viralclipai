//! High-level storage operations.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::client::R2Client;
use crate::error::StorageResult;

/// Highlights data stored in R2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightsData {
    /// List of highlights
    pub highlights: Vec<HighlightEntry>,
    /// Video URL
    pub video_url: Option<String>,
    /// Video title
    pub video_title: Option<String>,
    /// Custom prompt used
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightEntry {
    pub id: u32,
    pub title: String,
    pub description: Option<String>,
    pub start: String,
    pub end: String,
    pub duration: u32,
    /// Padding before the start timestamp (seconds)
    #[serde(default = "default_pad_before")]
    pub pad_before_seconds: f64,
    /// Padding after the end timestamp (seconds)
    #[serde(default = "default_pad_after")]
    pub pad_after_seconds: f64,
    pub hook_category: Option<String>,
    pub reason: Option<String>,
}

fn default_pad_before() -> f64 {
    1.0
}

fn default_pad_after() -> f64 {
    1.0
}

impl R2Client {
    /// Upload a video clip file.
    pub async fn upload_clip(
        &self,
        path: impl AsRef<Path>,
        user_id: &str,
        video_id: &str,
        filename: &str,
    ) -> StorageResult<String> {
        let key = format!("{}/{}/clips/{}", user_id, video_id, filename);
        let content_type = if filename.ends_with(".mp4") {
            "video/mp4"
        } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
            "image/jpeg"
        } else {
            "application/octet-stream"
        };

        self.upload_file(path, &key, content_type).await?;
        Ok(key)
    }

    /// Upload highlights JSON.
    pub async fn upload_highlights(
        &self,
        user_id: &str,
        video_id: &str,
        data: &HighlightsData,
    ) -> StorageResult<String> {
        let key = format!("{}/{}/highlights.json", user_id, video_id);
        let json = serde_json::to_vec(data)?;
        self.upload_bytes(json, &key, "application/json").await?;
        Ok(key)
    }

    /// Load highlights JSON.
    pub async fn load_highlights(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> StorageResult<HighlightsData> {
        let key = format!("{}/{}/highlights.json", user_id, video_id);
        let bytes = self.download_bytes(&key).await?;
        let data: HighlightsData = serde_json::from_slice(&bytes)?;
        Ok(data)
    }

    /// Delete all files for a video, including cached data.
    ///
    /// This deletes:
    /// - Styled clips: `{user_id}/{video_id}/clips/*.mp4`
    /// - Neural cache: `{user_id}/{video_id}/neural/*.json.gz`
    /// - Raw segments: `clips/{user_id}/{video_id}/raw/*.mp4`
    /// - Source videos: `sources/{user_id}/{video_id}/source.mp4`
    pub async fn delete_video_files(&self, user_id: &str, video_id: &str) -> StorageResult<u32> {
        let mut all_keys: Vec<String> = Vec::new();

        // 1. Styled clips and neural cache: {user_id}/{video_id}/
        let main_prefix = format!("{}/{}/", user_id, video_id);
        let main_objects = self.list_objects(&main_prefix).await?;
        all_keys.extend(main_objects.into_iter().map(|o| o.key));

        // 2. Raw segments: clips/{user_id}/{video_id}/raw/
        let raw_prefix = format!("clips/{}/{}/raw/", user_id, video_id);
        let raw_objects = self.list_objects(&raw_prefix).await?;
        all_keys.extend(raw_objects.into_iter().map(|o| o.key));

        // 3. Source video: sources/{user_id}/{video_id}/
        let source_prefix = format!("sources/{}/{}/", user_id, video_id);
        let source_objects = self.list_objects(&source_prefix).await?;
        all_keys.extend(source_objects.into_iter().map(|o| o.key));

        if all_keys.is_empty() {
            info!("No files found to delete for video {}/{}", user_id, video_id);
            return Ok(0);
        }

        info!(
            "Deleting {} files for video {}/{} (clips, neural cache, raw segments, source)",
            all_keys.len(),
            user_id,
            video_id
        );

        self.delete_objects(&all_keys).await
    }

    /// Delete a single clip and its thumbnail.
    pub async fn delete_clip(
        &self,
        user_id: &str,
        video_id: &str,
        clip_name: &str,
    ) -> StorageResult<u32> {
        let clip_key = format!("{}/{}/clips/{}", user_id, video_id, clip_name);
        let thumb_key = clip_key.replace(".mp4", ".jpg");

        let keys = vec![clip_key, thumb_key];
        self.delete_objects(&keys).await
    }
}
