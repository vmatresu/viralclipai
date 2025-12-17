//! Configuration for silence removal.
//!
//! These parameters control how aggressively silence is detected and cut.
//! The defaults are tuned for streamer content with music/game audio.

use serde::{Deserialize, Serialize};

/// Configuration for silence removal using Silero VAD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilenceRemovalConfig {
    /// VAD threshold for speech detection (0.0-1.0).
    ///
    /// - Lower values (0.3-0.4): More sensitive, may detect breathing/noise as speech
    /// - Default (0.5): Balanced, works well for most content
    /// - Higher values (0.6-0.7): Less sensitive, only confident speech
    ///
    /// Silero VAD is trained to output ~0.5 for borderline cases.
    pub vad_threshold: f32,

    /// Minimum silence duration before marking as "Cut" (milliseconds).
    ///
    /// - Lower values (500ms): Aggressive cutting, faster paced
    /// - Default (1000ms): Natural pauses preserved, long silence cut
    /// - Higher values (2000ms+): Only cut very long pauses
    pub min_silence_ms: u64,

    /// Padding to keep before speech starts (milliseconds).
    ///
    /// This prevents cutting off the beginning of words.
    /// - 100-150ms: Tight, may clip some consonants
    /// - 200ms: Safe default
    /// - 300ms+: Very safe, keeps more silence
    pub pre_speech_padding_ms: u64,

    /// Padding to keep after speech ends (milliseconds).
    ///
    /// This prevents cutting off word endings and allows for natural trailing.
    /// - 100-150ms: Tight, may clip some endings
    /// - 200ms: Safe default
    /// - 300ms+: Very safe, keeps trailing sounds
    pub post_speech_padding_ms: u64,

    /// Minimum percentage of video that must be kept.
    ///
    /// If VAD detects less than this percentage as speech, silence removal
    /// is skipped entirely to avoid producing unusable output.
    /// - Default: 10% (0.1)
    pub min_keep_ratio: f32,

    /// Maximum number of segments before using file-based filter.
    ///
    /// FFmpeg command-line has length limits. For very choppy audio with
    /// many cuts, we switch to writing a filter script file.
    /// - Default: 100 segments
    pub max_inline_segments: usize,
}

impl Default for SilenceRemovalConfig {
    fn default() -> Self {
        Self {
            vad_threshold: 0.5,
            min_silence_ms: 1000,
            pre_speech_padding_ms: 200,
            post_speech_padding_ms: 200,
            min_keep_ratio: 0.1,
            max_inline_segments: 100,
        }
    }
}

impl SilenceRemovalConfig {
    /// Create a more aggressive configuration for fast-paced content.
    pub fn aggressive() -> Self {
        Self {
            vad_threshold: 0.4,
            min_silence_ms: 500,
            pre_speech_padding_ms: 150,
            post_speech_padding_ms: 150,
            min_keep_ratio: 0.05,
            max_inline_segments: 100,
        }
    }

    /// Create a conservative configuration that preserves more content.
    pub fn conservative() -> Self {
        Self {
            vad_threshold: 0.6,
            min_silence_ms: 2000,
            pre_speech_padding_ms: 300,
            post_speech_padding_ms: 300,
            min_keep_ratio: 0.2,
            max_inline_segments: 100,
        }
    }

    /// Builder-style setter for VAD threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.vad_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Builder-style setter for minimum silence duration.
    pub fn with_min_silence_ms(mut self, ms: u64) -> Self {
        self.min_silence_ms = ms;
        self
    }

    /// Builder-style setter for pre-speech padding.
    pub fn with_pre_padding_ms(mut self, ms: u64) -> Self {
        self.pre_speech_padding_ms = ms;
        self
    }

    /// Builder-style setter for post-speech padding.
    pub fn with_post_padding_ms(mut self, ms: u64) -> Self {
        self.post_speech_padding_ms = ms;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SilenceRemovalConfig::default();
        assert!((config.vad_threshold - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.min_silence_ms, 1000);
    }

    #[test]
    fn test_aggressive_config() {
        let config = SilenceRemovalConfig::aggressive();
        assert!(config.min_silence_ms < SilenceRemovalConfig::default().min_silence_ms);
    }

    #[test]
    fn test_builder_pattern() {
        let config = SilenceRemovalConfig::default()
            .with_threshold(0.7)
            .with_min_silence_ms(500);

        assert!((config.vad_threshold - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.min_silence_ms, 500);
    }

    #[test]
    fn test_threshold_clamping() {
        let config = SilenceRemovalConfig::default().with_threshold(1.5);
        assert!((config.vad_threshold - 1.0).abs() < f32::EPSILON);

        let config = SilenceRemovalConfig::default().with_threshold(-0.5);
        assert!(config.vad_threshold.abs() < f32::EPSILON);
    }
}
