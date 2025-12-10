//! Share link models for clip sharing.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Public access level for shared clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShareAccessLevel {
    /// No public access (owner only).
    #[default]
    None,
    /// View/playback only.
    ViewPlayback,
    /// Download allowed.
    Download,
}

impl ShareAccessLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShareAccessLevel::None => "none",
            ShareAccessLevel::ViewPlayback => "view_playback",
            ShareAccessLevel::Download => "download",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "view_playback" => ShareAccessLevel::ViewPlayback,
            "download" => ShareAccessLevel::Download,
            _ => ShareAccessLevel::None,
        }
    }

    /// Check if this level allows playback.
    pub fn allows_playback(&self) -> bool {
        matches!(self, ShareAccessLevel::ViewPlayback | ShareAccessLevel::Download)
    }

    /// Check if this level allows download.
    pub fn allows_download(&self) -> bool {
        matches!(self, ShareAccessLevel::Download)
    }
}

/// Share configuration for a clip.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShareConfig {
    /// Unique share slug (URL-safe random token).
    pub share_slug: String,

    /// Clip ID being shared.
    pub clip_id: String,

    /// User ID (owner).
    pub user_id: String,

    /// Video ID the clip belongs to.
    pub video_id: String,

    /// Public access level.
    #[serde(default)]
    pub access_level: ShareAccessLevel,

    /// Optional expiry timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether watermark is enabled for shared playback.
    #[serde(default)]
    pub watermark_enabled: bool,

    /// When the share was created.
    pub created_at: DateTime<Utc>,

    /// When the share was last updated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,

    /// When the share was disabled/revoked (null = active).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<DateTime<Utc>>,
}

impl ShareConfig {
    /// Create a new share config.
    pub fn new(
        clip_id: impl Into<String>,
        user_id: impl Into<String>,
        video_id: impl Into<String>,
        access_level: ShareAccessLevel,
    ) -> Self {
        Self {
            share_slug: generate_share_slug(),
            clip_id: clip_id.into(),
            user_id: user_id.into(),
            video_id: video_id.into(),
            access_level,
            expires_at: None,
            watermark_enabled: false,
            created_at: Utc::now(),
            updated_at: None,
            disabled_at: None,
        }
    }

    /// Check if the share is active (not disabled, not expired).
    pub fn is_active(&self) -> bool {
        if self.disabled_at.is_some() {
            return false;
        }
        if let Some(expires) = self.expires_at {
            if expires < Utc::now() {
                return false;
            }
        }
        self.access_level != ShareAccessLevel::None
    }

    /// Check if the share is expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|e| e < Utc::now()).unwrap_or(false)
    }

    /// Disable the share.
    pub fn disable(mut self) -> Self {
        self.disabled_at = Some(Utc::now());
        self.updated_at = Some(Utc::now());
        self
    }

    /// Update access level.
    pub fn with_access_level(mut self, level: ShareAccessLevel) -> Self {
        self.access_level = level;
        self.updated_at = Some(Utc::now());
        self
    }

    /// Set expiry.
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self.updated_at = Some(Utc::now());
        self
    }

    /// Enable watermark.
    pub fn with_watermark(mut self) -> Self {
        self.watermark_enabled = true;
        self.updated_at = Some(Utc::now());
        self
    }
}

/// Maximum allowed expiry for share links (30 days).
pub const MAX_SHARE_EXPIRY_HOURS: u32 = 720;

