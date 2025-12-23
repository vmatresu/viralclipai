//! Temporal Decimation for Face Detection
//!
//! Implements keyframe-based detection with tracking interpolation to reduce
//! inference overhead while maintaining smooth face tracking.
//!
//! # Strategy
//! - Run full YuNet inference on **keyframes** (every N frames or on triggers)
//! - Use Kalman filter prediction for **gap frames** (no inference)
//! - Force re-detection on scene cuts, confidence drops, or track loss
//!
//! # Performance Impact
//! With N=5 (detect every 5 frames), effective throughput increases ~5x
//! while maintaining <50ms detection latency on keyframes.
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::temporal::{TemporalConfig, TemporalDecimator};
//!
//! let config = TemporalConfig::default();
//! let mut decimator = TemporalDecimator::new(config);
//!
//! for frame_idx in 0..1000 {
//!     if decimator.should_detect(frame_idx, tracker_confidence) {
//!         // Run YuNet detection
//!     } else {
//!         // Use Kalman prediction
//!     }
//! }
//! ```

use std::time::Duration;
use tracing::{debug, info};

/// Configuration for temporal decimation behavior.
#[derive(Debug, Clone)]
pub struct TemporalConfig {
    /// Run full detection every N frames (default: 5)
    pub detect_every_n: u32,

    /// Alternative: time-based interval (takes precedence if set)
    pub detect_interval: Option<Duration>,

    /// Scene cut detection threshold (0.0-1.0, default: 0.3)
    /// Lower = more sensitive to scene changes
    pub scene_cut_threshold: f64,

    /// Minimum tracker confidence before forcing re-detection
    pub min_confidence: f64,

    /// Position drift threshold as fraction of frame width
    /// If predicted position drifts > this from last detection, force re-detect
    pub drift_threshold: f64,

    /// Maximum frames without detection before declaring track lost
    pub max_gap_frames: u32,

    /// Minimum frames between forced re-detections (cooldown)
    pub min_detection_interval: u32,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            detect_every_n: 5,
            detect_interval: None,
            scene_cut_threshold: 0.3,
            min_confidence: 0.4,
            drift_threshold: 0.15,
            max_gap_frames: 30,
            min_detection_interval: 2,
        }
    }
}

impl TemporalConfig {
    /// Fast config for real-time processing (more gap frames).
    pub fn fast() -> Self {
        Self {
            detect_every_n: 10,
            min_confidence: 0.5,
            max_gap_frames: 50,
            ..Default::default()
        }
    }

    /// Quality config for offline processing (more detections).
    pub fn quality() -> Self {
        Self {
            detect_every_n: 3,
            min_confidence: 0.3,
            max_gap_frames: 15,
            ..Default::default()
        }
    }

    /// Time-based detection at specified interval.
    pub fn with_interval(interval_ms: u64) -> Self {
        Self {
            detect_interval: Some(Duration::from_millis(interval_ms)),
            ..Default::default()
        }
    }
}

/// Reason for triggering a detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionTrigger {
    /// Regular keyframe interval reached
    Keyframe,
    /// Scene cut detected
    SceneCut,
    /// Tracker confidence too low
    LowConfidence,
    /// Position drifted too far
    PositionDrift,
    /// All tracks lost
    NoTracks,
    /// First frame of sequence
    FirstFrame,
    /// Time interval reached
    TimeInterval,
    /// Track ID swap risk detected
    SwapRisk,
}

impl std::fmt::Display for DetectionTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectionTrigger::Keyframe => write!(f, "keyframe"),
            DetectionTrigger::SceneCut => write!(f, "scene_cut"),
            DetectionTrigger::LowConfidence => write!(f, "low_confidence"),
            DetectionTrigger::PositionDrift => write!(f, "position_drift"),
            DetectionTrigger::NoTracks => write!(f, "no_tracks"),
            DetectionTrigger::FirstFrame => write!(f, "first_frame"),
            DetectionTrigger::TimeInterval => write!(f, "time_interval"),
            DetectionTrigger::SwapRisk => write!(f, "swap_risk"),
        }
    }
}

/// Temporal decimator state machine.
///
/// Tracks frame counts and determines when to run full detection
/// versus relying on tracker predictions.
pub struct TemporalDecimator {
    config: TemporalConfig,
    /// Total frames processed
    frame_count: u64,
    /// Frame index of last detection
    last_detection_frame: u64,
    /// Timestamp of last detection (for time-based mode)
    last_detection_time_ms: u64,
    /// Current scene hash for cut detection
    current_scene_hash: u64,
    /// Whether scene cut occurred on current frame
    scene_cut_pending: bool,
    /// Statistics
    stats: DecimatorStats,
}

