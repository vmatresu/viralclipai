//! Optimized Face Detector
//!
//! Wraps `FaceInferenceEngine` to provide the same interface as `FaceDetector`,
//! enabling seamless switching between legacy and optimized pipelines.
//!
//! # Features
//! - Fixed-resolution letterbox preprocessing (960×540 default)
//! - Temporal decimation (detect every N frames)
//! - Kalman filter tracking for gap frame prediction
//! - Scene-cut aware tracker reset
//! - Zero-allocation hot loop (steady state)
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::{OptimizedFaceDetector, IntelligentCropConfig};
//!
//! let config = IntelligentCropConfig::default();
//! let detector = OptimizedFaceDetector::new(config)?;
//!
//! let detections = detector.detect_in_video(
//!     "video.mp4", 0.0, 60.0, 1920, 1080, 30.0
//! ).await?;
//! ```

use super::config::{FaceEngineMode, IntelligentCropConfig};
use super::face_engine::{EngineMode, FaceEngineConfig, FaceInferenceEngine};
use super::kalman_tracker::KalmanTrackerConfig;
use super::models::{BoundingBox, Detection};
use super::scene_cut::SceneCutConfig;
use super::temporal::TemporalConfig;
use super::tracker::IoUTracker;
use super::yunet;
use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::time::Instant;
use tracing::{info, warn};

#[cfg(feature = "opencv")]
use opencv::{
    core::Mat,
    prelude::*,
    videoio::{VideoCapture, CAP_ANY, CAP_PROP_FRAME_HEIGHT, CAP_PROP_FRAME_WIDTH, CAP_PROP_POS_MSEC},
};

/// Optimized face detector using the new inference engine.
///
/// Provides the same interface as `FaceDetector` but uses the optimized
/// pipeline with temporal decimation and Kalman tracking.
pub struct OptimizedFaceDetector {
    config: IntelligentCropConfig,
    engine_config: FaceEngineConfig,
}

impl OptimizedFaceDetector {
    /// Create a new optimized detector.
    pub fn new(config: IntelligentCropConfig) -> MediaResult<Self> {
        let opt = &config.optimized_engine;

        let engine_config = FaceEngineConfig {
            mode: match config.engine_mode {
                FaceEngineMode::Optimized => EngineMode::Optimized,
                FaceEngineMode::Legacy => EngineMode::Legacy,
            },
            inf_width: opt.inference_width,
            inf_height: opt.inference_height,
            padding_value: opt.padding_value,
            temporal: TemporalConfig {
                detect_every_n: opt.detect_every_n,
                max_gap_frames: opt.detect_every_n * 2,
                ..Default::default()
            },
            tracker: KalmanTrackerConfig::default(),
            scene_cut: SceneCutConfig {
                threshold: opt.scene_cut_threshold,
                ..Default::default()
            },
            confidence_threshold: config.min_detection_confidence,
            log_metrics: true,
        };

        Ok(Self {
            config,
            engine_config,
        })
    }

    /// Create with default configuration.
    pub fn with_defaults() -> MediaResult<Self> {
        Self::new(IntelligentCropConfig::default())
    }

    /// Create in legacy mode (uses original YuNet behavior).
    pub fn legacy(config: IntelligentCropConfig) -> MediaResult<Self> {
        let mut config = config;
        config.engine_mode = FaceEngineMode::Legacy;
        Self::new(config)
    }

    /// Detect faces in a video over a time range.
    ///
    /// Compatible with `FaceDetector::detect_in_video()`.
    #[cfg(feature = "opencv")]
    pub async fn detect_in_video<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
        _fps: f64,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        let video_path = video_path.as_ref();
        let video_path_str = video_path.to_string_lossy();

        // Use legacy path if configured
        if self.config.engine_mode == FaceEngineMode::Legacy {
            return self
                .detect_legacy(video_path, start_time, end_time, width, height)
                .await;
        }

        info!(
            mode = "optimized",
            inf_size = format!(
                "{}x{}",
                self.engine_config.inf_width, self.engine_config.inf_height
            ),
            detect_every_n = self.engine_config.temporal.detect_every_n,
            "Starting optimized face detection"
        );

        let start = Instant::now();

        // Create engine
        let mut engine = FaceInferenceEngine::new(self.engine_config.clone())?;

        // Open video
        let mut cap = VideoCapture::from_file(&video_path_str, CAP_ANY)
            .map_err(|e| MediaError::detection_failed(format!("Failed to open video: {}", e)))?;

        if !cap.is_opened().unwrap_or(false) {
            return Err(MediaError::detection_failed(format!(
                "Failed to open video file: {}",
                video_path_str
            )));
        }

        // Get actual video dimensions (for future use in adaptive scaling)
        let _actual_width = cap.get(CAP_PROP_FRAME_WIDTH).unwrap_or(width as f64) as u32;
        let _actual_height = cap.get(CAP_PROP_FRAME_HEIGHT).unwrap_or(height as f64) as u32;

        let duration = end_time - start_time;
        let sample_interval = 1.0 / self.config.fps_sample;
        let num_samples = (duration / sample_interval).ceil() as usize;
        let max_samples = num_samples.min(360);

        let mut all_detections: Vec<Vec<Detection>> = Vec::with_capacity(max_samples);
        let mut current_time = start_time;

