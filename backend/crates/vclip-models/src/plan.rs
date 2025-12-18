//! Plan configuration, storage limits, and credit-based usage tracking.
//!
//! ## Credit System
//!
//! Credits are charged per finished clip based on the detection tier:
//! - Tier 1 (Static): 10 credits - No AI detection
//! - Tier 2 (Basic): 10 credits - YuNet face detection
//! - Tier 3 (Smart): 20 credits - Motion/Speaker-aware detection
//! - Tier 4 (Premium): 30 credits - Cinematic tier with trajectory optimization
//!
//! Additional feature costs:
//! - Video analysis (get scenes): 3 credits
//! - Scene originals download: 5 credits per scene
//! - Streamer style: 10 credits
//! - Streamer Split style: 10 credits per scene
//! - Silent remover add-on: +5 credits per scene
//! - Object detection add-on (Cinematic): +10 credits
//!
//! ## Monthly Credits by Plan
//!
//! - Free: 200 credits
//! - Pro: 4,000 credits
//! - Studio: 12,000 credits
//!
//! ## Important
//!
//! Deleting clips does NOT refund credits.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::detection_tier::DetectionTier;

/// Storage limits in bytes for each plan tier (v1 spec).
pub const FREE_STORAGE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024; // 1 GB
pub const PRO_STORAGE_LIMIT_BYTES: u64 = 30 * 1024 * 1024 * 1024; // 30 GB
pub const STUDIO_STORAGE_LIMIT_BYTES: u64 = 150 * 1024 * 1024 * 1024; // 150 GB

/// Monthly credits included by plan tier.
pub const FREE_MONTHLY_CREDITS: u32 = 200;
pub const PRO_MONTHLY_CREDITS: u32 = 4000;
pub const STUDIO_MONTHLY_CREDITS: u32 = 12000;

/// Maximum clip length in seconds (applies to all plans).
pub const MAX_CLIP_LENGTH_SECONDS: u32 = 90;

// =============================================================================
// Credit Costs
// =============================================================================

/// Credit cost for video analysis (getting scenes).
pub const ANALYSIS_CREDIT_COST: u32 = 3;

/// Credit cost for downloading scene originals (per scene).
pub const SCENE_ORIGINALS_DOWNLOAD_COST: u32 = 5;

/// Credit cost for Streamer style.
pub const STREAMER_STYLE_COST: u32 = 10;

/// Credit cost for StreamerSplit style (per scene).
pub const STREAMER_SPLIT_STYLE_COST: u32 = 10;

/// Extra credit cost for silent remover add-on (per scene).
pub const SILENT_REMOVER_ADDON_COST: u32 = 5;

/// Extra credit cost for object detection add-on (Cinematic).
pub const OBJECT_DETECTION_ADDON_COST: u32 = 10;

/// Get credit cost for a detection tier.
///
/// - None (Static): 10 credits
/// - Basic: 10 credits
/// - MotionAware/SpeakerAware (Smart): 20 credits
/// - Cinematic (Premium): 30 credits
pub fn credits_for_detection_tier(tier: DetectionTier) -> u32 {
    match tier {
        DetectionTier::None => 10,
        DetectionTier::Basic => 10,
        DetectionTier::MotionAware | DetectionTier::SpeakerAware => 20,
        DetectionTier::Cinematic => 30,
    }
}

/// Plan tier enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlanTier {
    #[default]
    Free,
    Pro,
    Studio,
}

