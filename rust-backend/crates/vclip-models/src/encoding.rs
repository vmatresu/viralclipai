//! Video encoding configuration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Default video codec (H.264)
pub const DEFAULT_VIDEO_CODEC: &str = "libx264";
/// Default audio codec
pub const DEFAULT_AUDIO_CODEC: &str = "aac";
/// Default encoding preset
pub const DEFAULT_PRESET: &str = "fast";
/// Default CRF (Constant Rate Factor) for traditional styles
pub const DEFAULT_CRF: u8 = 18;
/// Default audio bitrate
pub const DEFAULT_AUDIO_BITRATE: &str = "128k";

/// Thumbnail generation settings
pub const THUMBNAIL_SCALE_WIDTH: u32 = 480;
pub const THUMBNAIL_TIMESTAMP: &str = "00:00:01";

/// Split view resolution (top/bottom)
pub const SPLIT_VIEW_WIDTH: u32 = 1080;
pub const SPLIT_VIEW_HEIGHT: u32 = 960;

/// Video encoding configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EncodingConfig {
    /// Video codec (e.g., "libx264", "h264_nvenc")
    #[serde(default = "default_video_codec")]
    pub codec: String,

    /// Encoding preset (e.g., "fast", "medium", "slow")
    #[serde(default = "default_preset")]
    pub preset: String,

    /// Constant Rate Factor (quality, 0-51, lower is better)
    #[serde(default = "default_crf")]
    pub crf: u8,

    /// Audio codec
    #[serde(default = "default_audio_codec")]
    pub audio_codec: String,

    /// Audio bitrate
    #[serde(default = "default_audio_bitrate")]
    pub audio_bitrate: String,

    /// Use hardware acceleration (NVENC)
    #[serde(default)]
    pub use_nvenc: bool,

    /// Additional FFmpeg output arguments
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_video_codec() -> String {
    DEFAULT_VIDEO_CODEC.to_string()
}
fn default_preset() -> String {
    DEFAULT_PRESET.to_string()
}
fn default_crf() -> u8 {
    DEFAULT_CRF
}
fn default_audio_codec() -> String {
    DEFAULT_AUDIO_CODEC.to_string()
}
fn default_audio_bitrate() -> String {
    DEFAULT_AUDIO_BITRATE.to_string()
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            codec: DEFAULT_VIDEO_CODEC.to_string(),
            preset: DEFAULT_PRESET.to_string(),
            crf: DEFAULT_CRF,
            audio_codec: DEFAULT_AUDIO_CODEC.to_string(),
            audio_bitrate: DEFAULT_AUDIO_BITRATE.to_string(),
            use_nvenc: false,
            extra_args: Vec::new(),
        }
    }
}

impl EncodingConfig {
    /// Create a new encoding configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create configuration optimized for intelligent cropping.
    pub fn for_intelligent_crop() -> Self {
        Self {
            crf: 22, // Slightly higher for intelligent crop
            preset: "medium".to_string(),
            ..Default::default()
        }
    }

    /// Create configuration for split view (higher CRF to reduce file size).
    pub fn for_split_view() -> Self {
        Self {
            crf: DEFAULT_CRF + 4, // Higher CRF for split view
            preset: "medium".to_string(),
            ..Default::default()
        }
    }

    /// Returns a new config with updated CRF.
    pub fn with_crf(mut self, crf: u8) -> Self {
        self.crf = crf;
        self
    }

    /// Enable NVENC hardware acceleration.
    pub fn with_nvenc(mut self) -> Self {
        self.use_nvenc = true;
        self.codec = "h264_nvenc".to_string();
        self
    }

    /// Convert to FFmpeg command arguments.
    pub fn to_ffmpeg_args(&self) -> Vec<String> {
        let mut args = vec![
            "-c:v".to_string(),
            self.codec.clone(),
            "-preset".to_string(),
            self.preset.clone(),
        ];

        // CRF is not used with NVENC, use -cq instead
        if self.use_nvenc {
            args.extend_from_slice(&["-cq".to_string(), self.crf.to_string()]);
        } else {
            args.extend_from_slice(&["-crf".to_string(), self.crf.to_string()]);
        }

        args.extend_from_slice(&[
            "-c:a".to_string(),
            self.audio_codec.clone(),
            "-b:a".to_string(),
            self.audio_bitrate.clone(),
        ]);

        args.extend(self.extra_args.clone());

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EncodingConfig::default();
        assert_eq!(config.codec, "libx264");
        assert_eq!(config.crf, 18);
    }

    #[test]
    fn test_ffmpeg_args() {
        let config = EncodingConfig::default();
        let args = config.to_ffmpeg_args();
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"-crf".to_string()));
        assert!(args.contains(&"18".to_string()));
    }

    #[test]
    fn test_nvenc_config() {
        let config = EncodingConfig::default().with_nvenc();
        let args = config.to_ffmpeg_args();
        assert!(args.contains(&"h264_nvenc".to_string()));
        assert!(args.contains(&"-cq".to_string())); // NVENC uses -cq instead of -crf
    }
}
