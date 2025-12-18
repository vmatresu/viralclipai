//! Settings API handlers.

use std::collections::HashMap;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::AppState;

/// User settings response.
#[derive(Serialize)]
pub struct UserSettingsResponse {
    pub settings: HashMap<String, serde_json::Value>,
    pub plan: String,
    /// Monthly credits included in plan.
    pub monthly_credits_limit: u32,
    /// Credits used this month.
    pub credits_used_this_month: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Storage usage information
    pub storage: StorageInfo,
    /// Feature flags for frontend gating.
    pub features: FeatureFlags,
    /// Credit pricing information for UI display.
    pub credit_pricing: CreditPricing,
}

/// Credit pricing information for frontend display.
/// NOTE: These are for display only - backend always computes actual cost.
#[derive(Serialize)]
pub struct CreditPricing {
    /// Cost to analyze a video (detect scenes).
    pub analysis_credit_cost: u32,
    /// Credit cost per style (keyed by style name).
    pub style_credit_costs: HashMap<String, u32>,
    /// Add-on costs.
    pub addons: AddonPricing,
}

/// Add-on pricing.
#[derive(Serialize)]
pub struct AddonPricing {
    /// Cost per scene for silent part removal.
    pub silent_remover_per_scene: u32,
    /// Cost for object detection add-on.
    pub object_detection_addon: u32,
    /// Cost per scene for downloading originals.
    pub scene_originals_download_per_scene: u32,
}

/// Feature flags based on the user's plan.
#[derive(Serialize)]
pub struct FeatureFlags {
    /// Whether exports include watermark.
    pub watermark_exports: bool,
    /// Whether API access is enabled.
    pub api_access: bool,
    /// Whether channel monitoring is enabled.
    pub channel_monitoring: bool,
    /// Maximum clip length in seconds.
    pub max_clip_length_seconds: u32,
    /// Whether priority processing is enabled.
    pub priority_processing: bool,
}

/// Storage usage information for the user.
#[derive(Serialize)]
pub struct StorageInfo {
    /// Total storage used in bytes.
    pub used_bytes: u64,
    /// Storage limit in bytes.
    pub limit_bytes: u64,
    /// Total number of clips.
    pub total_clips: u32,
    /// Usage percentage (0-100).
    pub percentage: f64,
    /// Human-readable used storage.
    pub used_formatted: String,
    /// Human-readable storage limit.
    pub limit_formatted: String,
    /// Human-readable remaining storage.
    pub remaining_formatted: String,
}

/// Get user settings.
pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UserSettingsResponse>> {
    // Get user settings
    let settings = state.user_service.get_user_settings(&user.uid).await?;

    // Get plan limits
    let limits = state.user_service.get_plan_limits(&user.uid).await?;

    // Get credits usage
    let credits_used = state.user_service.get_credits_usage(&user.uid).await?;

    // Get storage usage
    let storage_usage = state.user_service.get_storage_usage(&user.uid).await?;

    // Check if super admin
    let role = if state.user_service.is_super_admin(&user.uid).await? {
        Some("superadmin".to_string())
    } else {
        None
    };

    // Convert settings to HashMap
    let mut settings_map = settings.extra;
    if !settings.default_styles.is_empty() {
        settings_map.insert(
            "default_styles".to_string(),
            serde_json::json!(settings.default_styles),
        );
    }
    if let Some(mode) = settings.default_crop_mode {
        settings_map.insert("default_crop_mode".to_string(), serde_json::json!(mode));
    }
    if let Some(aspect) = settings.default_target_aspect {
        settings_map.insert("default_target_aspect".to_string(), serde_json::json!(aspect));
    }

    // Extract values from limits before building the response
    let plan_id = limits.plan_id.clone();
    let monthly_credits = limits.monthly_credits_included;
    let watermark = limits.watermark_exports;
    let api = limits.api_access;
    let monitoring = limits.channel_monitoring_included > 0;
    let max_len = limits.max_clip_length_seconds;
    let priority = limits.priority_processing;

    // Build credit pricing from model constants
    let mut style_credit_costs = HashMap::new();
    use vclip_models::Style;
    for style in Style::ALL.iter() {
        style_credit_costs.insert(style.to_string(), style.credit_cost());
    }

    Ok(Json(UserSettingsResponse {
        settings: settings_map,
        plan: plan_id,
        monthly_credits_limit: monthly_credits,
        credits_used_this_month: credits_used,
        role,
        storage: StorageInfo {
            used_bytes: storage_usage.total_bytes,
            limit_bytes: storage_usage.limit_bytes,
            total_clips: storage_usage.total_clips,
            percentage: storage_usage.percentage(),
            used_formatted: storage_usage.format_total(),
            limit_formatted: storage_usage.format_limit(),
            remaining_formatted: storage_usage.format_remaining(),
        },
        features: FeatureFlags {
            watermark_exports: watermark,
            api_access: api,
            channel_monitoring: monitoring,
            max_clip_length_seconds: max_len,
            priority_processing: priority,
        },
        credit_pricing: CreditPricing {
            analysis_credit_cost: vclip_models::ANALYSIS_CREDIT_COST,
            style_credit_costs,
            addons: AddonPricing {
                silent_remover_per_scene: 5,
                object_detection_addon: 10,
                scene_originals_download_per_scene: 5,
            },
        },
    }))
}

/// Settings update request.
#[derive(Debug, Deserialize)]
pub struct SettingsUpdateRequest {
    pub settings: HashMap<String, serde_json::Value>,
}

/// Settings update response.
#[derive(Serialize)]
pub struct SettingsUpdateResponse {
    pub settings: HashMap<String, serde_json::Value>,
}

/// Update user settings.
pub async fn update_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<SettingsUpdateRequest>,
) -> ApiResult<Json<SettingsUpdateResponse>> {
    // Validate settings size
    let settings_json = serde_json::to_string(&request.settings).unwrap_or_default();
    if settings_json.len() > 10000 {
        return Err(crate::error::ApiError::bad_request("Settings payload too large"));
    }
    
    // Update settings
    let updated = state
        .user_service
        .update_user_settings(&user.uid, request.settings)
        .await?;
    
    Ok(Json(SettingsUpdateResponse { settings: updated }))
}