impl PlanTier {
    /// Parse from string (case-insensitive).
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pro" => PlanTier::Pro,
            "studio" => PlanTier::Studio,
            _ => PlanTier::Free,
        }
    }

    /// Get the storage limit in bytes for this plan.
    pub fn storage_limit_bytes(&self) -> u64 {
        match self {
            PlanTier::Free => FREE_STORAGE_LIMIT_BYTES,
            PlanTier::Pro => PRO_STORAGE_LIMIT_BYTES,
            PlanTier::Studio => STUDIO_STORAGE_LIMIT_BYTES,
        }
    }

    /// Get monthly credits included for this plan.
    pub fn monthly_credits(&self) -> u32 {
        match self {
            PlanTier::Free => FREE_MONTHLY_CREDITS,
            PlanTier::Pro => PRO_MONTHLY_CREDITS,
            PlanTier::Studio => STUDIO_MONTHLY_CREDITS,
        }
    }

    /// Get the plan name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanTier::Free => "free",
            PlanTier::Pro => "pro",
            PlanTier::Studio => "studio",
        }
    }

    /// Whether exports from this plan include a watermark.
    pub fn has_watermark(&self) -> bool {
        matches!(self, PlanTier::Free)
    }

    /// Whether this plan has API access.
    pub fn has_api_access(&self) -> bool {
        matches!(self, PlanTier::Studio)
    }

    /// Whether this plan has channel monitoring.
    pub fn has_channel_monitoring(&self) -> bool {
        matches!(self, PlanTier::Studio)
    }

    /// Number of monitored channels included (Studio only).
    pub fn channels_included(&self) -> u32 {
        match self {
            PlanTier::Studio => 2,
            _ => 0,
        }
    }

    /// Maximum connected social accounts.
    pub fn connected_accounts_limit(&self) -> u32 {
        match self {
            PlanTier::Free => 1,
            PlanTier::Pro => 3,
            PlanTier::Studio => 10,
        }
    }

    /// Check if a detection tier is allowed on this plan.
    pub fn allows_detection_tier(&self, tier: DetectionTier) -> bool {
        match self {
            PlanTier::Free => matches!(tier, DetectionTier::None | DetectionTier::Basic),
            PlanTier::Pro => matches!(
                tier,
                DetectionTier::None
                    | DetectionTier::Basic
                    | DetectionTier::MotionAware
                    | DetectionTier::SpeakerAware
            ),
            PlanTier::Studio => true, // All tiers allowed
        }
    }
}

impl std::fmt::Display for PlanTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Plan limits configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanLimits {
    /// Plan identifier.
    pub plan_id: String,
    /// Monthly credits included.
    pub monthly_credits_included: u32,
    /// Maximum clip length in seconds.
    pub max_clip_length_seconds: u32,
    /// Maximum highlights per video.
    pub max_highlights_per_video: u32,
    /// Maximum styles per video.
    pub max_styles_per_video: u32,
    /// Whether reprocessing is allowed.
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
    /// Plan tier for tier-based feature checks.
    #[serde(default)]
    pub tier: PlanTier,
}

impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            plan_id: "free".to_string(),
            monthly_credits_included: FREE_MONTHLY_CREDITS,
            max_clip_length_seconds: MAX_CLIP_LENGTH_SECONDS,
            max_highlights_per_video: 3,
            max_styles_per_video: 2,
            can_reprocess: false,
            storage_limit_bytes: FREE_STORAGE_LIMIT_BYTES,
            watermark_exports: true,
            api_access: false,
            channel_monitoring_included: 0,
            connected_social_accounts_limit: 1,
            priority_processing: false,
            tier: PlanTier::Free,
        }
    }
}

impl PlanLimits {
    /// Create limits for a specific plan tier.
    pub fn for_tier(tier: PlanTier) -> Self {
        match tier {
            PlanTier::Free => Self::default(),
            PlanTier::Pro => Self {
                plan_id: "pro".to_string(),
                monthly_credits_included: PRO_MONTHLY_CREDITS,
                max_clip_length_seconds: MAX_CLIP_LENGTH_SECONDS,
                max_highlights_per_video: 10,
                max_styles_per_video: 5,
                can_reprocess: true,
                storage_limit_bytes: PRO_STORAGE_LIMIT_BYTES,
                watermark_exports: false,
                api_access: false,
                channel_monitoring_included: 0,
                connected_social_accounts_limit: 3,
                priority_processing: true,
                tier: PlanTier::Pro,
            },
            PlanTier::Studio => Self {
                plan_id: "studio".to_string(),
                monthly_credits_included: STUDIO_MONTHLY_CREDITS,
                max_clip_length_seconds: MAX_CLIP_LENGTH_SECONDS,
                max_highlights_per_video: 25,
                max_styles_per_video: 10,
                can_reprocess: true,
                storage_limit_bytes: STUDIO_STORAGE_LIMIT_BYTES,
                watermark_exports: false,
                api_access: true,
                channel_monitoring_included: 2,
                connected_social_accounts_limit: 10,
                priority_processing: true,
                tier: PlanTier::Studio,
            },
        }
    }

