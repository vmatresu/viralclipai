//! Face Inference Engine with Optimized Pipeline
//!
//! Unified face detection engine that combines all optimized components:
//! - Fixed-resolution letterbox preprocessing
//! - OpenVINO-first backend selection
//! - Temporal decimation (detect every N frames)
//! - Kalman filter tracking for gap frame interpolation
//! - Scene-cut aware tracker reset
//! - Zero-allocation hot loop with buffer pooling
//!
//! # Backward Compatibility
//! The engine supports switching between the legacy YuNet detector and the
//! new optimized pipeline via `EngineMode`:
//!
//! ```rust
//! // Use optimized pipeline (default)
//! let engine = FaceInferenceEngine::new(config);
//!
//! // Use legacy detector for testing/comparison
//! let config = FaceEngineConfig { mode: EngineMode::Legacy, ..Default::default() };
//! let engine = FaceInferenceEngine::new(config);
//! ```
//!
//! # Architecture
//! ```text
//! Video Frame
//!     │
//!     ▼
//! ┌─────────────────┐
//! │  Scene Cut Det  │ ← Check for scene boundaries
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │ Temporal Decim  │ ← Should we run inference?
//! └────────┬────────┘
//!          │
//!     ┌────┴────┐
//!     │ Keyframe│ Gap
//!     ▼         ▼
//! ┌─────────┐ ┌─────────┐
//! │Letterbox│ │ Kalman  │
//! │+ YuNet  │ │ Predict │
//! └────┬────┘ └────┬────┘
//!      │           │
//!      └─────┬─────┘
//!            ▼
//! ┌─────────────────┐
//! │  Map to Raw     │ ← Inverse coordinate transform
//! └────────┬────────┘
//!          │
//!          ▼
//!    Face Detections
//! ```

use super::backend::{BackendMetrics, BackendSelector, InferenceBackend};
use super::cpu_features::CpuFeatures;
use super::frame_converter::FrameConverter;
use super::kalman_tracker::{KalmanTracker, KalmanTrackerConfig, TrackerStats};
use super::letterbox::Letterboxer;
use super::mapping::{MappingMeta, NormalizedBBox, DEFAULT_INF_HEIGHT, DEFAULT_INF_WIDTH};
use super::models::BoundingBox;
use super::scene_cut::{SceneCutConfig, SceneCutDetector};
use super::temporal::{DecimatorStats, DetectionTrigger, TemporalConfig, TemporalDecimator};
use super::yunet::YuNetDetector;
use crate::error::{MediaError, MediaResult};
use std::time::Instant;
use tracing::{debug, info, warn};

#[cfg(feature = "opencv")]
use opencv::{core::Mat, prelude::*};

/// Engine operation mode for backward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineMode {
    /// Optimized pipeline with letterbox + temporal decimation + Kalman tracking
    #[default]
    Optimized,
    /// Legacy YuNet detector (original implementation, dynamic input size)
    Legacy,
}

impl std::fmt::Display for EngineMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineMode::Optimized => write!(f, "optimized"),
            EngineMode::Legacy => write!(f, "legacy"),
        }
    }
}

/// Configuration for FaceInferenceEngine.
#[derive(Debug, Clone)]
pub struct FaceEngineConfig {
    /// Engine operation mode
    pub mode: EngineMode,
    /// Inference canvas width
    pub inf_width: u32,
    /// Inference canvas height
    pub inf_height: u32,
    /// Padding value for letterbox (0 = black, 128 = gray)
    pub padding_value: u8,
    /// Temporal decimation config
    pub temporal: TemporalConfig,
    /// Kalman tracker config
    pub tracker: KalmanTrackerConfig,
    /// Scene cut detection config
    pub scene_cut: SceneCutConfig,
    /// Minimum confidence threshold for detections
    pub confidence_threshold: f64,
    /// Log detailed metrics
    pub log_metrics: bool,
}

impl Default for FaceEngineConfig {
    fn default() -> Self {
        Self {
            mode: EngineMode::Optimized,
            inf_width: DEFAULT_INF_WIDTH,
            inf_height: DEFAULT_INF_HEIGHT,
            padding_value: 0, // YuNet expects black padding
            temporal: TemporalConfig::default(),
            tracker: KalmanTrackerConfig::default(),
            scene_cut: SceneCutConfig::default(),
            confidence_threshold: 0.3,
            log_metrics: true,
        }
    }
}

impl FaceEngineConfig {
    /// Create config for legacy mode (backward compatibility).
    pub fn legacy() -> Self {
        Self {
            mode: EngineMode::Legacy,
            ..Default::default()
        }
    }