        for frame_idx in 0..max_samples {
            // Seek to current time
            if let Err(e) = cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0) {
                warn!("Failed to seek to {:.2}s: {}", current_time, e);
                all_detections.push(Vec::new());
                current_time += sample_interval;
                continue;
            }

            // Read frame
            let mut frame = Mat::default();
            let success = match cap.read(&mut frame) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to read frame at {:.2}s: {}", current_time, e);
                    all_detections.push(Vec::new());
                    current_time += sample_interval;
                    continue;
                }
            };

            if !success || frame.empty() {
                all_detections.push(Vec::new());
                current_time += sample_interval;
                continue;
            }

            // Process frame through engine
            let timestamp_ms = (current_time * 1000.0) as u64;
            match engine.process_frame(&frame, timestamp_ms) {
                Ok(frame_dets) => {
                    // Convert to Detection format
                    let dets: Vec<Detection> = frame_dets
                        .faces
                        .iter()
                        .map(|face| {
                            Detection::new(
                                current_time,
                                face.bbox,
                                face.confidence,
                                face.track_id,
                            )
                        })
                        .collect();
                    all_detections.push(dets);
                }
                Err(e) => {
                    warn!("Detection failed at frame {}: {}", frame_idx, e);
                    all_detections.push(Vec::new());
                }
            }

            current_time += sample_interval;

            if frame_idx % 40 == 0 {
                let total: usize = all_detections.iter().map(|d| d.len()).sum();
                info!(
                    frame = frame_idx,
                    total_frames = max_samples,
                    total_detections = total,
                    "Optimized detection progress"
                );
            }
        }

        // Log statistics
        let stats = engine.finalize_stats();
        let elapsed = start.elapsed();

        info!(
            frames = stats.frames_processed,
            keyframes = stats.keyframe_count,
            gap_frames = stats.gap_frame_count,
            scene_cuts = stats.scene_cut_count,
            faces = stats.faces_detected,
            throughput_mult = format!("{:.1}x", stats.throughput_multiplier()),
            elapsed_ms = elapsed.as_millis(),
            "Optimized detection complete"
        );

        Ok(all_detections)
    }

    /// Legacy detection path (original YuNet behavior).
    #[cfg(feature = "opencv")]
    async fn detect_legacy<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        info!(mode = "legacy", "Using legacy YuNet detection");

        // Check YuNet availability
        if !yunet::is_yunet_available() {
            if !yunet::ensure_yunet_available().await {
                return Err(MediaError::detection_failed(
                    "YuNet model not available and automatic download failed",
                ));
            }
        }

        // Use existing YuNet detection
        let raw_detections = yunet::detect_faces_with_yunet(
            video_path,
            start_time,
            end_time,
            width,
            height,
            self.config.fps_sample,
        )
        .await?;

        // Convert to tracked detections using IoU tracker
        let mut tracker = IoUTracker::new(self.config.iou_threshold, self.config.max_track_gap);
        let sample_interval = 1.0 / self.config.fps_sample;
        let mut current_time = start_time;
        let mut all_detections = Vec::with_capacity(raw_detections.len());

        for raw_dets in raw_detections {
            // Filter by minimum face size
            let filtered: Vec<(BoundingBox, f64)> = raw_dets
                .into_iter()
                .filter(|(bbox, _)| {
                    let face_area_ratio = bbox.area() / (width as f64 * height as f64);
                    face_area_ratio >= self.config.min_face_size
                })
                .collect();

            // Track faces
            let tracked = tracker.update(&filtered);

            // Convert to Detection
            let frame_dets: Vec<Detection> = tracked
                .into_iter()
                .map(|(track_id, bbox, score)| Detection::new(current_time, bbox, score, track_id))
                .collect();

            all_detections.push(frame_dets);
            current_time += sample_interval;
        }

        Ok(all_detections)
    }

    /// Get configuration.
    pub fn config(&self) -> &IntelligentCropConfig {
        &self.config
    }

    /// Check if using optimized mode.
    pub fn is_optimized(&self) -> bool {
        self.config.engine_mode == FaceEngineMode::Optimized
    }
}

/// Non-OpenCV stub.
#[cfg(not(feature = "opencv"))]
impl OptimizedFaceDetector {
    pub fn new(config: IntelligentCropConfig) -> MediaResult<Self> {
        Err(MediaError::detection_failed("OpenCV feature not enabled"))
    }

    pub fn with_defaults() -> MediaResult<Self> {
        Err(MediaError::detection_failed("OpenCV feature not enabled"))
    }

    pub async fn detect_in_video<P: AsRef<Path>>(
        &self,
        _video_path: P,
        _start_time: f64,
        _end_time: f64,
        _width: u32,
        _height: u32,
        _fps: f64,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        Err(MediaError::detection_failed("OpenCV feature not enabled"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_creation() {
        let config = IntelligentCropConfig::default();
        assert!(config.is_optimized());
    }

    #[test]
    fn test_legacy_mode() {
        let config = IntelligentCropConfig::default().with_legacy_engine();
        assert!(!config.is_optimized());
        assert_eq!(config.engine_mode, FaceEngineMode::Legacy);
    }

    #[test]
    fn test_engine_config_conversion() {
        let config = IntelligentCropConfig::default();
        let detector = OptimizedFaceDetector::new(config.clone());

        // Should succeed if opencv feature is enabled, fail otherwise
        #[cfg(feature = "opencv")]
        {
            assert!(detector.is_ok());
            let detector = detector.unwrap();
            assert!(detector.is_optimized());
        }
    }
}