    /// Check if a detection tier is allowed on this plan.
    pub fn allows_detection_tier(&self, tier: DetectionTier) -> bool {
        self.tier.allows_detection_tier(tier)
    }
}

/// Storage usage information.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct StorageUsage {
    /// Total storage used in bytes.
    pub total_bytes: u64,
    /// Total number of clips.
    pub total_clips: u32,
    /// Storage limit in bytes.
    pub limit_bytes: u64,
}

impl StorageUsage {
    /// Create new storage usage.
    pub fn new(total_bytes: u64, total_clips: u32, limit_bytes: u64) -> Self {
        Self {
            total_bytes,
            total_clips,
            limit_bytes,
        }
    }

    /// Get usage as a percentage (0-100).
    pub fn percentage(&self) -> f64 {
        if self.limit_bytes == 0 {
            return 0.0;
        }
        (self.total_bytes as f64 / self.limit_bytes as f64) * 100.0
    }

    /// Check if adding bytes would exceed the limit.
    pub fn would_exceed(&self, additional_bytes: u64) -> bool {
        self.total_bytes.saturating_add(additional_bytes) > self.limit_bytes
    }

    /// Get remaining bytes.
    pub fn remaining_bytes(&self) -> u64 {
        self.limit_bytes.saturating_sub(self.total_bytes)
    }

    /// Format total bytes as human-readable string.
    pub fn format_total(&self) -> String {
        format_bytes(self.total_bytes)
    }

    /// Format limit bytes as human-readable string.
    pub fn format_limit(&self) -> String {
        format_bytes(self.limit_bytes)
    }

    /// Format remaining bytes as human-readable string.
    pub fn format_remaining(&self) -> String {
        format_bytes(self.remaining_bytes())
    }
}