    /// Create config for fast processing (more gap frames).
    pub fn fast() -> Self {
        Self {
            temporal: TemporalConfig::fast(),
            ..Default::default()
        }
    }

    /// Create config for quality processing (more detections).
    pub fn quality() -> Self {
        Self {
            temporal: TemporalConfig::quality(),
            ..Default::default()
        }
    }

    /// Create config optimized for YouTube content (16:9).
    pub fn youtube() -> Self {
        Self {
            inf_width: 960,
            inf_height: 540,
            ..Default::default()
        }
    }
}

/// Statistics and metrics from engine operation.
#[derive(Debug, Clone, Default)]
pub struct EngineStats {
    /// Total frames processed
    pub frames_processed: u64,
    /// Keyframe detections run
    pub keyframe_count: u64,
    /// Gap frames predicted
    pub gap_frame_count: u64,
    /// Scene cuts detected
    pub scene_cut_count: u64,
    /// Total faces detected
    pub faces_detected: u64,
    /// Total inference time (ms)
    pub total_inference_time_ms: u64,
    /// Average inference time per keyframe (ms)
    pub avg_inference_time_ms: f64,
    /// Peak inference time (ms)
    pub peak_inference_time_ms: u64,
    /// Tracker statistics
    pub tracker_stats: TrackerStats,
    /// Decimator statistics
    pub decimator_stats: DecimatorStats,
    /// Backend used
    pub backend: String,
    /// CPU tier detected
    pub cpu_tier: String,
}

impl EngineStats {
    /// Calculate throughput multiplier from decimation.
    pub fn throughput_multiplier(&self) -> f64 {
        let total = self.keyframe_count + self.gap_frame_count;
        if self.keyframe_count > 0 {
            total as f64 / self.keyframe_count as f64
        } else {
            1.0
        }
    }

    /// Log summary statistics.
    pub fn log_summary(&self) {
        info!(
            frames = self.frames_processed,
            keyframes = self.keyframe_count,
            gap_frames = self.gap_frame_count,
            scene_cuts = self.scene_cut_count,
            faces = self.faces_detected,
            avg_inference_ms = format!("{:.2}", self.avg_inference_time_ms),
            throughput_mult = format!("{:.1}x", self.throughput_multiplier()),
            backend = %self.backend,
            cpu_tier = %self.cpu_tier,
            "Face engine statistics"
        );
    }
}

/// Face detection result for a single frame from FaceInferenceEngine.
#[derive(Debug, Clone)]
pub struct FaceFrameResult {
    /// Frame index
    pub frame_idx: u64,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Detection trigger type (None for gap frames)
    pub trigger: Option<DetectionTrigger>,
    /// Detected/tracked faces with track IDs
    pub faces: Vec<TrackedFace>,
    /// Scene hash for this frame
    pub scene_hash: u64,
    /// Whether this was a keyframe (full inference)
    pub is_keyframe: bool,
}

/// A tracked face with identity and confidence.
#[derive(Debug, Clone)]
pub struct TrackedFace {
    /// Unique track ID (persistent across frames)
    pub track_id: u32,
    /// Bounding box in raw frame coordinates
    pub bbox: BoundingBox,
    /// Detection/tracking confidence
    pub confidence: f64,
    /// Normalized bounding box (0-1 coordinates)
    pub bbox_normalized: NormalizedBBox,
}

/// Unified face detection engine.
///
/// Provides optimized face detection with temporal decimation, Kalman tracking,
/// and scene-cut awareness. Supports legacy mode for backward compatibility.
#[cfg(feature = "opencv")]
pub struct FaceInferenceEngine {
    config: FaceEngineConfig,
    /// YuNet detector instance
    detector: Option<YuNetDetector>,
    /// Letterboxer for preprocessing
    letterboxer: Letterboxer,
    /// Frame converter with buffer pooling
    frame_converter: FrameConverter,
    /// Kalman tracker for gap frame interpolation
    tracker: KalmanTracker,
    /// Temporal decimator
    decimator: TemporalDecimator,
    /// Scene cut detector
    scene_cut: SceneCutDetector,
    /// Current mapping metadata
    current_mapping: Option<MappingMeta>,
    /// Backend selection result
    backend_metrics: Option<BackendMetrics>,
    /// Engine statistics
    stats: EngineStats,
    /// Frame dimensions (raw)
    raw_dims: Option<(u32, u32)>,
    /// Frame counter
    frame_count: u64,
}

