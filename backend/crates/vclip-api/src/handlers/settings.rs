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
    pub max_clips_per_month: u32,
    pub clips_used_this_month: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
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
    
    // Get monthly usage
    let used = state.user_service.get_monthly_usage(&user.uid).await?;
    
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
    
    Ok(Json(UserSettingsResponse {
        settings: settings_map,
        plan: limits.plan_id,
        max_clips_per_month: limits.max_clips_per_month,
        clips_used_this_month: used,
        role,
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
