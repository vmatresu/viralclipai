//! Analysis draft repository for video analysis workflow in Firestore.

use std::collections::HashMap;

use chrono::Utc;
use tracing::info;

use vclip_models::{AnalysisDraft, AnalysisStatus, DraftScene};

use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{FromFirestoreValue, ToFirestoreValue, Value};

/// Repository for analysis draft documents.
pub struct AnalysisDraftRepository {
    client: FirestoreClient,
    user_id: String,
}

impl AnalysisDraftRepository {
    /// Create a new analysis draft repository.
    pub fn new(client: FirestoreClient, user_id: impl Into<String>) -> Self {
        Self {
            client,
            user_id: user_id.into(),
        }
    }

    /// Collection path for user's analysis drafts.
    fn collection(&self) -> String {
        format!("users/{}/analysis_drafts", self.user_id)
    }

    /// Subcollection path for scenes within a draft.
    fn scenes_collection(&self, draft_id: &str) -> String {
        format!("users/{}/analysis_drafts/{}/scenes", self.user_id, draft_id)
    }

    /// Create a new analysis draft.
    pub async fn create(&self, draft: &AnalysisDraft) -> FirestoreResult<()> {
        let fields = draft_to_fields(draft);
        self.client
            .create_document(&self.collection(), &draft.id, fields)
            .await?;
        info!(
            "Created analysis draft {} for user {}",
            draft.id, self.user_id
        );
        Ok(())
    }

    /// Get an analysis draft by ID.
    pub async fn get(&self, draft_id: &str) -> FirestoreResult<Option<AnalysisDraft>> {
        let doc = self.client.get_document(&self.collection(), draft_id).await?;

        match doc {
            Some(d) => {
                let draft = document_to_draft(&d, draft_id)?;
                Ok(Some(draft))
            }
            None => Ok(None),
        }
    }

    /// Update draft status.
    pub async fn update_status(
        &self,
        draft_id: &str,
        status: AnalysisStatus,
        error_message: Option<String>,
    ) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), status.as_str().to_firestore_value());
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        if let Some(msg) = error_message {
            fields.insert("error_message".to_string(), msg.to_firestore_value());
        }

        self.client
            .update_document(
                &self.collection(),
                draft_id,
                fields,
                Some(vec![
                    "status".to_string(),
                    "updated_at".to_string(),
                    "error_message".to_string(),
                ]),
            )
            .await?;

        info!(
            "Updated analysis draft {} status to {:?}",
            draft_id,
            status.as_str()
        );
        Ok(())
    }

    /// Update draft with video title and scene count.
    pub async fn update_completion(
        &self,
        draft_id: &str,
        video_title: Option<String>,
        scene_count: u32,
        warning_count: u32,
    ) -> FirestoreResult<()> {
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            AnalysisStatus::Completed.as_str().to_firestore_value(),
        );
        fields.insert("scene_count".to_string(), scene_count.to_firestore_value());
        fields.insert(
            "warning_count".to_string(),
            warning_count.to_firestore_value(),
        );
        fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());

        let mut update_mask = vec![
            "status".to_string(),
            "scene_count".to_string(),
            "warning_count".to_string(),
            "updated_at".to_string(),
        ];

        if let Some(title) = video_title {
            fields.insert("video_title".to_string(), title.to_firestore_value());
            update_mask.push("video_title".to_string());
        }

        self.client
            .update_document(&self.collection(), draft_id, fields, Some(update_mask))
            .await?;

        info!(
            "Completed analysis draft {} with {} scenes",
            draft_id, scene_count
        );
        Ok(())
    }

    /// List all analysis drafts for the user.
    pub async fn list(&self, limit: Option<u32>) -> FirestoreResult<Vec<AnalysisDraft>> {
        let response = self.client.list_documents(&self.collection(), limit, None).await?;

        let drafts: Vec<AnalysisDraft> = response
            .documents
            .unwrap_or_default()
            .iter()
            .filter_map(|doc| {
                let name = doc.name.as_ref()?;
                let draft_id = name.split('/').last()?;
                document_to_draft(doc, draft_id).ok()
            })
            .collect();

        Ok(drafts)
    }

    /// Delete an analysis draft and its scenes.
    pub async fn delete(&self, draft_id: &str) -> FirestoreResult<bool> {
        // First delete all scenes
        let scenes = self.get_scenes(draft_id).await?;
        for scene in scenes {
            self.client
                .delete_document(&self.scenes_collection(draft_id), &scene.id.to_string())
                .await?;
        }

        // Then delete the draft
        self.client
            .delete_document(&self.collection(), draft_id)
            .await?;

        info!("Deleted analysis draft {} and its scenes", draft_id);
        Ok(true)
    }

    /// Upsert scenes for a draft.
    pub async fn upsert_scenes(&self, draft_id: &str, scenes: &[DraftScene]) -> FirestoreResult<()> {
        for scene in scenes {
            let fields = scene_to_fields(scene);
            let scene_id = scene.id.to_string();

            // Try update first, create if not exists
            match self
                .client
                .update_document(&self.scenes_collection(draft_id), &scene_id, fields.clone(), None)
                .await
            {
                Ok(_) => {}
                Err(FirestoreError::NotFound(_)) => {
                    self.client
                        .create_document(&self.scenes_collection(draft_id), &scene_id, fields)
                        .await?;
                }
                Err(e) => return Err(e),
            }
        }

        info!("Upserted {} scenes for draft {}", scenes.len(), draft_id);
        Ok(())
    }

    /// Get all scenes for a draft.
    pub async fn get_scenes(&self, draft_id: &str) -> FirestoreResult<Vec<DraftScene>> {
        let response = self
            .client
            .list_documents(&self.scenes_collection(draft_id), None, None)
            .await?;

        let scenes: Vec<DraftScene> = response
            .documents
            .unwrap_or_default()
            .iter()
            .filter_map(|doc| document_to_scene(doc, draft_id).ok())
            .collect();

        Ok(scenes)
    }

    /// Delete expired drafts for this user.
    /// Returns the number of drafts deleted.
    pub async fn delete_expired(&self) -> FirestoreResult<u32> {
        let drafts = self.list(None).await?;
        let now = Utc::now();
        let mut deleted = 0;

        for draft in drafts {
            if draft.expires_at < now {
                self.delete(&draft.id).await?;
                deleted += 1;
            }
        }

        if deleted > 0 {
            info!("Deleted {} expired drafts for user {}", deleted, self.user_id);
        }

        Ok(deleted)
    }
}