#[cfg(feature = "opencv")]
impl FaceInferenceEngine {
    /// Create a new face inference engine.
    ///
    /// Initializes the optimal backend (OpenVINO > OpenCV DNN) and
    /// pre-allocates buffers for zero-allocation hot loop.
    pub fn new(config: FaceEngineConfig) -> MediaResult<Self> {
        info!(
            mode = %config.mode,
            inf_size = format!("{}x{}", config.inf_width, config.inf_height),
            "Initializing face inference engine"
        );

        if tuned_guard_enabled() {
            CpuFeatures::verify_tuned_requirements()
                .map_err(|e| MediaError::internal(e.to_string()))?;
        } else {
            CpuFeatures::detect().log_capabilities();
        }

        // Select optimal backend
        let backend_metrics = match BackendSelector::select_optimal(
            config.inf_width as i32,
            config.inf_height as i32,
        ) {
            Ok((_, metrics)) => {
                metrics.log();
                Some(metrics)
            }
            Err(e) => {
                warn!("Backend selection failed, will use default: {}", e);
                None
            }
        };

        let letterboxer = Letterboxer::new(config.inf_width as i32, config.inf_height as i32)
            .with_padding_value(config.padding_value);

        let frame_converter =
            FrameConverter::new(config.inf_width as i32, config.inf_height as i32)
                .with_padding_value(config.padding_value);

        let tracker = KalmanTracker::with_config(config.tracker.clone());
        let decimator = TemporalDecimator::new(config.temporal.clone());
        let scene_cut = SceneCutDetector::with_config(config.scene_cut.clone());

        let mut stats = EngineStats::default();
        if let Some(ref metrics) = backend_metrics {
            stats.backend = metrics.backend.to_string();
            stats.cpu_tier = metrics.cpu_tier.to_string();
        }

        Ok(Self {
            config,
            detector: None, // Lazily initialized on first frame
            letterboxer,
            frame_converter,
            tracker,
            decimator,
            scene_cut,
            current_mapping: None,
            backend_metrics,
            stats,
            raw_dims: None,
            frame_count: 0,
        })
    }

    /// Create with default configuration.
    pub fn with_defaults() -> MediaResult<Self> {
        Self::new(FaceEngineConfig::default())
    }

    /// Create in legacy mode for backward compatibility.
    pub fn legacy() -> MediaResult<Self> {
        Self::new(FaceEngineConfig::legacy())
    }

    /// Process a frame and return face detections.
    ///
    /// In optimized mode:
    /// - Checks for scene cuts
    /// - Decides whether to run full inference (keyframe) or predict (gap)
    /// - Returns tracked faces with persistent IDs
    ///
    /// In legacy mode:
    /// - Runs YuNet detection on every frame
    /// - No tracking, IDs are frame-local
    pub fn process_frame(
        &mut self,
        frame: &Mat,
        timestamp_ms: u64,
    ) -> MediaResult<FaceFrameResult> {
        match self.config.mode {
            EngineMode::Optimized => self.process_optimized(frame, timestamp_ms),
            EngineMode::Legacy => self.process_legacy(frame, timestamp_ms),
        }
    }

