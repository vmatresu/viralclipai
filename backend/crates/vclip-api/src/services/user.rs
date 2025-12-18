//! User and SaaS service for plan limits, ownership checks, and settings.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use vclip_firestore::{FirestoreClient, FromFirestoreValue, ToFirestoreValue, Value};
use vclip_storage::R2Client;
use vclip_models::{VideoId, VideoStatus};

use crate::error::{ApiError, ApiResult};

/// User settings stored in Firestore.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserSettings {
    #[serde(default)]
    pub default_styles: Vec<String>,
    #[serde(default)]
    pub default_crop_mode: Option<String>,
    #[serde(default)]
    pub default_target_aspect: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// User record in Firestore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub uid: String,
    pub email: Option<String>,
    pub plan: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub settings: UserSettings,
    #[serde(default)]
    pub role: Option<String>,
    /// Credits used this billing month.
    #[serde(default)]
    pub credits_used_this_month: u32,
    #[serde(default)]
    pub usage_reset_month: Option<String>,
    /// Total storage used in bytes across all videos/clips.
    #[serde(default)]
    pub total_storage_bytes: u64,
    /// Total number of clips across all videos.
    #[serde(default)]
    pub total_clips_count: u32,
}

impl Default for UserRecord {
    fn default() -> Self {
        Self {
            uid: String::new(),
            email: None,
            plan: "free".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            settings: UserSettings::default(),
            role: None,
            credits_used_this_month: 0,
            usage_reset_month: None,
            total_storage_bytes: 0,
            total_clips_count: 0,
        }
    }
}

/// Plan limits configuration.
///
/// This mirrors `vclip_models::PlanLimits` but is kept separate for API layer concerns.
#[derive(Debug, Clone)]
pub struct PlanLimits {
    pub plan_id: String,
    /// Monthly credits included in plan.
    pub monthly_credits_included: u32,
    /// Maximum clip length in seconds (90s for all plans).
    pub max_clip_length_seconds: u32,
    pub max_highlights_per_video: u32,
    pub max_styles_per_video: u32,
    pub can_reprocess: bool,
    /// Storage limit in bytes.
    pub storage_limit_bytes: u64,
    /// Whether exports include watermark.
    pub watermark_exports: bool,
    /// Whether API access is enabled.
    pub api_access: bool,
    /// Number of monitored channels included.
    pub channel_monitoring_included: u32,
    /// Maximum connected social accounts.
    pub connected_social_accounts_limit: u32,
    /// Whether priority processing is enabled.
    pub priority_processing: bool,
    /// Plan tier for feature checks.
    pub tier: vclip_models::PlanTier,
}

/// Result of unified quota check.
#[derive(Debug, Clone)]
pub struct QuotaCheckResult {
    /// Credits used this month.
    pub credits_used: u32,
    /// Monthly credit limit.
    pub credits_limit: u32,
    /// Storage used in bytes.
    pub storage_used_bytes: u64,
    /// Storage limit in bytes.
    pub storage_limit_bytes: u64,
    /// Storage usage percentage (0-100).
    pub storage_percentage: f64,
    /// User's plan ID.
    pub plan_id: String,
}

impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            plan_id: "free".to_string(),
            monthly_credits_included: vclip_models::FREE_MONTHLY_CREDITS,
            max_clip_length_seconds: vclip_models::MAX_CLIP_LENGTH_SECONDS,
            max_highlights_per_video: 3,
            max_styles_per_video: 2,
            can_reprocess: false,
            storage_limit_bytes: vclip_models::FREE_STORAGE_LIMIT_BYTES,
            watermark_exports: true,
            api_access: false,
            channel_monitoring_included: 0,
            connected_social_accounts_limit: 1,
            priority_processing: false,
            tier: vclip_models::PlanTier::Free,
        }
    }
}

impl PlanLimits {
    /// Check if a detection tier is allowed on this plan.
    pub fn allows_detection_tier(&self, tier: vclip_models::DetectionTier) -> bool {
        self.tier.allows_detection_tier(tier)
    }
}

/// User service for SaaS operations.
#[derive(Clone)]
pub struct UserService {
    firestore: Arc<FirestoreClient>,
    storage: Arc<R2Client>,
}

impl UserService {
    /// Create a new user service.
    pub fn new(firestore: Arc<FirestoreClient>, storage: Arc<R2Client>) -> Self {
        Self { firestore, storage }
    }

