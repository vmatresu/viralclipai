//! High-level storage operations.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::client::R2Client;
use crate::error::StorageResult;

/// Information about a clip file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipInfo {
    /// Filename
    pub name: String,
    /// Scene title
    pub title: String,
    /// Description
    pub description: String,
    /// API URL for streaming
    pub url: String,
    /// Direct presigned URL
    pub direct_url: Option<String>,
    /// Thumbnail URL
    pub thumbnail: Option<String>,
    /// File size display string
    pub size: String,
    /// Style
    pub style: Option<String>,
}

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

    /// List clips for a video with metadata.
    pub async fn list_clips_with_metadata(
        &self,
        user_id: &str,
        video_id: &str,
        highlights_map: &HashMap<u32, (String, String)>, // id -> (title, description)
        url_expiry: Duration,
    ) -> StorageResult<Vec<ClipInfo>> {
        let prefix = format!("{}/{}/clips/", user_id, video_id);
        let objects = self.list_objects(&prefix).await?;

        // Build a set of all keys for thumbnail lookup
        let keys_set: std::collections::HashSet<_> =
            objects.iter().map(|o| o.key.clone()).collect();

        let mut clips = Vec::new();

        for obj in &objects {
            // Only process .mp4 files
            if !obj.key.to_lowercase().ends_with(".mp4") {
                continue;
            }

            let filename = obj.key.split('/').last().unwrap_or(&obj.key).to_string();
            let size_mb = obj.size as f64 / (1024.0 * 1024.0);

            // Check for thumbnail - prefer CDN URL if available
            let thumb_key = obj.key.replace(".mp4", ".jpg");
            let thumb_url = if keys_set.contains(&thumb_key) {
                // Use CDN URL if configured, otherwise presigned
                self.get_url(&thumb_key, url_expiry).await.ok()
            } else {
                None
            };

            // Generate direct URL - prefer CDN URL if available
            let direct_url = self.get_url(&obj.key, url_expiry).await.ok();

            // Extract style from filename
            let style = extract_style_from_filename(&filename);

            // Extract title and description from highlights map
            let (title, description) = extract_metadata_from_filename(&filename, highlights_map);

            clips.push(ClipInfo {
                name: filename.clone(),
                title,
                description,
                url: format!("/api/videos/{}/clips/{}", video_id, filename),
                direct_url,
                thumbnail: thumb_url,
                size: format!("{:.1} MB", size_mb),
                style,
            });
        }

        clips.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(clips)
    }

    /// Delete all files for a video.
    pub async fn delete_video_files(&self, user_id: &str, video_id: &str) -> StorageResult<u32> {
        let prefix = format!("{}/{}/", user_id, video_id);
        let objects = self.list_objects(&prefix).await?;

        if objects.is_empty() {
            info!("No files found to delete for video {}/{}", user_id, video_id);
            return Ok(0);
        }

        let keys: Vec<_> = objects.into_iter().map(|o| o.key).collect();
        self.delete_objects(&keys).await
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

/// Extract style from filename (e.g., "clip_01_1_title_split.mp4" -> "split").
fn extract_style_from_filename(filename: &str) -> Option<String> {
    if !filename.ends_with(".mp4") {
        return None;
    }

    let name_without_ext = &filename[..filename.len() - 4];

    // Known styles ordered by length (longest first)
    let known_styles = [
        "intelligent_split",
        "left_focus",
        "right_focus",
        "intelligent",
        "split",
        "original",
    ];

    for style in known_styles {
        if name_without_ext.ends_with(&format!("_{}", style)) {
            return Some(style.to_string());
        }
    }

    None
}

/// Extract title and description from filename using highlights map.
fn extract_metadata_from_filename(
    filename: &str,
    highlights_map: &HashMap<u32, (String, String)>,
) -> (String, String) {
    // Filename format: clip_{priority:02d}_{scene_id}_{safe_title}_{style}.mp4
    let parts: Vec<&str> = filename.split('_').collect();

    if parts.len() >= 2 && parts[0] == "clip" {
        if let Ok(priority) = parts[1].parse::<u32>() {
            if let Some((title, description)) = highlights_map.get(&priority) {
                return (title.clone(), description.clone());
            }
        }
    }

    (filename.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_style() {
        assert_eq!(
            extract_style_from_filename("clip_01_1_title_split.mp4"),
            Some("split".to_string())
        );
        assert_eq!(
            extract_style_from_filename("clip_01_1_title_intelligent_split.mp4"),
            Some("intelligent_split".to_string())
        );
        assert_eq!(
            extract_style_from_filename("clip_01_1_title_original.mp4"),
            Some("original".to_string())
        );
        assert_eq!(extract_style_from_filename("random.mp4"), None);
    }

    #[test]
    fn test_extract_metadata() {
        let mut map = HashMap::new();
        map.insert(1, ("Test Title".to_string(), "Test Description".to_string()));

        let (title, desc) = extract_metadata_from_filename("clip_01_1_title_split.mp4", &map);
        assert_eq!(title, "Test Title");
        assert_eq!(desc, "Test Description");
    }
}
