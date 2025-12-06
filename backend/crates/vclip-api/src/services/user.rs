//! User and SaaS service for plan limits, ownership checks, and settings.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use vclip_firestore::{FirestoreClient, FromFirestoreValue, ToFirestoreValue, Value};
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
    #[serde(default)]
    pub clips_used_this_month: u32,
    #[serde(default)]
    pub usage_reset_month: Option<String>,
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
            clips_used_this_month: 0,
            usage_reset_month: None,
        }
    }
}

/// Plan limits configuration.
#[derive(Debug, Clone)]
pub struct PlanLimits {
    pub plan_id: String,
    pub max_clips_per_month: u32,
    pub max_highlights_per_video: u32,
    pub max_styles_per_video: u32,
    pub can_reprocess: bool,
}

impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            plan_id: "free".to_string(),
            max_clips_per_month: 20,
            max_highlights_per_video: 3,
            max_styles_per_video: 2,
            can_reprocess: false,
        }
    }
}

/// User service for SaaS operations.
#[derive(Clone)]
pub struct UserService {
    firestore: Arc<FirestoreClient>,
}

impl UserService {
    /// Create a new user service.
    pub fn new(firestore: Arc<FirestoreClient>) -> Self {
        Self { firestore }
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
                    clips_used_this_month: 0,
                    usage_reset_month: Some(current_month_key()),
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

    /// Get monthly usage for a user.
    pub async fn get_monthly_usage(&self, uid: &str) -> ApiResult<u32> {
        let user = self.get_or_create_user(uid, None).await?;
        
        // Check if we need to reset the counter
        let current_month = current_month_key();
        if user.usage_reset_month.as_deref() != Some(&current_month) {
            // Reset counter for new month
            return Ok(0);
        }
        
        Ok(user.clips_used_this_month)
    }

    /// Increment monthly usage.
    pub async fn increment_usage(&self, uid: &str, count: u32) -> ApiResult<()> {
        let mut user = self.get_or_create_user(uid, None).await?;
        
        let current_month = current_month_key();
        if user.usage_reset_month.as_deref() != Some(&current_month) {
            // Reset for new month
            user.clips_used_this_month = count;
            user.usage_reset_month = Some(current_month);
        } else {
            user.clips_used_this_month += count;
        }
        user.updated_at = Utc::now();
        
        self.update_user(&user).await?;
        Ok(())
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
    pub async fn is_video_processing(&self, uid: &str, video_id: &str) -> ApiResult<bool> {
        let video_repo = vclip_firestore::VideoRepository::new(
            (*self.firestore).clone(),
            uid,
        );
        
        match video_repo.get(&VideoId::from_string(video_id)).await {
            Ok(Some(video)) => Ok(video.status == VideoStatus::Processing),
            Ok(None) => Ok(false),
            Err(_) => Ok(false),
        }
    }

    /// Check if user has pro or studio plan.
    pub async fn has_pro_or_studio_plan(&self, uid: &str) -> ApiResult<bool> {
        let user = self.get_or_create_user(uid, None).await?;
        Ok(user.plan == "pro" || user.plan == "studio")
    }

    /// Check if user is a super admin.
    pub async fn is_super_admin(&self, uid: &str) -> ApiResult<bool> {
        let user = self.get_or_create_user(uid, None).await?;
        Ok(user.role.as_deref() == Some("superadmin"))
    }

    /// Validate plan limits before processing.
    pub async fn validate_plan_limits(&self, uid: &str, clip_count: u32) -> ApiResult<()> {
        let limits = self.get_plan_limits(uid).await?;
        let used = self.get_monthly_usage(uid).await?;
        
        if used + clip_count > limits.max_clips_per_month {
            return Err(ApiError::forbidden(format!(
                "Monthly clip limit exceeded. Used: {}, Limit: {}, Requested: {}",
                used, limits.max_clips_per_month, clip_count
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
        clips_used_this_month: get_u32("clips_used_this_month"),
        usage_reset_month: get_string("usage_reset_month"),
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

    // Get limits map
    let max_clips_per_month = if let Some(Value::MapValue(limits_map)) = fields.get("limits") {
        limits_map
            .fields
            .as_ref()
            .and_then(|fields| fields.get("max_clips_per_month"))
            .and_then(|v| u32::from_firestore_value(v))
            .unwrap_or(20) // Default fallback
    } else {
        20 // Default fallback
    };

    // For now, use reasonable defaults for other limits based on plan tier
    // TODO: Store these in Firestore as well
    let (max_highlights_per_video, max_styles_per_video, can_reprocess) = match plan_id.as_str() {
        "free" => (3, 2, false),
        "pro" => (10, 5, true),
        "studio" => (25, 10, true),
        _ => (3, 2, false),
    };

    Ok(PlanLimits {
        plan_id,
        max_clips_per_month,
        max_highlights_per_video,
        max_styles_per_video,
        can_reprocess,
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
        "clips_used_this_month".to_string(),
        user.clips_used_this_month.to_firestore_value(),
    );
    if let Some(ref month) = user.usage_reset_month {
        fields.insert("usage_reset_month".to_string(), month.to_firestore_value());
    }
    fields
}
