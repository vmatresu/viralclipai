//! Highlights repository for video highlights storage in Firestore.

use std::collections::HashMap;
use chrono::Utc;
use tracing::info;
use vclip_models::{VideoId, highlight::VideoHighlights};
use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{ArrayValue, FromFirestoreValue, MapValue, ToFirestoreValue, Value};

/// Repository for highlights documents.
pub struct HighlightsRepository {
    client: FirestoreClient,
    user_id: String,
}

impl HighlightsRepository {
    /// Create a new highlights repository.
    pub fn new(client: FirestoreClient, user_id: impl Into<String>) -> Self {
        Self {
            client,
            user_id: user_id.into(),
        }
    }

    /// Collection path for a video's highlights.
    fn collection(&self, video_id: &VideoId) -> String {
        format!(
            "users/{}/videos/{}/highlights",
            self.user_id,
            video_id.as_str()
        )
    }

    /// Document ID (always use "main" for highlights).
    fn doc_id() ->  &'static str {
        "main"
    }

    /// Get highlights for a video.
    pub async fn get(&self, video_id: &VideoId) -> FirestoreResult<Option<VideoHighlights>> {
        let doc = self
            .client
            .get_document(&self.collection(video_id), Self::doc_id())
            .await?;

        match doc {
            Some(d) => {
                let highlights = document_to_video_highlights(&d, video_id)?;
                Ok(Some(highlights))
            }
            None => Ok(None),
        }
    }

    /// Create or update highlights for a video (upsert).
    pub async fn upsert(&self, highlights: &VideoHighlights) -> FirestoreResult<()> {
        let video_id = VideoId::from_string(&highlights.video_id);
        let fields = video_highlights_to_fields(highlights);

        // Try to update first, create if doesn't exist
        match self
            .client
            .update_document(
                &self.collection(&video_id),
                Self::doc_id(),
                fields.clone(),
                None, // No update mask = full document update
            )
            .await
        {
            Ok(_) => {
                info!("Updated highlights for video {}", highlights.video_id);
                Ok(())
            }
            Err(FirestoreError::NotFound(_)) => {
                // Document doesn't exist, create it
                self.client
                    .create_document(&self.collection(&video_id), Self::doc_id(), fields)
                    .await?;
                info!("Created highlights for video {}", highlights.video_id);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Delete highlights for a video.
    pub async fn delete(&self, video_id: &VideoId) -> FirestoreResult<bool> {
        self.client
            .delete_document(&self.collection(video_id), Self::doc_id())
            .await?;
        Ok(true)
    }
}

// Helper functions for highlights conversion

fn video_highlights_to_fields(highlights: &VideoHighlights) -> HashMap<String, Value> {
    let mut fields = HashMap::new();
    fields.insert("video_id".to_string(), highlights.video_id.to_firestore_value());
    
    // Serialize highlights array
    let highlights_array: Vec<Value> = highlights
        .highlights
        .iter()
        .map(|h| {
            let mut h_fields = HashMap::new();
            h_fields.insert("id".to_string(), h.id.to_firestore_value());
            h_fields.insert("title".to_string(), h.title.to_firestore_value());
            h_fields.insert("start".to_string(), h.start.to_firestore_value());
            h_fields.insert("end".to_string(), h.end.to_firestore_value());
            h_fields.insert("duration".to_string(), h.duration.to_firestore_value());
            h_fields.insert("pad_before".to_string(), h.pad_before.to_firestore_value());
            h_fields.insert("pad_after".to_string(), h.pad_after.to_firestore_value());
            if let Some(ref category) = h.hook_category {
                // Serialize enum as string
                let category_str = match category {
                    vclip_models::HighlightCategory::Emotional => "emotional",
                    vclip_models::HighlightCategory::Educational => "educational",
                    vclip_models::HighlightCategory::Controversial => "controversial",
                    vclip_models::HighlightCategory::Inspirational => "inspirational",
                    vclip_models::HighlightCategory::Humorous => "humorous",
                    vclip_models::HighlightCategory::Dramatic => "dramatic",
                    vclip_models::HighlightCategory::Surprising => "surprising",
                    vclip_models::HighlightCategory::Other => "other",
                };
                h_fields.insert("hook_category".to_string(), category_str.to_firestore_value());
            }
            if let Some(ref reason) = h.reason {
                h_fields.insert("reason".to_string(), reason.to_firestore_value());
            }
            if let Some(ref description) = h.description {
                h_fields.insert("description".to_string(), description.to_firestore_value());
            }
            Value::MapValue(MapValue { fields: Some(h_fields) })
        })
        .collect();
    fields.insert("highlights".to_string(), Value::ArrayValue(ArrayValue { values: Some(highlights_array) }));

    if let Some(ref url) = highlights.video_url {
        fields.insert("video_url".to_string(), url.to_firestore_value());
    }
    if let Some(ref title) = highlights.video_title {
        fields.insert("video_title".to_string(), title.to_firestore_value());
    }
    if let Some(ref prompt) = highlights.custom_prompt {
        fields.insert("custom_prompt".to_string(), prompt.to_firestore_value());
    }
    
    fields.insert("created_at".to_string(), highlights.created_at.to_firestore_value());
    fields.insert("updated_at".to_string(), highlights.updated_at.to_firestore_value());
    
    fields
}

fn document_to_video_highlights(
    doc: &crate::types::Document,
    video_id: &VideoId,
) -> FirestoreResult<VideoHighlights> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    // Parse highlights array
    let highlights = fields
        .get("highlights")
        .and_then(|v| match v {
            Value::ArrayValue(ArrayValue { values: Some(values) }) => Some(values),
            _ => None,
        })
        .ok_or_else(|| FirestoreError::InvalidResponse("Missing highlights array".to_string()))?
        .iter()
        .filter_map(|v| match v {
            Value::MapValue(MapValue { fields: Some(fields) }) => {
                let id = fields.get("id").and_then(|v| u32::from_firestore_value(v)).unwrap_or(0);
                let title = fields.get("title").and_then(|v| String::from_firestore_value(v)).unwrap_or_default();
                let start = fields.get("start").and_then(|v| String::from_firestore_value(v)).unwrap_or_default();
                let end = fields.get("end").and_then(|v| String::from_firestore_value(v)).unwrap_or_default();
                let duration = fields.get("duration").and_then(|v| u32::from_firestore_value(v)).unwrap_or(0);
                let pad_before = fields.get("pad_before").and_then(|v| f64::from_firestore_value(v)).unwrap_or(1.0);
                let pad_after = fields.get("pad_after").and_then(|v| f64::from_firestore_value(v)).unwrap_or(1.0);
                
                let hook_category = fields.get("hook_category")
                    .and_then(|v| String::from_firestore_value(v))
                    .and_then(|s| match s.as_str() {
                        "emotional" => Some(vclip_models::HighlightCategory::Emotional),
                        "educational" => Some(vclip_models::HighlightCategory::Educational),
                        "controversial" => Some(vclip_models::HighlightCategory::Controversial),
                        "inspirational" => Some(vclip_models::HighlightCategory::Inspirational),
                        "humorous" => Some(vclip_models::HighlightCategory::Humorous),
                        "dramatic" => Some(vclip_models::HighlightCategory::Dramatic),
                        "surprising" => Some(vclip_models::HighlightCategory::Surprising),
                        _ => Some(vclip_models::HighlightCategory::Other),
                    });
                
                let reason = fields.get("reason").and_then(|v| String::from_firestore_value(v));
                let description = fields.get("description").and_then(|v| String::from_firestore_value(v));

                Some(vclip_models::Highlight {
                    id,
                    title,
                    start,
                    end,
                    duration,
                    pad_before,
                    pad_after,
                    hook_category,
                    reason,
                    description,
                })
            }
            _ => None,
        })
        .collect();

    let video_url = fields.get("video_url").and_then(|v| String::from_firestore_value(v));
    let video_title = fields.get("video_title").and_then(|v| String::from_firestore_value(v));
    let custom_prompt = fields.get("custom_prompt").and_then(|v| String::from_firestore_value(v));
    
    let created_at = fields
        .get("created_at")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or_else(Utc::now);
    
    let updated_at = fields
        .get("updated_at")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or_else(Utc::now);

    Ok(VideoHighlights {
        video_id: video_id.as_str().to_string(),
        highlights,
        video_url,
        video_title,
        custom_prompt,
        created_at,
        updated_at,
    })
}