/// Statistics for monitoring decimation performance.
#[derive(Debug, Clone, Default)]
pub struct DecimatorStats {
    /// Total keyframe detections
    pub keyframe_count: u64,
    /// Total gap frame predictions
    pub gap_frame_count: u64,
    /// Scene cuts detected
    pub scene_cut_count: u64,
    /// Low confidence re-detections
    pub low_confidence_count: u64,
    /// No-track re-detections
    pub no_track_count: u64,
}

impl DecimatorStats {
    /// Calculate decimation ratio (gap frames / total frames).
    pub fn decimation_ratio(&self) -> f64 {
        let total = self.keyframe_count + self.gap_frame_count;
        if total > 0 {
            self.gap_frame_count as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Calculate effective throughput multiplier.
    pub fn throughput_multiplier(&self) -> f64 {
        let total = self.keyframe_count + self.gap_frame_count;
        if self.keyframe_count > 0 {
            total as f64 / self.keyframe_count as f64
        } else {
            1.0
        }
    }
}

impl TemporalDecimator {
    /// Create a new temporal decimator with given configuration.
    pub fn new(config: TemporalConfig) -> Self {
        Self {
            config,
            frame_count: 0,
            last_detection_frame: 0,
            last_detection_time_ms: 0,
            current_scene_hash: 0,
            scene_cut_pending: false,
            stats: DecimatorStats::default(),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(TemporalConfig::default())
    }

    /// Check if current frame should trigger a detection.
    ///
    /// # Arguments
    /// * `tracker_confidence` - Minimum confidence across active tracks (0.0-1.0)
    /// * `active_track_count` - Number of currently active tracks
    /// * `timestamp_ms` - Current frame timestamp in milliseconds
    ///
    /// # Returns
    /// `Some(trigger)` if detection should run, `None` for gap frame
    pub fn should_detect(
        &mut self,
        tracker_confidence: f64,
        active_track_count: usize,
        timestamp_ms: u64,
    ) -> Option<DetectionTrigger> {
        let frame_idx = self.frame_count;
        self.frame_count += 1;

        // First frame always detects
        if frame_idx == 0 {
            self.record_detection(timestamp_ms);
            return Some(DetectionTrigger::FirstFrame);
        }

        // Check for pending scene cut
        if self.scene_cut_pending {
            self.scene_cut_pending = false;
            self.record_detection(timestamp_ms);
            self.stats.scene_cut_count += 1;
            return Some(DetectionTrigger::SceneCut);
        }

        // Check for no active tracks
        if active_track_count == 0 {
            if self.frames_since_detection() >= self.config.min_detection_interval as u64 {
                self.record_detection(timestamp_ms);
                self.stats.no_track_count += 1;
                return Some(DetectionTrigger::NoTracks);
            }
        }

        // Check for low confidence
        if tracker_confidence < self.config.min_confidence {
            if self.frames_since_detection() >= self.config.min_detection_interval as u64 {
                self.record_detection(timestamp_ms);
                self.stats.low_confidence_count += 1;
                return Some(DetectionTrigger::LowConfidence);
            }
        }

        // Check for time-based interval
        if let Some(interval) = self.config.detect_interval {
            let elapsed_ms = timestamp_ms.saturating_sub(self.last_detection_time_ms);
            if elapsed_ms >= interval.as_millis() as u64 {
                self.record_detection(timestamp_ms);
                self.stats.keyframe_count += 1;
                return Some(DetectionTrigger::TimeInterval);
            }
        }

        // Check for frame-based keyframe
        if self.frames_since_detection() >= self.config.detect_every_n as u64 {
            self.record_detection(timestamp_ms);
            self.stats.keyframe_count += 1;
            return Some(DetectionTrigger::Keyframe);
        }

        // Gap frame - use tracker prediction
        self.stats.gap_frame_count += 1;
        None
    }

    /// Notify the decimator of a scene cut.
    ///
    /// Call this when scene cut detection identifies a cut.
    /// The next `should_detect` call will return `SceneCut`.
    pub fn notify_scene_cut(&mut self, new_scene_hash: u64) {
        if self.current_scene_hash != 0 && self.current_scene_hash != new_scene_hash {
            info!(
                old_hash = self.current_scene_hash,
                new_hash = new_scene_hash,
                "Scene cut detected, pending re-detection"
            );
            self.scene_cut_pending = true;
        }
        self.current_scene_hash = new_scene_hash;
    }

    /// Notify of potential track swap risk.
    ///
    /// Call when tracks are overlapping and IDs might swap.
    pub fn notify_swap_risk(&mut self) {
        // Force detection on next frame
        self.last_detection_frame = 0;
    }

    /// Get current statistics.
    pub fn stats(&self) -> &DecimatorStats {
        &self.stats
    }

    /// Get configuration.
    pub fn config(&self) -> &TemporalConfig {
        &self.config
    }

    /// Get current frame count.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Reset state for new video.
    pub fn reset(&mut self) {
        self.frame_count = 0;
        self.last_detection_frame = 0;
        self.last_detection_time_ms = 0;
        self.current_scene_hash = 0;
        self.scene_cut_pending = false;
        self.stats = DecimatorStats::default();
    }

    /// Log summary statistics.
    pub fn log_summary(&self) {
        info!(
            keyframes = self.stats.keyframe_count,
            gap_frames = self.stats.gap_frame_count,
            scene_cuts = self.stats.scene_cut_count,
            decimation_ratio = format!("{:.1}%", self.stats.decimation_ratio() * 100.0),
            throughput_mult = format!("{:.1}x", self.stats.throughput_multiplier()),
            "Temporal decimation summary"
        );
    }

    fn frames_since_detection(&self) -> u64 {
        self.frame_count.saturating_sub(self.last_detection_frame)
    }

    fn record_detection(&mut self, timestamp_ms: u64) {
        self.last_detection_frame = self.frame_count;
        self.last_detection_time_ms = timestamp_ms;
        debug!(
            frame = self.frame_count,
            timestamp_ms = timestamp_ms,
            "Detection recorded"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TemporalConfig::default();
        assert_eq!(config.detect_every_n, 5);
        assert!(config.detect_interval.is_none());
    }

    #[test]
    fn test_first_frame_always_detects() {
        let mut decimator = TemporalDecimator::with_defaults();
        let trigger = decimator.should_detect(1.0, 0, 0);
        assert_eq!(trigger, Some(DetectionTrigger::FirstFrame));
    }

    #[test]
    fn test_keyframe_interval() {
        let config = TemporalConfig {
            detect_every_n: 3,
            ..Default::default()
        };
        let mut decimator = TemporalDecimator::new(config);

        // Frame 0: first frame
        assert!(decimator.should_detect(1.0, 1, 0).is_some());
        // Frames 1, 2: gap
        assert!(decimator.should_detect(1.0, 1, 33).is_none());
        assert!(decimator.should_detect(1.0, 1, 66).is_none());
        // Frame 3: keyframe
        let trigger = decimator.should_detect(1.0, 1, 100);
        assert_eq!(trigger, Some(DetectionTrigger::Keyframe));
    }

    #[test]
    fn test_scene_cut_trigger() {
        let mut decimator = TemporalDecimator::with_defaults();

        // First frame
        decimator.should_detect(1.0, 1, 0);
        decimator.current_scene_hash = 12345;

        // Notify scene cut
        decimator.notify_scene_cut(67890);

        // Next frame should trigger scene cut
        let trigger = decimator.should_detect(1.0, 1, 33);
        assert_eq!(trigger, Some(DetectionTrigger::SceneCut));
    }

    #[test]
    fn test_no_tracks_trigger() {
        let config = TemporalConfig {
            min_detection_interval: 1,
            ..Default::default()
        };
        let mut decimator = TemporalDecimator::new(config);

        // First frame
        decimator.should_detect(1.0, 1, 0);
        // Gap frame
        decimator.should_detect(1.0, 1, 33);
        // No tracks - should trigger
        let trigger = decimator.should_detect(1.0, 0, 66);
        assert_eq!(trigger, Some(DetectionTrigger::NoTracks));
    }

    #[test]
    fn test_low_confidence_trigger() {
        let config = TemporalConfig {
            min_confidence: 0.5,
            min_detection_interval: 1,
            ..Default::default()
        };
        let mut decimator = TemporalDecimator::new(config);

        // First frame
        decimator.should_detect(1.0, 1, 0);
        // Gap
        decimator.should_detect(1.0, 1, 33);
        // Low confidence
        let trigger = decimator.should_detect(0.3, 1, 66);
        assert_eq!(trigger, Some(DetectionTrigger::LowConfidence));
    }

    #[test]
    fn test_statistics() {
        let config = TemporalConfig {
            detect_every_n: 5,
            ..Default::default()
        };
        let mut decimator = TemporalDecimator::new(config);

        // Process 20 frames
        for i in 0..20 {
            decimator.should_detect(1.0, 1, i * 33);
        }

        let stats = decimator.stats();
        // First frame + keyframes at 5, 10, 15 = 4 keyframes
        // But first frame is counted separately
        assert!(stats.keyframe_count >= 3);
        assert!(stats.gap_frame_count > 10);
        assert!(stats.decimation_ratio() > 0.5);
    }

    #[test]
    fn test_time_based_detection() {
        let config = TemporalConfig::with_interval(100); // 100ms
        let mut decimator = TemporalDecimator::new(config);

        // First frame at t=0
        assert!(decimator.should_detect(1.0, 1, 0).is_some());
        // t=50ms - gap
        assert!(decimator.should_detect(1.0, 1, 50).is_none());
        // t=100ms - should trigger
        let trigger = decimator.should_detect(1.0, 1, 100);
        assert_eq!(trigger, Some(DetectionTrigger::TimeInterval));
    }
}