/// Generate a URL-safe random share slug using cryptographically secure randomness.
///
/// Uses UUID v4 (which relies on OS randomness) to generate a 12-character
/// alphanumeric slug that is hard to guess.
fn generate_share_slug() -> String {
    // UUID v4 uses OS-provided randomness (e.g., /dev/urandom on Unix)
    // This provides 122 bits of entropy, far more than needed for share slugs
    let uuid = uuid::Uuid::new_v4();
    let bytes = uuid.as_bytes();
    
    // Base62 alphabet for URL-safe slugs
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    
    // Generate 12-character slug from first 9 bytes of UUID
    // Each byte gives us ~8 bits, we use 6 bits per character (log2(62) ≈ 5.95)
    let mut result = String::with_capacity(12);
    for i in 0..12 {
        // Use different byte combinations to maximize entropy usage
        let byte_idx = i * 9 / 12;
        let shift = (i * 3) % 8;
        let value = if byte_idx + 1 < bytes.len() {
            ((bytes[byte_idx] as u16) << 8 | bytes[byte_idx + 1] as u16) >> shift
        } else {
            bytes[byte_idx] as u16
        };
        result.push(CHARS[(value as usize) % 62] as char);
    }
    
    result
}

/// Validate a share slug format.
pub fn is_valid_share_slug(slug: &str) -> bool {
    // Share slugs are 8-16 alphanumeric characters
    if slug.len() < 8 || slug.len() > 16 {
        return false;
    }
    slug.chars().all(|c| c.is_ascii_alphanumeric())
}

// ============================================================================
// API Request/Response Types
// ============================================================================

/// Request to create or update a share.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateShareRequest {
    /// Access level.
    #[serde(default)]
    pub access_level: ShareAccessLevel,

    /// Optional expiry in hours from now (max 720 hours / 30 days).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_hours: Option<u32>,

    /// Enable watermark.
    #[serde(default)]
    pub watermark_enabled: bool,
}

impl Default for CreateShareRequest {
    fn default() -> Self {
        Self {
            access_level: ShareAccessLevel::ViewPlayback,
            expires_in_hours: None,
            watermark_enabled: false,
        }
    }
}

/// Response for share creation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShareResponse {
    /// The public share URL.
    pub share_url: String,

    /// Share slug (for reference).
    pub share_slug: String,

    /// Access level.
    pub access_level: ShareAccessLevel,

    /// When it expires (null = never).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    /// Watermark enabled.
    pub watermark_enabled: bool,

    /// When created.
    pub created_at: String,
}

impl ShareResponse {
    /// Create from ShareConfig.
    pub fn from_config(config: &ShareConfig, base_url: &str) -> Self {
        Self {
            share_url: format!("{}/c/{}", base_url.trim_end_matches('/'), config.share_slug),
            share_slug: config.share_slug.clone(),
            access_level: config.access_level,
            expires_at: config.expires_at.map(|e| e.to_rfc3339()),
            watermark_enabled: config.watermark_enabled,
            created_at: config.created_at.to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_slug_generation() {
        let slug1 = generate_share_slug();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let slug2 = generate_share_slug();

        assert!(is_valid_share_slug(&slug1));
        assert!(is_valid_share_slug(&slug2));
        assert_ne!(slug1, slug2, "slugs should be unique");
    }

    #[test]
    fn test_share_slug_validation() {
        assert!(is_valid_share_slug("abcd1234"));
        assert!(is_valid_share_slug("ABCD1234efgh"));
        assert!(!is_valid_share_slug("short"));
        assert!(!is_valid_share_slug("has-dash"));
        assert!(!is_valid_share_slug("has_underscore"));
    }

    #[test]
    fn test_share_config_is_active() {
        let config = ShareConfig::new("clip-1", "user-1", "video-1", ShareAccessLevel::ViewPlayback);
        assert!(config.is_active());

        let disabled = config.clone().disable();
        assert!(!disabled.is_active());

        let no_access = ShareConfig::new("clip-1", "user-1", "video-1", ShareAccessLevel::None);
        assert!(!no_access.is_active());
    }

    #[test]
    fn test_access_level_permissions() {
        assert!(!ShareAccessLevel::None.allows_playback());
        assert!(!ShareAccessLevel::None.allows_download());

        assert!(ShareAccessLevel::ViewPlayback.allows_playback());
        assert!(!ShareAccessLevel::ViewPlayback.allows_download());

        assert!(ShareAccessLevel::Download.allows_playback());
        assert!(ShareAccessLevel::Download.allows_download());
    }
}