    /// Process frame using optimized pipeline.
    fn process_optimized(
        &mut self,
        frame: &Mat,
        timestamp_ms: u64,
    ) -> MediaResult<FaceFrameResult> {
        if frame.empty() {
            return Err(MediaError::detection_failed("Empty frame"));
        }

        let frame_idx = self.frame_count;
        self.frame_count += 1;
        self.stats.frames_processed += 1;

        // Ensure detector is initialized
        self.ensure_detector(frame.cols() as u32, frame.rows() as u32)?;

        // Check for scene cut
        let scene_hash = self.scene_cut.compute_scene_hash(frame);
        let is_scene_cut = self.scene_cut.check_frame(frame);

        if is_scene_cut {
            self.stats.scene_cut_count += 1;
            self.decimator.notify_scene_cut(scene_hash);
            self.tracker.handle_scene_cut(scene_hash);
        }

        // Get tracker state for decimation decision
        let tracker_confidence = self.tracker.min_confidence();
        let active_tracks = self.tracker.active_count();

        // Decide: keyframe or gap frame?
        let trigger = self
            .decimator
            .should_detect(tracker_confidence, active_tracks, timestamp_ms);

        let (faces, is_keyframe) = if let Some(trigger) = trigger {
            // KEYFRAME: Run full inference
            let start = Instant::now();

            let detections = self.detect_keyframe(frame)?;
            let faces = self.tracker.update(&detections, timestamp_ms, scene_hash);

            let elapsed_ms = start.elapsed().as_millis() as u64;
            self.stats.total_inference_time_ms += elapsed_ms;
            self.stats.keyframe_count += 1;
            self.stats.peak_inference_time_ms = self.stats.peak_inference_time_ms.max(elapsed_ms);

            debug!(
                frame = frame_idx,
                trigger = %trigger,
                detections = detections.len(),
                tracks = faces.len(),
                time_ms = elapsed_ms,
                "Keyframe detection"
            );

            (faces, true)
        } else {
            // GAP FRAME: Predict using Kalman tracker
            let faces = self.tracker.predict(timestamp_ms);
            self.stats.gap_frame_count += 1;

            debug!(
                frame = frame_idx,
                tracks = faces.len(),
                "Gap frame prediction"
            );

            (faces, false)
        };

        // Convert to TrackedFace with coordinate mapping
        let mapping = self
            .current_mapping
            .as_ref()
            .ok_or_else(|| MediaError::detection_failed("No mapping available"))?;

        let tracked_faces: Vec<TrackedFace> = faces
            .into_iter()
            .map(|(track_id, bbox, confidence)| {
                let bbox_normalized = mapping.normalize(&bbox);
                TrackedFace {
                    track_id,
                    bbox,
                    confidence,
                    bbox_normalized,
                }
            })
            .collect();

        self.stats.faces_detected += tracked_faces.len() as u64;

        Ok(FaceFrameResult {
            frame_idx,
            timestamp_ms,
            trigger,
            faces: tracked_faces,
            scene_hash,
            is_keyframe,
        })
    }

    /// Detect faces on keyframe with letterboxing.
    fn detect_keyframe(&mut self, frame: &Mat) -> MediaResult<Vec<(BoundingBox, f64)>> {
        let detector = self
            .detector
            .as_mut()
            .ok_or_else(|| MediaError::detection_failed("Detector not initialized"))?;

        // Letterbox the frame
        let (letterboxed, meta) = self.letterboxer.process(frame)?;
        self.current_mapping = Some(meta);

        // Run detection on letterboxed frame
        let mut detections = detector.detect_in_frame(letterboxed)?;

        // Map coordinates back to raw frame space
        let mapping = self.current_mapping.as_ref().unwrap();
        for (bbox, _) in &mut detections {
            *bbox = mapping.map_rect(bbox);
        }

        Ok(detections)
    }

    /// Process frame using legacy pipeline (for backward compatibility).
    fn process_legacy(&mut self, frame: &Mat, timestamp_ms: u64) -> MediaResult<FaceFrameResult> {
        if frame.empty() {
            return Err(MediaError::detection_failed("Empty frame"));
        }

        let frame_idx = self.frame_count;
        self.frame_count += 1;
        self.stats.frames_processed += 1;
        self.stats.keyframe_count += 1;

        // Ensure detector is initialized
        self.ensure_detector(frame.cols() as u32, frame.rows() as u32)?;

        let start = Instant::now();

        // Direct detection without letterboxing (legacy behavior)
        let detector = self
            .detector
            .as_mut()
            .ok_or_else(|| MediaError::detection_failed("Detector not initialized"))?;
        let detections = detector.detect_in_frame(frame)?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.stats.total_inference_time_ms += elapsed_ms;
        self.stats.peak_inference_time_ms = self.stats.peak_inference_time_ms.max(elapsed_ms);

        // Create mapping for coordinate normalization
        let raw_width = frame.cols() as u32;
        let raw_height = frame.rows() as u32;
        let mapping = MappingMeta::with_defaults(raw_width, raw_height);
        self.current_mapping = Some(mapping);

        // Convert to TrackedFace (no tracking in legacy mode, IDs are sequential)
        let tracked_faces: Vec<TrackedFace> = detections
            .into_iter()
            .enumerate()
            .map(|(idx, (bbox, confidence))| {
                let bbox_normalized = mapping.normalize(&bbox);
                TrackedFace {
                    track_id: idx as u32,
                    bbox,
                    confidence,
                    bbox_normalized,
                }
            })
            .collect();

        self.stats.faces_detected += tracked_faces.len() as u64;

        Ok(FaceFrameResult {
            frame_idx,
            timestamp_ms,
            trigger: Some(DetectionTrigger::Keyframe),
            faces: tracked_faces,
            scene_hash: 0,
            is_keyframe: true,
        })
    }

