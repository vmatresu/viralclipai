//! MotionAware tier pipeline - visual motion heuristics.
//!
//! Uses optical flow / frame differencing to detect motion centers.
//! No neural network inference required - fast and efficient.

use async_trait::async_trait;
use std::path::Path;
use tracing::{debug, info};
use vclip_models::DetectionTier;

use crate::detection::pipeline::{DetectionPipeline, DetectionResult, FrameResult};
use crate::error::MediaResult;
use crate::intelligent::models::{BoundingBox, Detection};
use crate::intelligent::IntelligentCropConfig;
use crate::probe::probe_video;

/// Decay window for motion coasting (seconds).
const MOTION_DECAY_SECONDS: f64 = 2.0;

/// Pipeline for `DetectionTier::MotionAware` - motion heuristics for high-motion content.
///
/// Creates synthetic "face" detections around motion centers. Useful for content
/// where faces may not be visible or where motion is the primary subject.
pub struct MotionAwarePipeline {
    config: IntelligentCropConfig,
}

impl MotionAwarePipeline {
    pub fn new() -> Self {
        Self {
            config: IntelligentCropConfig::default(),
        }
    }

    /// Detect motion centers and convert to synthetic detections.
    #[cfg(feature = "opencv")]
    fn detect_motion_tracks(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        use crate::intelligent::motion::MotionDetector;
        use opencv::prelude::{MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
        use opencv::videoio::{VideoCapture, CAP_ANY, CAP_PROP_POS_MSEC};

        let mut cap = VideoCapture::from_file(video_path.to_str().unwrap_or(""), CAP_ANY)
            .map_err(|e| crate::error::MediaError::detection_failed(format!("Open video: {e}")))?;

        if !cap.is_opened().unwrap_or(false) {
            return Err(crate::error::MediaError::detection_failed(
                "Failed to open video for motion analysis",
            ));
        }

        let mut detector = MotionDetector::new(width as i32, height as i32);
        let sample_interval = 1.0 / self.config.fps_sample.max(1e-3);
        let mut frames = Vec::new();
        let mut current_time = start_time;

        // Coasting state for smooth motion tracking
        let mut last_detection: Option<Detection> = None;
        let mut last_seen_time: Option<f64> = None;

        while current_time < end_time {
            cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Seek: {e}")))?;

            let mut frame = opencv::core::Mat::default();
            let read_ok = cap
                .read(&mut frame)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Read: {e}")))?;

            if !read_ok || frame.empty() {
                frames.push(Vec::new());
                current_time += sample_interval;
                continue;
            }

            let detection = detector.detect_center(&frame)?.map(|center| {
                // Use moderate box size around motion center
                let size = (width.min(height) as f64 * 0.35).max(64.0);
                let bbox = BoundingBox::new(
                    center.x as f64 - size / 2.0,
                    center.y as f64 - size / 2.0,
                    size,
                    size,
                )
                .clamp(width, height);

                Detection::new(current_time, bbox, 1.0, 1)
            });

            // Apply coasting: hold last valid motion target for decay window
            let frame_dets = match detection {
                Some(det) => {
                    last_seen_time = Some(current_time);
                    last_detection = Some(det.clone());
                    vec![det]
                }
                None => {
                    if let (Some(last_det), Some(last_time)) = (&last_detection, last_seen_time) {
                        if current_time - last_time <= MOTION_DECAY_SECONDS {
                            let mut held = last_det.clone();
                            held.time = current_time;
                            vec![held]
                        } else {
                            last_detection = None;
                            last_seen_time = None;
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                }
            };

            frames.push(frame_dets);
            current_time += sample_interval;
        }

        Ok(frames)
    }

    #[cfg(not(feature = "opencv"))]
    fn detect_motion_tracks(
        &self,
        _video_path: &Path,
        _start_time: f64,
        _end_time: f64,
        _width: u32,
        _height: u32,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        // Without OpenCV, return empty detections (will use center fallback)
        Ok(Vec::new())
    }
}

impl Default for MotionAwarePipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DetectionPipeline for MotionAwarePipeline {
    async fn analyze(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
    ) -> MediaResult<DetectionResult> {
        let video_info = probe_video(video_path).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = end_time - start_time;

        debug!(
            "MotionAware pipeline analyzing {}x{} @ {:.2}fps, {:.2}s",
            width, height, fps, duration
        );

        let motion_detections =
            self.detect_motion_tracks(video_path, start_time, end_time, width, height)?;

        let sample_interval = 1.0 / self.config.fps_sample.max(1e-3);
        let frames: Vec<FrameResult> = motion_detections
            .into_iter()
            .enumerate()
            .map(|(i, faces)| FrameResult {
                time: start_time + (i as f64 * sample_interval),
                faces,
                activity_scores: None,
                active_speaker: None,
            })
            .collect();

        info!(
            "MotionAware pipeline: {} frames with {} total detections",
            frames.len(),
            frames.iter().map(|f| f.faces.len()).sum::<usize>()
        );

        Ok(DetectionResult {
            frames,
            speaker_segments: None,
            tier_used: DetectionTier::MotionAware,
            width,
            height,
            fps,
            duration: video_info.duration,
        })
    }

    fn tier(&self) -> DetectionTier {
        DetectionTier::MotionAware
    }

    fn name(&self) -> &'static str {
        "motion_aware"
    }
}