// Helper functions for conversion

fn draft_to_fields(draft: &AnalysisDraft) -> HashMap<String, Value> {
    let mut fields = HashMap::new();

    fields.insert("id".to_string(), draft.id.to_firestore_value());
    fields.insert("user_id".to_string(), draft.user_id.to_firestore_value());
    fields.insert("source_url".to_string(), draft.source_url.to_firestore_value());
    fields.insert("status".to_string(), draft.status.as_str().to_firestore_value());
    fields.insert("scene_count".to_string(), draft.scene_count.to_firestore_value());
    fields.insert(
        "warning_count".to_string(),
        draft.warning_count.to_firestore_value(),
    );
    fields.insert("created_at".to_string(), draft.created_at.to_firestore_value());
    fields.insert("updated_at".to_string(), draft.updated_at.to_firestore_value());
    fields.insert("expires_at".to_string(), draft.expires_at.to_firestore_value());

    if let Some(ref title) = draft.video_title {
        fields.insert("video_title".to_string(), title.to_firestore_value());
    }
    if let Some(ref prompt) = draft.prompt_instructions {
        fields.insert("prompt_instructions".to_string(), prompt.to_firestore_value());
    }
    if let Some(ref error) = draft.error_message {
        fields.insert("error_message".to_string(), error.to_firestore_value());
    }
    if let Some(ref request_id) = draft.request_id {
        fields.insert("request_id".to_string(), request_id.to_firestore_value());
    }

    fields
}