    /// Get or create a user record.
    pub async fn get_or_create_user(
        &self,
        uid: &str,
        email: Option<&str>,
    ) -> ApiResult<UserRecord> {
        // Try to get existing user
        match self.get_user(uid).await {
            Ok(Some(mut user)) => {
                // Update email if changed
                if email.is_some() && user.email != email.map(|s| s.to_string()) {
                    user.email = email.map(|s| s.to_string());
                    user.updated_at = Utc::now();
                    // Update in Firestore (fire and forget)
                    let _ = self.update_user(&user).await;
                }
                Ok(user)
            }
            Ok(None) => {
                // Create new user
                let user = UserRecord {
                    uid: uid.to_string(),
                    email: email.map(|s| s.to_string()),
                    plan: "free".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    settings: UserSettings::default(),
                    role: None,
                    credits_used_this_month: 0,
                    usage_reset_month: Some(current_month_key()),
                    total_storage_bytes: 0,
                    total_clips_count: 0,
                };
                self.create_user(&user).await?;
                info!("Created new user: {}", uid);
                Ok(user)
            }
            Err(e) => {
                warn!("Error getting user {}: {}", uid, e);
                Err(e)
            }
        }
    }

    /// Get user by ID.
    async fn get_user(&self, uid: &str) -> ApiResult<Option<UserRecord>> {
        let doc = self
            .firestore
            .get_document("users", uid)
            .await
            .map_err(|e| ApiError::internal(format!("Firestore error: {}", e)))?;

        match doc {
            Some(d) => {
                let user = parse_user_document(&d)?;
                Ok(Some(user))
            }
            None => Ok(None),
        }
    }

    /// Create a new user.
    async fn create_user(&self, user: &UserRecord) -> ApiResult<()> {
        let fields = user_to_fields(user);
        self.firestore
            .create_document("users", &user.uid, fields)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create user: {}", e)))?;
        Ok(())
    }

