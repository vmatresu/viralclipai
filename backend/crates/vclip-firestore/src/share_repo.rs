//! Share repository for managing clip share links.
//!
//! Uses a dual-document pattern:
//! - Config doc at `users/{uid}/videos/{vid}/clips/{cid}/shares/config`
//! - Slug index at `share_slugs/{slug}` for fast public lookups

use std::collections::HashMap;

use chrono::Utc;
use tracing::info;

use vclip_models::share::{ShareAccessLevel, ShareConfig};

use crate::client::FirestoreClient;
use crate::error::{FirestoreError, FirestoreResult};
use crate::types::{Document, DocumentMask, FromFirestoreValue, Precondition, ToFirestoreValue, Value, Write};

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

        let config_collection = Self::config_path(&config.user_id, &config.video_id, &config.clip_id);
        let config_doc_name = self.client.full_document_name(&config_collection, "config");
        let slug_doc_name = self.client.full_document_name(Self::slug_collection(), &config.share_slug);

        let writes = vec![
            Write {
                update: Some(Document {
                    name: Some(config_doc_name),
                    fields: Some(config_fields),
                    create_time: None,
                    update_time: None,
                }),
                delete: None,
                update_mask: None,
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

        self.client.batch_write(writes).await?;

        info!(
            "Created share (atomic): slug={}, clip_id={}, user_id={}",
            config.share_slug, config.clip_id, config.user_id
        );

        Ok(())
    }

    /// Disable a share atomically (update config and delete slug index).
    pub async fn disable_share(
        &self,
        user_id: &str,
        video_id: &str,
        clip_id: &str,
        share_slug: &str,
    ) -> FirestoreResult<()> {
        let now = Utc::now();

        let mut config_fields = HashMap::new();
        config_fields.insert("disabled_at".to_string(), now.to_firestore_value());
        config_fields.insert("access_level".to_string(), ShareAccessLevel::None.as_str().to_firestore_value());
        config_fields.insert("updated_at".to_string(), now.to_firestore_value());

        let config_collection = Self::config_path(user_id, video_id, clip_id);
        let config_doc_name = self.client.full_document_name(&config_collection, "config");
        let slug_doc_name = self.client.full_document_name(Self::slug_collection(), share_slug);

        let writes = vec![
            Write {
                update: Some(Document {
                    name: Some(config_doc_name),
                    fields: Some(config_fields),
                    create_time: None,
                    update_time: None,
                }),
                delete: None,
                update_mask: Some(DocumentMask {
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
                current_document: None,
            },
        ];

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

    /// Delete a share slug index document.
    pub async fn delete_slug(&self, share_slug: &str) -> FirestoreResult<()> {
        self.client
            .delete_document(Self::slug_collection(), share_slug)
            .await?;
        info!("Deleted share slug: {}", share_slug);
        Ok(())
    }

    /// Delete all share slugs for a video.
    /// 
    /// Called when a video is deleted to clean up all share links.
    pub async fn delete_slugs_for_video(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> FirestoreResult<u32> {
        let response = self.client
            .list_documents(Self::slug_collection(), None, None)
            .await?;

        let mut deleted_count = 0u32;
        
        if let Some(docs) = response.documents {
            for doc in docs {
                if let Ok(index) = document_to_share_slug_index(&doc) {
                    if index.user_id == user_id && index.video_id == video_id {
                        if let Err(e) = self.client
                            .delete_document(Self::slug_collection(), &index.share_slug)
                            .await
                        {
                            tracing::warn!(
                                "Failed to delete share slug {} for video {}: {}",
                                index.share_slug, video_id, e
                            );
                        } else {
                            deleted_count += 1;
                        }
                    }
                }
            }
        }

        if deleted_count > 0 {
            info!(
                "Deleted {} share slugs for video {}/{}",
                deleted_count, user_id, video_id
            );
        }

        Ok(deleted_count)
    }

    /// Delete a share slug for a specific clip.
    pub async fn delete_slug_for_clip(
        &self,
        user_id: &str,
        video_id: &str,
        clip_id: &str,
    ) -> FirestoreResult<bool> {
        let config = self.get_config(user_id, video_id, clip_id).await?;
        
        if let Some(cfg) = config {
            self.client
                .delete_document(Self::slug_collection(), &cfg.share_slug)
                .await?;
            info!(
                "Deleted share slug {} for clip {}/{}",
                cfg.share_slug, video_id, clip_id
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// ============================================================================
// Field Conversion Helpers
// ============================================================================

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

fn document_to_share_slug_index(doc: &Document) -> FirestoreResult<ShareSlugIndex> {
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

fn document_to_share_config(doc: &Document) -> FirestoreResult<ShareConfig> {
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