    /// Ensure detector is initialized with correct dimensions.
    fn ensure_detector(&mut self, raw_width: u32, raw_height: u32) -> MediaResult<()> {
        let needs_init = match self.raw_dims {
            Some((w, h)) => w != raw_width || h != raw_height,
            None => true,
        };

        if needs_init {
            info!(
                "Initializing YuNet detector for {}x{} frames",
                raw_width, raw_height
            );

            // For optimized mode, use letterbox dimensions
            // For legacy mode, use raw dimensions
            let (det_width, det_height) = match self.config.mode {
                EngineMode::Optimized => (self.config.inf_width, self.config.inf_height),
                EngineMode::Legacy => (raw_width, raw_height),
            };

            let detector = YuNetDetector::new(det_width, det_height)?;
            self.detector = Some(detector);
            self.raw_dims = Some((raw_width, raw_height));

            // Update mapping
            self.current_mapping = Some(MappingMeta::for_yunet(
                raw_width,
                raw_height,
                self.config.inf_width,
                self.config.inf_height,
            ));
        }

        Ok(())
    }

    /// Reset engine state for a new video.
    pub fn reset(&mut self) {
        self.tracker.hard_reset();
        self.decimator.reset();
        self.scene_cut.reset();
        self.frame_count = 0;
        self.current_mapping = None;
        debug!("Face engine reset for new video");
    }

    /// Get current statistics.
    pub fn stats(&self) -> &EngineStats {
        &self.stats
    }

    /// Finalize and get comprehensive statistics.
    pub fn finalize_stats(&mut self) -> EngineStats {
        self.stats.tracker_stats = self.tracker.stats();
        self.stats.decimator_stats = self.decimator.stats().clone();

        // Calculate average inference time
        if self.stats.keyframe_count > 0 {
            self.stats.avg_inference_time_ms =
                self.stats.total_inference_time_ms as f64 / self.stats.keyframe_count as f64;
        }

        self.stats.clone()
    }

    /// Log summary statistics.
    pub fn log_summary(&mut self) {
        let stats = self.finalize_stats();
        stats.log_summary();
    }

    /// Get current engine mode.
    pub fn mode(&self) -> EngineMode {
        self.config.mode
    }

    /// Get inference dimensions.
    pub fn inference_dims(&self) -> (u32, u32) {
        (self.config.inf_width, self.config.inf_height)
    }

    /// Get backend being used.
    pub fn backend(&self) -> Option<InferenceBackend> {
        self.backend_metrics.as_ref().map(|m| m.backend)
    }
}

fn tuned_guard_enabled() -> bool {
    match std::env::var("VCLIP_TUNED_BUILD") {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            value.is_empty()
                || matches!(value.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

/// Non-OpenCV stub.
#[cfg(not(feature = "opencv"))]
pub struct FaceInferenceEngine {
    config: FaceEngineConfig,
}

#[cfg(not(feature = "opencv"))]
impl FaceInferenceEngine {
    pub fn new(config: FaceEngineConfig) -> MediaResult<Self> {
        Ok(Self { config })
    }

    pub fn with_defaults() -> MediaResult<Self> {
        Self::new(FaceEngineConfig::default())
    }

    pub fn legacy() -> MediaResult<Self> {
        Self::new(FaceEngineConfig::legacy())
    }

    pub fn mode(&self) -> EngineMode {
        self.config.mode
    }

    pub fn inference_dims(&self) -> (u32, u32) {
        (self.config.inf_width, self.config.inf_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_mode_display() {
        assert_eq!(format!("{}", EngineMode::Optimized), "optimized");
        assert_eq!(format!("{}", EngineMode::Legacy), "legacy");
    }

    #[test]
    fn test_config_default() {
        let config = FaceEngineConfig::default();
        assert_eq!(config.mode, EngineMode::Optimized);
        assert_eq!(config.inf_width, DEFAULT_INF_WIDTH);
        assert_eq!(config.inf_height, DEFAULT_INF_HEIGHT);
        assert_eq!(config.padding_value, 0);
    }

    #[test]
    fn test_config_legacy() {
        let config = FaceEngineConfig::legacy();
        assert_eq!(config.mode, EngineMode::Legacy);
    }

    #[test]
    fn test_config_youtube() {
        let config = FaceEngineConfig::youtube();
        assert_eq!(config.inf_width, 960);
        assert_eq!(config.inf_height, 540);
    }

    #[test]
    fn test_stats_throughput() {
        let mut stats = EngineStats::default();
        stats.keyframe_count = 10;
        stats.gap_frame_count = 40;

        let throughput = stats.throughput_multiplier();
        assert!((throughput - 5.0).abs() < 0.01);
    }
}