fn document_to_draft(doc: &crate::types::Document, draft_id: &str) -> FirestoreResult<AnalysisDraft> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Document has no fields".to_string())
    })?;

    let user_id = fields
        .get("user_id")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let source_url = fields
        .get("source_url")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let status_str = fields
        .get("status")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_else(|| "pending".to_string());

    let status = match status_str.as_str() {
        "pending" => AnalysisStatus::Pending,
        "downloading" => AnalysisStatus::Downloading,
        "analyzing" => AnalysisStatus::Analyzing,
        "completed" => AnalysisStatus::Completed,
        "failed" => AnalysisStatus::Failed,
        "expired" => AnalysisStatus::Expired,
        _ => AnalysisStatus::Pending,
    };

    let video_title = fields
        .get("video_title")
        .and_then(|v| String::from_firestore_value(v));

    let prompt_instructions = fields
        .get("prompt_instructions")
        .and_then(|v| String::from_firestore_value(v));

    let error_message = fields
        .get("error_message")
        .and_then(|v| String::from_firestore_value(v));

    let request_id = fields
        .get("request_id")
        .and_then(|v| String::from_firestore_value(v));

    let scene_count = fields
        .get("scene_count")
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or(0);

    let warning_count = fields
        .get("warning_count")
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or(0);

    let created_at = fields
        .get("created_at")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or_else(Utc::now);

    let updated_at = fields
        .get("updated_at")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or_else(Utc::now);

    let expires_at = fields
        .get("expires_at")
        .and_then(|v| chrono::DateTime::from_firestore_value(v))
        .unwrap_or_else(|| Utc::now() + chrono::Duration::days(7));

    Ok(AnalysisDraft {
        id: draft_id.to_string(),
        user_id,
        source_url,
        video_title,
        prompt_instructions,
        status,
        error_message,
        request_id,
        scene_count,
        warning_count,
        created_at,
        updated_at,
        expires_at,
    })
}

fn scene_to_fields(scene: &DraftScene) -> HashMap<String, Value> {
    let mut fields = HashMap::new();

    fields.insert("id".to_string(), scene.id.to_firestore_value());
    fields.insert(
        "analysis_draft_id".to_string(),
        scene.analysis_draft_id.to_firestore_value(),
    );
    fields.insert("title".to_string(), scene.title.to_firestore_value());
    fields.insert("start".to_string(), scene.start.to_firestore_value());
    fields.insert("end".to_string(), scene.end.to_firestore_value());
    fields.insert(
        "duration_secs".to_string(),
        scene.duration_secs.to_firestore_value(),
    );
    fields.insert("pad_before".to_string(), scene.pad_before.to_firestore_value());
    fields.insert("pad_after".to_string(), scene.pad_after.to_firestore_value());

    if let Some(ref desc) = scene.description {
        fields.insert("description".to_string(), desc.to_firestore_value());
    }
    if let Some(ref reason) = scene.reason {
        fields.insert("reason".to_string(), reason.to_firestore_value());
    }
    if let Some(confidence) = scene.confidence {
        fields.insert("confidence".to_string(), confidence.to_firestore_value());
    }
    if let Some(ref category) = scene.hook_category {
        fields.insert("hook_category".to_string(), category.to_firestore_value());
    }

    fields
}

fn document_to_scene(doc: &crate::types::Document, draft_id: &str) -> FirestoreResult<DraftScene> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        FirestoreError::InvalidResponse("Scene document has no fields".to_string())
    })?;

    let id = fields
        .get("id")
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or(0);

    let title = fields
        .get("title")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let start = fields
        .get("start")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let end = fields
        .get("end")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_default();

    let duration_secs = fields
        .get("duration_secs")
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or(0);

    let pad_before = fields
        .get("pad_before")
        .and_then(|v| f64::from_firestore_value(v))
        .unwrap_or(1.0);

    let pad_after = fields
        .get("pad_after")
        .and_then(|v| f64::from_firestore_value(v))
        .unwrap_or(1.0);

    let description = fields
        .get("description")
        .and_then(|v| String::from_firestore_value(v));

    let reason = fields
        .get("reason")
        .and_then(|v| String::from_firestore_value(v));

    let confidence = fields
        .get("confidence")
        .and_then(|v| f64::from_firestore_value(v));

    let hook_category = fields
        .get("hook_category")
        .and_then(|v| String::from_firestore_value(v));

    Ok(DraftScene {
        id,
        analysis_draft_id: draft_id.to_string(),
        title,
        description,
        reason,
        start,
        end,
        duration_secs,
        pad_before,
        pad_after,
        confidence,
        hook_category,
    })
}
