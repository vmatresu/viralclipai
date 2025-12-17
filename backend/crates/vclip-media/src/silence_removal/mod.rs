//! Silence removal using Silero VAD v5.
//!
//! This module implements "Layer A: The Meat Cleaver" - detecting
//! and removing segments without speech, even in presence of
//! music and game audio.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
//! │ Audio Input  │───►│ Silero VAD   │───►│ Segmenter    │
//! │ (16kHz mono) │    │ (speech_prob)│    │ (Keep/Cut)   │
//! └──────────────┘    └──────────────┘    └──────────────┘
//!                                                │
//!                                                ▼
//!                     ┌──────────────┐    ┌──────────────┐
//!                     │ Output Video │◄───│ FFmpeg       │
//!                     │ (cuts applied│    │ concat filter│
//!                     └──────────────┘    └──────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use vclip_media::silence_removal::{
//!     analyze_audio_segments,
//!     apply_silence_removal,
//!     SilenceRemovalConfig,
//! };
//!
//! // Analyze audio and get Keep/Cut segments
//! let config = SilenceRemovalConfig::default();
//! let segments = analyze_audio_segments(&input_path, config).await?;
//!
//! // Apply cuts using FFmpeg
//! apply_silence_removal(&input_path, &output_path, &segments).await?;
//! ```

mod analyze;
mod apply;
mod config;
mod segmenter;
mod vad;

pub use analyze::analyze_audio_segments;
pub use apply::{apply_silence_removal, should_apply_silence_removal};
pub use config::SilenceRemovalConfig;
pub use segmenter::{compute_segment_stats, Segment, SegmentLabel, SegmentStats, SilenceRemover};

/// Default configuration optimized for streamer content.
///
/// These defaults are tuned for:
/// - Twitch/YouTube/Kick stream VODs
/// - Mixed content with music, game audio, and speech
/// - Conservative cuts to avoid removing valid content
pub fn default_config() -> SilenceRemovalConfig {
    SilenceRemovalConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert!((config.vad_threshold - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.min_silence_ms, 1000);
        assert_eq!(config.pre_speech_padding_ms, 200);
        assert_eq!(config.post_speech_padding_ms, 200);
    }
}