    /// Update user record.
    async fn update_user(&self, user: &UserRecord) -> ApiResult<()> {
        let fields = user_to_fields(user);
        self.firestore
            .update_document("users", &user.uid, fields, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update user: {}", e)))?;
        Ok(())
    }

    /// Get user settings.
    pub async fn get_user_settings(&self, uid: &str) -> ApiResult<UserSettings> {
        match self.get_user(uid).await? {
            Some(user) => Ok(user.settings),
            None => Ok(UserSettings::default()),
        }
    }

    /// Update user settings.
    pub async fn update_user_settings(
        &self,
        uid: &str,
        settings: HashMap<String, serde_json::Value>,
    ) -> ApiResult<HashMap<String, serde_json::Value>> {
        let mut user = self.get_or_create_user(uid, None).await?;
        
        // Merge settings
        for (key, value) in settings {
            user.settings.extra.insert(key, value);
        }
        user.updated_at = Utc::now();
        
        self.update_user(&user).await?;
        
        // Return merged settings as HashMap
        let mut result = user.settings.extra;
        if !user.settings.default_styles.is_empty() {
            result.insert(
                "default_styles".to_string(),
                serde_json::json!(user.settings.default_styles),
            );
        }
        if let Some(mode) = user.settings.default_crop_mode {
            result.insert("default_crop_mode".to_string(), serde_json::json!(mode));
        }
        if let Some(aspect) = user.settings.default_target_aspect {
            result.insert("default_target_aspect".to_string(), serde_json::json!(aspect));
        }
        
        Ok(result)
    }

    /// Get plan limits for a user.
    pub async fn get_plan_limits(&self, uid: &str) -> ApiResult<PlanLimits> {
        let user = self.get_or_create_user(uid, None).await?;
        
        // Get plan document from Firestore
        match self.firestore.get_document("plans", &user.plan).await {
            Ok(Some(plan_doc)) => {
                // Parse plan document
                let plan_limits = parse_plan_limits(&plan_doc)?;
                Ok(plan_limits)
            }
            Ok(None) => {
                // Plan not found, fallback to free plan
                warn!("Plan '{}' not found for user {}, falling back to free", user.plan, uid);
                match self.firestore.get_document("plans", "free").await {
                    Ok(Some(free_doc)) => {
                        parse_plan_limits(&free_doc)
                    }
                    _ => {
                        // Ultimate fallback to hardcoded free plan
                        warn!("Free plan not found in database, using hardcoded fallback");
                        Ok(PlanLimits::default())
                    }
                }
            }
            Err(e) => {
                warn!("Error fetching plan '{}' for user {}: {}, falling back to free", user.plan, uid, e);
                // Fallback to free plan on error
                match self.firestore.get_document("plans", "free").await {
                    Ok(Some(free_doc)) => {
                        parse_plan_limits(&free_doc)
                    }
                    _ => {
                        // Ultimate fallback to hardcoded free plan
                        Ok(PlanLimits::default())
                    }
                }
            }
        }
    }

    /// Get monthly credits usage for a user.
    pub async fn get_credits_usage(&self, uid: &str) -> ApiResult<u32> {
        let user = self.get_or_create_user(uid, None).await?;

        // Check if we need to reset the counter
        let current_month = current_month_key();
        if user.usage_reset_month.as_deref() != Some(&current_month) {
            // Reset counter for new month
            return Ok(0);
        }

        Ok(user.credits_used_this_month)
    }



    /// Increment credits usage.
    ///
    /// This is the primary method for tracking usage. Credits are NOT refunded on deletion.
    pub async fn increment_credits(&self, uid: &str, credits: u32) -> ApiResult<()> {
        let mut user = self.get_or_create_user(uid, None).await?;

        let current_month = current_month_key();
        if user.usage_reset_month.as_deref() != Some(&current_month) {
            // Reset for new month
            user.credits_used_this_month = credits;
            user.usage_reset_month = Some(current_month);
        } else {
            user.credits_used_this_month += credits;
        }
        user.updated_at = Utc::now();

        self.update_user(&user).await?;
        info!("Charged {} credits to user {}, total now: {}", credits, uid, user.credits_used_this_month);
        Ok(())
    }



    /// Maximum retries for atomic credit reservation.
    const MAX_CREDIT_RESERVE_RETRIES: u32 = 5;

    /// Check if user has sufficient credits and reserve them atomically.
    ///
    /// This is the single source of truth for credit enforcement.
    /// Uses optimistic locking with Firestore's Document.update_time precondition
    /// to prevent race conditions where concurrent requests could overspend credits.
    ///
    /// Returns `Ok(())` if credits are available and reserved, or `Err` with user-friendly message.
    ///
    /// IMPORTANT: Credits are charged upfront. They are NOT refunded if the job fails.
    pub async fn check_and_reserve_credits(&self, uid: &str, credits_needed: u32) -> ApiResult<()> {
        let limits = self.get_plan_limits(uid).await?;
        let current_month = current_month_key();
        let mut last_error: Option<vclip_firestore::FirestoreError> = None;

        for attempt in 0..Self::MAX_CREDIT_RESERVE_RETRIES {
            // Fetch document directly to get server-side update_time for precondition
            let doc = self.firestore
                .get_document("users", uid)
                .await
                .map_err(|e| ApiError::internal(format!("Firestore error: {}", e)))?;

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
                    // User doesn't exist, create them first
                    let _ = self.get_or_create_user(uid, None).await?;
                    (0u32, None, None)
                }
            };

            // Check if we need to reset for new month
            let effective_credits_used = if usage_reset_month.as_deref() == Some(&current_month) {
                credits_used
            } else {
                0 // Will reset on this write
            };

            // Check if we have enough credits
            let remaining = limits.monthly_credits_included.saturating_sub(effective_credits_used);
            if credits_needed > remaining {
                return Err(ApiError::forbidden(format!(
                    "Insufficient credits. You need {} credits but only have {} remaining ({} used of {} monthly limit). Please upgrade your plan.",
                    credits_needed, remaining, effective_credits_used, limits.monthly_credits_included
                )));
            }

            // Calculate new credit value
            let new_credits = if usage_reset_month.as_deref() == Some(&current_month) {
                credits_used.saturating_add(credits_needed)
            } else {
                // New month - reset counter
                credits_needed
            };

            // Build update fields
            let mut fields = std::collections::HashMap::new();
            fields.insert(
                "credits_used_this_month".to_string(),
                new_credits.to_firestore_value(),
            );
            fields.insert(
                "usage_reset_month".to_string(),
                current_month.to_firestore_value(),
            );
            fields.insert(
                "updated_at".to_string(),
                Utc::now().to_firestore_value(),
            );

            let update_mask = vec![
                "credits_used_this_month".to_string(),
                "usage_reset_month".to_string(),
                "updated_at".to_string(),
            ];

            // Attempt atomic update with Firestore's server-side update_time precondition
            match self.firestore
                .update_document_with_precondition(
                    "users",
                    uid,
                    fields,
                    Some(update_mask),
                    update_time.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    info!(
                        "Reserved {} credits for user {} (total now: {})",
                        credits_needed, uid, new_credits
                    );
                    return Ok(());
                }
                Err(e) if e.is_precondition_failed() => {
                    // Another writer updated the document; retry with exponential backoff
                    debug!(
                        "Credit reservation precondition failed for user {} (attempt {}), retrying",
                        uid, attempt + 1
                    );
                    last_error = Some(e);
                    // Exponential backoff: 50ms, 100ms, 150ms, 200ms, 250ms
                    tokio::time::sleep(tokio::time::Duration::from_millis(50 * (attempt as u64 + 1))).await;
                    continue;
                }
                Err(e) => {
                    warn!("Failed to reserve credits for user {}: {}", uid, e);
                    return Err(ApiError::internal("Failed to reserve credits"));
                }
            }
        }

        // Exhausted retries
        warn!(
            "Credit reservation failed after {} retries for user {}: {:?}",
            Self::MAX_CREDIT_RESERVE_RETRIES, uid, last_error
        );
        Err(ApiError::internal("Failed to reserve credits due to concurrent updates. Please try again."))
    }

    /// Set credits to a specific value (admin only).
    pub async fn set_credits(&self, uid: &str, credits: u32) -> ApiResult<UserRecord> {
        let mut user = self.get_or_create_user(uid, None).await?;

        let current_month = current_month_key();
        user.credits_used_this_month = credits;
        user.usage_reset_month = Some(current_month);
        user.updated_at = Utc::now();

        self.update_user(&user).await?;
        info!("Set user {} credits to {}", uid, credits);
        Ok(user)
    }

    /// Check if user owns a video.
    pub async fn user_owns_video(&self, uid: &str, video_id: &str) -> ApiResult<bool> {
        let video_repo = vclip_firestore::VideoRepository::new(
            (*self.firestore).clone(),
            uid,
        );
        
        match video_repo.get(&VideoId::from_string(video_id)).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => {
                debug!("Error checking video ownership: {}", e);
                Ok(false)
            }
        }
    }

    /// Check if video is currently processing.
    /// Returns false if video has highlights (effectively complete) even if status is stuck as processing.
    pub async fn is_video_processing(&self, uid: &str, video_id: &str) -> ApiResult<bool> {
        let video_repo = vclip_firestore::VideoRepository::new(
            (*self.firestore).clone(),
            uid,
        );

        match video_repo.get(&VideoId::from_string(video_id)).await {
            Ok(Some(video)) => {
                // If status is not processing, definitely not processing
                if video.status != VideoStatus::Processing {
                    return Ok(false);
                }

                // If status is processing, check if we have highlights
                // If highlights exist, video is effectively complete (handles stuck processing state)
                match self.storage.load_highlights(uid, video_id).await {
                    Ok(highlights) => {
                        // If highlights exist and have content, video is not actually processing
                        Ok(highlights.highlights.is_empty())
                    }
                    Err(_) => {
                        // If we can't load highlights, assume it's still processing
                        Ok(true)
                    }
                }
            }
            Ok(None) => Ok(false),
            Err(_) => Ok(false),
        }
    }

    /// Check if user has pro or studio plan.
    pub async fn has_pro_or_studio_plan(&self, uid: &str) -> ApiResult<bool> {
        let user = self.get_or_create_user(uid, None).await?;
        Ok(user.plan == "pro" || user.plan == "studio")
    }

    /// Check if user has studio plan.
    pub async fn has_studio_plan(&self, uid: &str) -> ApiResult<bool> {
        let user = self.get_or_create_user(uid, None).await?;
        Ok(user.plan == "studio")
    }

    /// Check if user is a super admin.
    pub async fn is_super_admin(&self, uid: &str) -> ApiResult<bool> {
        let user = self.get_or_create_user(uid, None).await?;
        Ok(user.role.as_deref() == Some("superadmin"))
    }

    /// Update a user's plan (admin only).
    pub async fn update_user_plan(&self, uid: &str, new_plan: &str) -> ApiResult<UserRecord> {
        // Validate plan exists
        let plan_exists = self.firestore.get_document("plans", new_plan).await
            .map_err(|e| ApiError::internal(format!("Failed to check plan: {}", e)))?
            .is_some();
        
        if !plan_exists {
            return Err(ApiError::bad_request(format!("Plan '{}' does not exist", new_plan)));
        }

        let mut user = self.get_or_create_user(uid, None).await?;
        user.plan = new_plan.to_string();
        user.updated_at = Utc::now();
        self.update_user(&user).await?;
        
        info!("Updated user {} plan to '{}'", uid, new_plan);
        Ok(user)
    }

    /// Get user by UID (public for admin use).
    pub async fn get_user_by_uid(&self, uid: &str) -> ApiResult<Option<UserRecord>> {
        self.get_user(uid).await
    }

    /// List all users with pagination.
    pub async fn list_users(&self, limit: u32, page_token: Option<&str>) -> ApiResult<(Vec<UserRecord>, Option<String>)> {
        let response = self.firestore
            .list_documents("users", Some(limit), page_token)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list users: {}", e)))?;
        
        let docs = response.documents.unwrap_or_default();
        let mut users = Vec::with_capacity(docs.len());
        for doc in docs {
            match parse_user_document(&doc) {
                Ok(user) => users.push(user),
                Err(e) => warn!("Failed to parse user document: {}", e),
            }
        }
        Ok((users, response.next_page_token))
    }



    /// Validate that user has sufficient credits for an operation.
    /// This does NOT reserve credits - use `check_and_reserve_credits()` for that.
    pub async fn validate_credits(&self, uid: &str, credits_needed: u32) -> ApiResult<()> {
        let limits = self.get_plan_limits(uid).await?;
        let used = self.get_credits_usage(uid).await?;

        if used + credits_needed > limits.monthly_credits_included {
            let remaining = limits.monthly_credits_included.saturating_sub(used);
            return Err(ApiError::forbidden(format!(
                "Insufficient credits. You need {} credits but only have {} remaining. Please upgrade your plan.",
                credits_needed, remaining
            )));
        }

        Ok(())
    }

    /// Update video status.
    pub async fn update_video_status(
        &self,
        uid: &str,
        video_id: &str,
        status: VideoStatus,
    ) -> ApiResult<()> {
        let video_repo = vclip_firestore::VideoRepository::new(
            (*self.firestore).clone(),
            uid,
        );
        
        video_repo
            .update_status(&VideoId::from_string(video_id), status)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update video status: {}", e)))?;
        
        Ok(())
    }

    /// Update video title.
    pub async fn update_video_title(
        &self,
        uid: &str,
        video_id: &str,
        title: &str,
    ) -> ApiResult<bool> {
        let video_repo = vclip_firestore::VideoRepository::new(
            (*self.firestore).clone(),
            uid,
        );
        
        // Check if video exists
        match video_repo.get(&VideoId::from_string(video_id)).await {
            Ok(Some(_)) => {
                // Update title using a custom update
                let mut fields = HashMap::new();
                fields.insert("video_title".to_string(), title.to_firestore_value());
                fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());
                
                self.firestore
                    .update_document(
                        &format!("users/{}/videos", uid),
                        video_id,
                        fields,
                        Some(vec!["video_title".to_string(), "updated_at".to_string()]),
                    )
                    .await
                    .map_err(|e| ApiError::internal(format!("Failed to update title: {}", e)))?;
                
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(e) => Err(ApiError::internal(format!("Failed to get video: {}", e))),
        }
    }

    // ========================================================================
    // Storage Tracking Methods
    // ========================================================================

    /// Maximum retries for optimistic concurrency updates.
    const MAX_STORAGE_UPDATE_RETRIES: u32 = 5;

    /// Get the user's current storage usage.
    pub async fn get_storage_usage(&self, uid: &str) -> ApiResult<vclip_models::StorageUsage> {
        let user = self.get_or_create_user(uid, None).await?;
        let limits = self.get_plan_limits(uid).await?;
        
        Ok(vclip_models::StorageUsage::new(
            user.total_storage_bytes,
            user.total_clips_count,
            limits.storage_limit_bytes,
        ))
    }

    /// Add storage usage when a clip is created (concurrency-safe).
    /// Uses optimistic locking with retry to handle concurrent updates.
    /// Returns the new total storage bytes.
    pub async fn add_storage(&self, uid: &str, size_bytes: u64) -> ApiResult<u64> {
        self.update_storage_with_retry(uid, size_bytes as i64, 1).await
    }

    /// Subtract storage usage when a clip is deleted (concurrency-safe).
    /// Uses optimistic locking with retry to handle concurrent updates.
    /// Returns the new total storage bytes.
    pub async fn subtract_storage(&self, uid: &str, size_bytes: u64) -> ApiResult<u64> {
        self.update_storage_with_retry(uid, -(size_bytes as i64), -1).await
    }

    /// Internal helper for concurrency-safe storage updates with retry.
    /// 
    /// Uses Firestore's `updateTime` precondition to implement optimistic locking.
    /// If another writer updated the document between our read and write, we retry.
    async fn update_storage_with_retry(
        &self,
        uid: &str,
        bytes_delta: i64,
        clips_delta: i32,
    ) -> ApiResult<u64> {
        let mut last_error = None;
        
        for attempt in 0..Self::MAX_STORAGE_UPDATE_RETRIES {
            // Get current document with update_time
            let doc = self.firestore
                .get_document("users", uid)
                .await
                .map_err(|e| ApiError::internal(format!("Firestore error: {}", e)))?;
            
            let (current_bytes, current_clips, update_time) = match &doc {
                Some(d) => {
                    let fields = d.fields.as_ref();
                    let bytes = fields
                        .and_then(|f| f.get("total_storage_bytes"))
                        .and_then(|v| u64::from_firestore_value(v))
                        .unwrap_or(0);
                    let clips = fields
                        .and_then(|f| f.get("total_clips_count"))
                        .and_then(|v| u32::from_firestore_value(v))
                        .unwrap_or(0);
                    (bytes, clips, d.update_time.clone())
                }
                None => {
                    // User doesn't exist, create them first
                    let _ = self.get_or_create_user(uid, None).await?;
                    (0u64, 0u32, None)
                }
            };
            
            // Calculate new values with safe arithmetic
            let new_bytes = if bytes_delta >= 0 {
                current_bytes.saturating_add(bytes_delta as u64)
            } else {
                current_bytes.saturating_sub((-bytes_delta) as u64)
            };
            
            let new_clips = if clips_delta >= 0 {
                current_clips.saturating_add(clips_delta as u32)
            } else {
                current_clips.saturating_sub((-clips_delta) as u32)
            };
            
            // Build update fields
            let mut fields = std::collections::HashMap::new();
            fields.insert("total_storage_bytes".to_string(), new_bytes.to_firestore_value());
            fields.insert("total_clips_count".to_string(), new_clips.to_firestore_value());
            fields.insert("updated_at".to_string(), Utc::now().to_firestore_value());
            
            let update_mask = vec![
                "total_storage_bytes".to_string(),
                "total_clips_count".to_string(),
                "updated_at".to_string(),
            ];
            
            // Attempt update with precondition
            match self.firestore
                .update_document_with_precondition(
                    "users",
                    uid,
                    fields,
                    Some(update_mask),
                    update_time.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    let action = if bytes_delta >= 0 { "Added" } else { "Subtracted" };
                    info!(
                        "{} {} bytes storage for user {}, new total: {} bytes ({} clips)",
                        action, bytes_delta.unsigned_abs(), uid, new_bytes, new_clips
                    );
                    return Ok(new_bytes);
                }
                Err(e) if e.is_precondition_failed() => {
                    // Another writer updated the document; retry
                    debug!(
                        "Storage update precondition failed for user {} (attempt {}), retrying",
                        uid, attempt + 1
                    );
                    last_error = Some(e);
                    // Brief backoff before retry
                    tokio::time::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1))).await;
                    continue;
                }
                Err(e) => {
                    return Err(ApiError::internal(format!("Failed to update storage: {}", e)));
                }
            }
        }
        
        // All retries exhausted
        warn!(
            "Storage update failed after {} retries for user {}: {:?}",
            Self::MAX_STORAGE_UPDATE_RETRIES, uid, last_error
        );
        Err(ApiError::internal(format!(
            "Failed to update storage after {} retries",
            Self::MAX_STORAGE_UPDATE_RETRIES
        )))
    }

    /// Check if adding a clip would exceed storage limits.
    /// Returns Ok(()) if within limits, Err with details if would exceed.
    pub async fn check_storage_quota(&self, uid: &str, additional_bytes: u64) -> ApiResult<()> {
        let usage = self.get_storage_usage(uid).await?;
        
        if usage.would_exceed(additional_bytes) {
            let limits = self.get_plan_limits(uid).await?;
            return Err(ApiError::forbidden(format!(
                "Storage quota exceeded. Current usage: {} / {}. Requested: {}. Upgrade to {} for more storage.",
                vclip_models::format_bytes(usage.total_bytes),
                vclip_models::format_bytes(usage.limit_bytes),
                vclip_models::format_bytes(additional_bytes),
                if limits.plan_id == "free" { "Pro" } else { "Studio" }
            )));
        }
        
        Ok(())
    }

    /// Unified quota enforcement check for clip creation.
    ///
    /// This is the single source of truth for quota checks, used by both
    /// HTTP handlers and WebSocket handlers to ensure consistent enforcement.
    ///
    /// Checks:
    /// 1. Monthly credit quota
    /// 2. Storage quota
    ///
    /// NOTE: This does NOT reserve credits. Call `check_and_reserve_credits()` after
    /// calculating the exact credit cost for the operation.
    ///
    /// Returns `Ok(QuotaCheckResult)` with current usage info, or `Err` with
    /// a user-friendly error message if any quota would be exceeded.
    pub async fn check_all_quotas(&self, uid: &str, credits_to_use: u32) -> ApiResult<QuotaCheckResult> {
        // Get all quota information upfront
        let limits = self.get_plan_limits(uid).await?;
        let credits_used = self.get_credits_usage(uid).await?;
        let storage_usage = self.get_storage_usage(uid).await?;

        // Check monthly credit quota
        if credits_used >= limits.monthly_credits_included {
            return Err(ApiError::forbidden(format!(
                "Monthly credit limit exceeded. You've used {} of {} credits this month. Please upgrade your plan or wait until next month.",
                credits_used, limits.monthly_credits_included
            )));
        }

        // Check if this request would exceed monthly quota
        if credits_used.saturating_add(credits_to_use) > limits.monthly_credits_included {
            let remaining = limits.monthly_credits_included.saturating_sub(credits_used);
            return Err(ApiError::forbidden(format!(
                "Insufficient credits. You need {} credits but only have {} remaining ({} used of {} monthly limit).",
                credits_to_use, remaining, credits_used, limits.monthly_credits_included
            )));
        }

        // Check storage quota
        if storage_usage.percentage() >= 100.0 {
            return Err(ApiError::forbidden(format!(
                "Storage limit exceeded. You've used {} of {} storage. Please delete some clips or upgrade your plan.",
                storage_usage.format_total(), storage_usage.format_limit()
            )));
        }

        Ok(QuotaCheckResult {
            credits_used,
            credits_limit: limits.monthly_credits_included,
            storage_used_bytes: storage_usage.total_bytes,
            storage_limit_bytes: storage_usage.limit_bytes,
            storage_percentage: storage_usage.percentage(),
            plan_id: limits.plan_id,
        })
    }
    
    /// Check if the user's plan allows reprocessing.
    pub async fn can_reprocess(&self, uid: &str) -> ApiResult<bool> {
        let limits = self.get_plan_limits(uid).await?;
        Ok(limits.can_reprocess)
    }

    /// Recalculate storage usage from all videos (for migration/consistency).
    pub async fn recalculate_storage(&self, uid: &str) -> ApiResult<(u64, u32)> {
        let video_repo = vclip_firestore::VideoRepository::new(
            (*self.firestore).clone(),
            uid,
        );
        
        let videos = video_repo.list(None).await
            .map_err(|e| ApiError::internal(format!("Failed to list videos: {}", e)))?;
        
        let mut total_bytes: u64 = 0;
        let mut total_clips: u32 = 0;
        
        for video in videos {
            let clip_repo = vclip_firestore::ClipRepository::new(
                (*self.firestore).clone(),
                uid,
                video.video_id.clone(),
            );
            
            let clips = clip_repo.list(None).await.unwrap_or_default();
            let video_size: u64 = clips.iter().map(|c| c.file_size_bytes).sum();
            let video_clips = clips.len() as u32;
            
            total_bytes += video_size;
            total_clips += video_clips;
            
            // Update video's total_size_bytes
            if let Err(e) = video_repo.update_total_size(&video.video_id, video_size).await {
                warn!("Failed to update video {} total size: {}", video.video_id, e);
            }
        }
        
        // Update user's totals
        let mut user = self.get_or_create_user(uid, None).await?;
        user.total_storage_bytes = total_bytes;
        user.total_clips_count = total_clips;
        user.updated_at = Utc::now();
        self.update_user(&user).await?;
        
        info!(
            "Recalculated storage for user {}: {} bytes, {} clips",
            uid, total_bytes, total_clips
        );
        
        Ok((total_bytes, total_clips))
    }
}