/// Detailed storage accounting with per-category breakdown.
///
/// Phase 5 storage tracking split: only styled clips count toward quota.
/// Source videos, raw segments, and neural cache are non-billable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct StorageAccounting {
    // === Billable Storage (counts toward quota) ===

    /// Styled clip storage in bytes (the final rendered clips).
    /// This is the only category that counts toward the user's quota.
    pub styled_clips_bytes: u64,

    /// Number of styled clips.
    pub styled_clips_count: u32,

    // === Non-Billable Storage (does not count toward quota) ===

    /// Source video cache storage in bytes.
    /// Temporary copies of original videos for faster reprocessing.
    pub source_videos_bytes: u64,

    /// Raw segment cache storage in bytes.
    /// Extracted segments before style application.
    pub raw_segments_bytes: u64,

    /// Neural analysis cache storage in bytes.
    /// Cached face detection/tracking results.
    pub neural_cache_bytes: u64,

    // === Metadata ===

    /// Last updated timestamp.
    #[serde(default)]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl StorageAccounting {
    /// Create a new empty storage accounting.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total billable storage (styled clips only).
    pub fn billable_bytes(&self) -> u64 {
        self.styled_clips_bytes
    }

    /// Get total non-billable storage (cache).
    pub fn cache_bytes(&self) -> u64 {
        self.source_videos_bytes
            .saturating_add(self.raw_segments_bytes)
            .saturating_add(self.neural_cache_bytes)
    }

    /// Get total storage across all categories.
    pub fn total_bytes(&self) -> u64 {
        self.billable_bytes().saturating_add(self.cache_bytes())
    }

    /// Check if adding bytes to styled clips would exceed the limit.
    pub fn would_exceed_quota(&self, additional_bytes: u64, limit_bytes: u64) -> bool {
        self.billable_bytes().saturating_add(additional_bytes) > limit_bytes
    }

    /// Convert to a StorageUsage (for backwards compatibility).
    /// Only includes billable storage.
    pub fn to_quota_usage(&self, limit_bytes: u64) -> StorageUsage {
        StorageUsage::new(self.billable_bytes(), self.styled_clips_count, limit_bytes)
    }

    /// Add styled clip storage.
    pub fn add_styled_clip(&mut self, bytes: u64) {
        self.styled_clips_bytes = self.styled_clips_bytes.saturating_add(bytes);
        self.styled_clips_count = self.styled_clips_count.saturating_add(1);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Add source video storage.
    pub fn add_source_video(&mut self, bytes: u64) {
        self.source_videos_bytes = self.source_videos_bytes.saturating_add(bytes);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Add raw segment storage.
    pub fn add_raw_segment(&mut self, bytes: u64) {
        self.raw_segments_bytes = self.raw_segments_bytes.saturating_add(bytes);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Add neural cache storage.
    pub fn add_neural_cache(&mut self, bytes: u64) {
        self.neural_cache_bytes = self.neural_cache_bytes.saturating_add(bytes);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Remove styled clip storage.
    pub fn remove_styled_clip(&mut self, bytes: u64) {
        self.styled_clips_bytes = self.styled_clips_bytes.saturating_sub(bytes);
        self.styled_clips_count = self.styled_clips_count.saturating_sub(1);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Remove source video storage.
    pub fn remove_source_video(&mut self, bytes: u64) {
        self.source_videos_bytes = self.source_videos_bytes.saturating_sub(bytes);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Remove raw segment storage.
    pub fn remove_raw_segment(&mut self, bytes: u64) {
        self.raw_segments_bytes = self.raw_segments_bytes.saturating_sub(bytes);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Remove neural cache storage.
    pub fn remove_neural_cache(&mut self, bytes: u64) {
        self.neural_cache_bytes = self.neural_cache_bytes.saturating_sub(bytes);
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Clear all non-billable storage for a video deletion.
    /// This zeros out source, raw, and neural cache bytes.
    pub fn clear_video_cache(&mut self) {
        self.source_videos_bytes = 0;
        self.raw_segments_bytes = 0;
        self.neural_cache_bytes = 0;
        self.updated_at = Some(chrono::Utc::now());
    }
}

/// Format bytes as human-readable string (KB, MB, GB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_tier_storage_limits() {
        // v1 spec: Free=1GB, Pro=30GB, Studio=150GB
        assert_eq!(PlanTier::Free.storage_limit_bytes(), 1024 * 1024 * 1024);
        assert_eq!(PlanTier::Pro.storage_limit_bytes(), 30 * 1024 * 1024 * 1024);
        assert_eq!(
            PlanTier::Studio.storage_limit_bytes(),
            150 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_storage_constants_match_tiers() {
        // Verify constants match tier methods
        assert_eq!(
            FREE_STORAGE_LIMIT_BYTES,
            PlanTier::Free.storage_limit_bytes()
        );
        assert_eq!(PRO_STORAGE_LIMIT_BYTES, PlanTier::Pro.storage_limit_bytes());
        assert_eq!(
            STUDIO_STORAGE_LIMIT_BYTES,
            PlanTier::Studio.storage_limit_bytes()
        );
    }

    #[test]
    fn test_monthly_credits() {
        assert_eq!(PlanTier::Free.monthly_credits(), 200);
        assert_eq!(PlanTier::Pro.monthly_credits(), 4000);
        assert_eq!(PlanTier::Studio.monthly_credits(), 12000);
    }

    #[test]
    fn test_credits_for_detection_tier() {
        assert_eq!(credits_for_detection_tier(DetectionTier::None), 10);
        assert_eq!(credits_for_detection_tier(DetectionTier::Basic), 10);
        assert_eq!(credits_for_detection_tier(DetectionTier::MotionAware), 20);
        assert_eq!(credits_for_detection_tier(DetectionTier::SpeakerAware), 20);
        assert_eq!(credits_for_detection_tier(DetectionTier::Cinematic), 30);
    }

    #[test]
    fn test_plan_tier_allows_detection_tier() {
        // Free: only None and Basic
        assert!(PlanTier::Free.allows_detection_tier(DetectionTier::None));
        assert!(PlanTier::Free.allows_detection_tier(DetectionTier::Basic));
        assert!(!PlanTier::Free.allows_detection_tier(DetectionTier::MotionAware));
        assert!(!PlanTier::Free.allows_detection_tier(DetectionTier::SpeakerAware));
        assert!(!PlanTier::Free.allows_detection_tier(DetectionTier::Cinematic));

        // Pro: all except Cinematic
        assert!(PlanTier::Pro.allows_detection_tier(DetectionTier::None));
        assert!(PlanTier::Pro.allows_detection_tier(DetectionTier::Basic));
        assert!(PlanTier::Pro.allows_detection_tier(DetectionTier::MotionAware));
        assert!(PlanTier::Pro.allows_detection_tier(DetectionTier::SpeakerAware));
        assert!(!PlanTier::Pro.allows_detection_tier(DetectionTier::Cinematic));

        // Studio: all tiers
        assert!(PlanTier::Studio.allows_detection_tier(DetectionTier::None));
        assert!(PlanTier::Studio.allows_detection_tier(DetectionTier::Cinematic));
    }

    #[test]
    fn test_plan_feature_flags() {
        // Free
        assert!(PlanTier::Free.has_watermark());
        assert!(!PlanTier::Free.has_api_access());
        assert!(!PlanTier::Free.has_channel_monitoring());

        // Pro
        assert!(!PlanTier::Pro.has_watermark());
        assert!(!PlanTier::Pro.has_api_access());
        assert!(!PlanTier::Pro.has_channel_monitoring());

        // Studio
        assert!(!PlanTier::Studio.has_watermark());
        assert!(PlanTier::Studio.has_api_access());
        assert!(PlanTier::Studio.has_channel_monitoring());
        assert_eq!(PlanTier::Studio.channels_included(), 2);
    }

    #[test]
    fn test_storage_usage_percentage() {
        let usage = StorageUsage::new(50 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert!((usage.percentage() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_storage_usage_percentage_zero_limit() {
        // Zero limit should return 0% to avoid division by zero
        let usage = StorageUsage::new(50 * 1024 * 1024, 10, 0);
        assert!((usage.percentage() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_storage_usage_percentage_at_100() {
        let usage = StorageUsage::new(100 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert!((usage.percentage() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_storage_usage_percentage_over_100() {
        // Over 100% usage should still calculate correctly
        let usage = StorageUsage::new(150 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert!((usage.percentage() - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_storage_usage_would_exceed() {
        let usage = StorageUsage::new(90 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert!(!usage.would_exceed(5 * 1024 * 1024));
        assert!(usage.would_exceed(15 * 1024 * 1024));
    }

    #[test]
    fn test_storage_usage_would_exceed_edge_cases() {
        let usage = StorageUsage::new(100 * 1024 * 1024, 10, 100 * 1024 * 1024);
        // Already at limit, any additional bytes should exceed
        assert!(usage.would_exceed(1));
        // Zero bytes should not exceed
        assert!(!usage.would_exceed(0));
    }

    #[test]
    fn test_storage_usage_remaining_bytes() {
        let usage = StorageUsage::new(90 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert_eq!(usage.remaining_bytes(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_storage_usage_remaining_bytes_at_limit() {
        let usage = StorageUsage::new(100 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert_eq!(usage.remaining_bytes(), 0);
    }

    #[test]
    fn test_storage_usage_remaining_bytes_over_limit() {
        // Over limit should still return 0 (saturating_sub)
        let usage = StorageUsage::new(150 * 1024 * 1024, 10, 100 * 1024 * 1024);
        assert_eq!(usage.remaining_bytes(), 0);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_format_bytes_edge_cases() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
        // Fractional values
        assert_eq!(format_bytes(1024 + 512), "1.50 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024 + 512 * 1024), "2.50 MB");
    }

    #[test]
    fn test_plan_tier_from_string() {
        assert_eq!(PlanTier::from_str("free"), PlanTier::Free);
        assert_eq!(PlanTier::from_str("pro"), PlanTier::Pro);
        assert_eq!(PlanTier::from_str("studio"), PlanTier::Studio);
        assert_eq!(PlanTier::from_str("unknown"), PlanTier::Free); // Default
        assert_eq!(PlanTier::from_str("FREE"), PlanTier::Free); // Case insensitive
        assert_eq!(PlanTier::from_str("Pro"), PlanTier::Pro); // Mixed case
    }
}
