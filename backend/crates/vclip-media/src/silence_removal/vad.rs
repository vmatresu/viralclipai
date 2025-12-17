//! Wrapper for Silero VAD v5 using voice_activity_detector crate.
//!
//! This module provides a thin abstraction over the voice_activity_detector crate,
//! making it easy to swap implementations if needed.
//!
//! # Why Silero VAD?
//!
//! - Works on CPU with good performance (no GPU required)
//! - Handles music/game audio well (doesn't confuse it with speech)
//! - ONNX model bundled in the crate (no external downloads)
//! - Widely used and battle-tested
//!
//! # Sample Rate Requirements
//!
//! Silero VAD v5 supports:
//! - 8kHz: 256 samples per frame (~32ms)
//! - 16kHz: 512 samples per frame (~32ms)
//!
//! The analyze module handles conversion from any input format.

use thiserror::Error;
use tracing::{debug, trace};
use voice_activity_detector::VoiceActivityDetector;

/// Errors from VAD operations.
#[derive(Error, Debug)]
pub enum VadError {
    #[error("Failed to initialize Silero VAD: {0}")]
    InitializationFailed(String),

    #[error("VAD inference failed: {0}")]
    InferenceFailed(String),

    #[error("Invalid audio format: {0}")]
    InvalidAudioFormat(String),
}

/// Result type for VAD operations.
pub type VadResult<T> = Result<T, VadError>;

/// Wrapper around Silero VAD for speech detection.
///
/// This struct manages the VAD model and provides a simple interface
/// for processing audio frames.
pub struct SileroVad {
    vad: VoiceActivityDetector,
    sample_rate: usize,
    frame_size: usize,
}

impl SileroVad {
    /// Create a new SileroVad instance.
    ///
    /// # Arguments
    /// - `sample_rate`: Audio sample rate (8000 or 16000 supported)
    pub fn new(sample_rate: usize) -> VadResult<Self> {
        // Validate sample rate and determine frame size
        let frame_size = match sample_rate {
            8000 => 256,  // Required by Silero VAD V5
            16000 => 512, // Required by Silero VAD V5
            _ => {
                return Err(VadError::InvalidAudioFormat(format!(
                    "Sample rate must be 8000 or 16000, got {}",
                    sample_rate
                )));
            }
        };

        let vad = VoiceActivityDetector::builder()
            .sample_rate(sample_rate as i64)
            .chunk_size(frame_size)
            .build()
            .map_err(|e| {
                VadError::InitializationFailed(format!("Failed to create VAD: {:?}", e))
            })?;

        debug!(
            sample_rate = sample_rate,
            frame_size = frame_size,
            "Initialized Silero VAD v5"
        );

        Ok(Self {
            vad,
            sample_rate,
            frame_size,
        })
    }

    /// Create a new SileroVad instance with custom threshold.
    ///
    /// Note: The threshold is applied during segmentation, not VAD inference.
    /// This method exists for API compatibility.
    ///
    /// # Arguments
    /// - `sample_rate`: Audio sample rate (8000 or 16000 supported)
    /// - `_threshold`: Speech detection threshold (0.0-1.0) - applied in segmenter
    pub fn with_threshold(sample_rate: usize, _threshold: f32) -> VadResult<Self> {
        // The voice_activity_detector crate doesn't support threshold in builder
        // Threshold is applied in the segmenter instead
        Self::new(sample_rate)
    }

    /// Get the expected frame size for this VAD instance.
    ///
    /// Silero VAD V5 has fixed frame sizes:
    /// - 16kHz: 512 samples (32ms)
    /// - 8kHz: 256 samples (32ms)
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Get the frame duration in milliseconds.
    pub fn frame_duration_ms(&self) -> u64 {
        (self.frame_size * 1000 / self.sample_rate) as u64
    }

    /// Analyze a single audio frame and return speech probability.
    ///
    /// # Arguments
    /// - `samples`: Audio samples as f32 in range [-1.0, 1.0]
    ///
    /// # Returns
    /// Speech probability from 0.0 (definitely not speech) to 1.0 (definitely speech)
    pub fn analyze_frame(&mut self, samples: &[f32]) -> VadResult<f32> {
        // The crate will pad/truncate if needed, but warn if size is wrong
        if samples.len() != self.frame_size {
            trace!(
                expected = self.frame_size,
                got = samples.len(),
                "Frame size mismatch (will be padded/truncated)"
            );
        }

        let prob = self.vad.predict(samples.iter().copied());

        trace!(speech_prob = prob, "VAD frame analyzed");

        Ok(prob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_creation() {
        let vad = SileroVad::new(16000);
        assert!(vad.is_ok());
    }

    #[test]
    fn test_invalid_sample_rate() {
        let vad = SileroVad::new(44100);
        assert!(vad.is_err());
    }

    #[test]
    fn test_frame_size() {
        let vad = SileroVad::new(16000).unwrap();
        assert_eq!(vad.frame_size(), 512);

        let vad = SileroVad::new(8000).unwrap();
        assert_eq!(vad.frame_size(), 256);
    }

    #[test]
    fn test_frame_duration() {
        let vad = SileroVad::new(16000).unwrap();
        assert_eq!(vad.frame_duration_ms(), 32);
    }

    #[test]
    fn test_analyze_silence() {
        let mut vad = SileroVad::new(16000).unwrap();
        let silence = vec![0.0f32; vad.frame_size()];
        let prob = vad.analyze_frame(&silence).unwrap();
        assert!(prob < 0.5, "Silence should have low speech probability");
    }
}
