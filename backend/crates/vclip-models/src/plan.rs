//! Plan configuration and storage limits.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Storage limits in bytes for each plan tier.
pub const FREE_STORAGE_LIMIT_BYTES: u64 = 100 * 1024 * 1024; // 100 MB
pub const PRO_STORAGE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024; // 1 GB
pub const STUDIO_STORAGE_LIMIT_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5 GB

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

    /// Get the plan name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanTier::Free => "free",
            PlanTier::Pro => "pro",
            PlanTier::Studio => "studio",
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
    /// Maximum clips per month.
    pub max_clips_per_month: u32,
    /// Maximum highlights per video.
    pub max_highlights_per_video: u32,
    /// Maximum styles per video.
    pub max_styles_per_video: u32,
    /// Whether reprocessing is allowed.
    pub can_reprocess: bool,
    /// Storage limit in bytes.
    pub storage_limit_bytes: u64,
}

impl Default for PlanLimits {
    fn default() -> Self {
        Self {
            plan_id: "free".to_string(),
            max_clips_per_month: 20,
            max_highlights_per_video: 3,
            max_styles_per_video: 2,
            can_reprocess: false,
            storage_limit_bytes: FREE_STORAGE_LIMIT_BYTES,
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
                max_clips_per_month: 500,
                max_highlights_per_video: 10,
                max_styles_per_video: 5,
                can_reprocess: true,
                storage_limit_bytes: PRO_STORAGE_LIMIT_BYTES,
            },
            PlanTier::Studio => Self {
                plan_id: "studio".to_string(),
                max_clips_per_month: 2000,
                max_highlights_per_video: 25,
                max_styles_per_video: 10,
                can_reprocess: true,
                storage_limit_bytes: STUDIO_STORAGE_LIMIT_BYTES,
            },
        }
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
        assert_eq!(PlanTier::Free.storage_limit_bytes(), 100 * 1024 * 1024);
        assert_eq!(PlanTier::Pro.storage_limit_bytes(), 1024 * 1024 * 1024);
        assert_eq!(PlanTier::Studio.storage_limit_bytes(), 5 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_storage_constants_match_tiers() {
        // Verify constants match tier methods
        assert_eq!(FREE_STORAGE_LIMIT_BYTES, PlanTier::Free.storage_limit_bytes());
        assert_eq!(PRO_STORAGE_LIMIT_BYTES, PlanTier::Pro.storage_limit_bytes());
        assert_eq!(STUDIO_STORAGE_LIMIT_BYTES, PlanTier::Studio.storage_limit_bytes());
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