/// Get current month key (YYYY-MM).
fn current_month_key() -> String {
    let now = Utc::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

/// Parse user document from Firestore.
fn parse_user_document(doc: &vclip_firestore::Document) -> ApiResult<UserRecord> {
    let fields = doc.fields.as_ref().ok_or_else(|| {
        ApiError::internal("User document has no fields")
    })?;
    
    let get_string = |key: &str| -> Option<String> {
        fields.get(key).and_then(|v| String::from_firestore_value(v))
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
    
    Ok(UserRecord {
        uid: get_string("uid").unwrap_or_default(),
        email: get_string("email"),
        plan: get_string("plan").unwrap_or_else(|| "free".to_string()),
        created_at: fields
            .get("created_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
        updated_at: fields
            .get("updated_at")
            .and_then(|v| chrono::DateTime::from_firestore_value(v))
            .unwrap_or_else(Utc::now),
        settings: UserSettings::default(), // TODO: parse nested settings
        role: get_string("role"),
        credits_used_this_month: get_u32("credits_used_this_month"),
        usage_reset_month: get_string("usage_reset_month"),
        total_storage_bytes: get_u64("total_storage_bytes"),
        total_clips_count: get_u32("total_clips_count"),
    })
}

/// Parse plan limits from a Firestore plan document.
fn parse_plan_limits(plan_doc: &vclip_firestore::Document) -> ApiResult<PlanLimits> {
    let fields = plan_doc.fields.as_ref().ok_or_else(|| {
        ApiError::internal("Plan document has no fields")
    })?;

    // Get plan ID
    let plan_id = fields
        .get("id")
        .and_then(|v| String::from_firestore_value(v))
        .unwrap_or_else(|| "unknown".to_string());

    // Determine plan tier from ID
    let tier = vclip_models::PlanTier::from_str(&plan_id);

    // Get limits map
    let limits_map = if let Some(Value::MapValue(map)) = fields.get("limits") {
        map.fields.as_ref()
    } else {
        None
    };

    // Parse credits (with fallback to tier defaults)
    let monthly_credits_included = limits_map
        .and_then(|f| f.get("monthly_credits_included"))
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or_else(|| tier.monthly_credits());

    // Parse storage limit (with fallback to tier defaults)
    let storage_limit_bytes = limits_map
        .and_then(|f| f.get("storage_limit_bytes"))
        .and_then(|v| u64::from_firestore_value(v))
        .unwrap_or_else(|| tier.storage_limit_bytes());

    // Parse feature flags (with tier-based defaults)
    let watermark_exports = limits_map
        .and_then(|f| f.get("watermark_exports"))
        .and_then(|v| bool::from_firestore_value(v))
        .unwrap_or_else(|| tier.has_watermark());

    let api_access = limits_map
        .and_then(|f| f.get("api_access"))
        .and_then(|v| bool::from_firestore_value(v))
        .unwrap_or_else(|| tier.has_api_access());

    let channel_monitoring_included = limits_map
        .and_then(|f| f.get("channel_monitoring_included"))
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or_else(|| tier.channels_included());

    let connected_social_accounts_limit = limits_map
        .and_then(|f| f.get("connected_social_accounts_limit"))
        .and_then(|v| u32::from_firestore_value(v))
        .unwrap_or_else(|| tier.connected_accounts_limit());

    let priority_processing = limits_map
        .and_then(|f| f.get("priority_processing"))
        .and_then(|v| bool::from_firestore_value(v))
        .unwrap_or(matches!(tier, vclip_models::PlanTier::Pro | vclip_models::PlanTier::Studio));

    // For now, use reasonable defaults for other limits based on plan tier
    let (max_highlights_per_video, max_styles_per_video, can_reprocess) = match tier {
        vclip_models::PlanTier::Free => (3, 2, false),
        vclip_models::PlanTier::Pro => (10, 5, true),
        vclip_models::PlanTier::Studio => (25, 10, true),
    };

    Ok(PlanLimits {
        plan_id,
        monthly_credits_included,
        max_clip_length_seconds: vclip_models::MAX_CLIP_LENGTH_SECONDS,
        max_highlights_per_video,
        max_styles_per_video,
        can_reprocess,
        storage_limit_bytes,
        watermark_exports,
        api_access,
        channel_monitoring_included,
        connected_social_accounts_limit,
        priority_processing,
        tier,
    })
}

/// Convert user record to Firestore fields.
fn user_to_fields(user: &UserRecord) -> HashMap<String, Value> {
    let mut fields = HashMap::new();
    fields.insert("uid".to_string(), user.uid.to_firestore_value());
    if let Some(ref email) = user.email {
        fields.insert("email".to_string(), email.to_firestore_value());
    }
    fields.insert("plan".to_string(), user.plan.to_firestore_value());
    fields.insert("created_at".to_string(), user.created_at.to_firestore_value());
    fields.insert("updated_at".to_string(), user.updated_at.to_firestore_value());
    if let Some(ref role) = user.role {
        fields.insert("role".to_string(), role.to_firestore_value());
    }
    fields.insert(
        "credits_used_this_month".to_string(),
        user.credits_used_this_month.to_firestore_value(),
    );
    if let Some(ref month) = user.usage_reset_month {
        fields.insert("usage_reset_month".to_string(), month.to_firestore_value());
    }
    fields.insert(
        "total_storage_bytes".to_string(),
        user.total_storage_bytes.to_firestore_value(),
    );
    fields.insert(
        "total_clips_count".to_string(),
        user.total_clips_count.to_firestore_value(),
    );
    fields
}
