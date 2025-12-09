//! Pipeline builder for tier-specific detection configurations.
//!
//! The `PipelineBuilder` creates detection pipelines appropriate for each
//! `DetectionTier`. Each tier is strict: if a required detector is unavailable
//! or fails, the pipeline returns an error rather than degrading quality.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};
use vclip_models::DetectionTier;

use super::pipeline::{ActiveSpeakerHint, DetectionPipeline, DetectionResult, FrameResult};
use super::providers::{
    AudioProvider, FaceActivityProvider, FaceProvider, StandardAudioProvider,
    VisualFaceActivityProvider, YuNetFaceProvider,
};
use crate::error::MediaResult;
use crate::intelligent::{IntelligentCropConfig, IoUTracker, SpeakerDetector};
use crate::intelligent::face_mesh::{FaceDetailAnalyzer, OrtFaceMeshAnalyzer};
use crate::intelligent::models::Detection;
use crate::intelligent::models::BoundingBox;
use crate::intelligent::yunet::YuNetDetector;
use crate::probe::probe_video;

/// Builder for creating detection pipelines based on tier.
pub struct PipelineBuilder {
    requested_tier: DetectionTier,
}

impl PipelineBuilder {
    /// Create a builder for the specified tier.
    pub fn for_tier(tier: DetectionTier) -> Self {
        Self {
            requested_tier: tier,
        }
    }

    /// Build the detection pipeline.
    ///
    /// Returns a boxed trait object that implements `DetectionPipeline`.
    /// May degrade to a lower tier if required components are unavailable.
    pub fn build(self) -> MediaResult<Box<dyn DetectionPipeline>> {
        match self.requested_tier {
            DetectionTier::None => {
                info!("Building None tier pipeline (heuristic only)");
                Ok(Box::new(NonePipeline))
            }
            DetectionTier::Basic => {
                info!("Building Basic tier pipeline (YuNet face detection)");
                Ok(Box::new(BasicPipeline::new()))
            }
            DetectionTier::SpeakerAware => {
                info!("Building SpeakerAware tier pipeline (YuNet + audio + face activity)");
                Ok(Box::new(SpeakerAwarePipeline::new()))
            }
            DetectionTier::MotionAware => {
                // MotionAware uses Basic pipeline for face detection,
                // visual activity is computed separately in the cropper
                info!("Building MotionAware tier pipeline (YuNet + visual motion)");
                Ok(Box::new(BasicPipeline::new()))
            }
            DetectionTier::ActivityAware => {
                // ActivityAware uses Basic pipeline for face detection,
                // full visual activity with temporal tracking in the cropper
                info!("Building ActivityAware tier pipeline (YuNet + full visual activity)");
                Ok(Box::new(BasicPipeline::new()))
            }
        }
    }
}

// ============================================================================
// None Tier Pipeline
// ============================================================================

/// Pipeline for `DetectionTier::None` - no detection, heuristic positioning only.
struct NonePipeline;

#[async_trait]
impl DetectionPipeline for NonePipeline {
    async fn analyze(
        &self,
        video_path: &Path,
        _start_time: f64,
        _end_time: f64,
    ) -> MediaResult<DetectionResult> {
        let video_info = probe_video(video_path).await?;

        Ok(DetectionResult {
            frames: vec![],
            speaker_segments: None,
            tier_used: DetectionTier::None,
            width: video_info.width,
            height: video_info.height,
            fps: video_info.fps,
            duration: video_info.duration,
        })
    }

    fn tier(&self) -> DetectionTier {
        DetectionTier::None
    }

    fn name(&self) -> &'static str {
        "none"
    }
}

// ============================================================================
// Basic Tier Pipeline
// ============================================================================

/// Pipeline for `DetectionTier::Basic` - YuNet face detection only.
struct BasicPipeline {
    face_provider: YuNetFaceProvider,
}

impl BasicPipeline {
    fn new() -> Self {
        Self {
            face_provider: YuNetFaceProvider::new(),
        }
    }
}

#[async_trait]
impl DetectionPipeline for BasicPipeline {
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
            "Basic pipeline analyzing {}x{} @ {:.2}fps, {:.2}s",
            width, height, fps, duration
        );

        // Detect faces using YuNet (with heuristic fallback)
        let face_detections = self
            .face_provider
            .detect_faces(video_path, start_time, end_time, width, height, fps)
            .await?;

        // Convert to FrameResults
        let sample_interval = 1.0 / 2.0; // Default sample rate from IntelligentCropConfig
        let frames: Vec<FrameResult> = face_detections
            .into_iter()
            .enumerate()
            .map(|(i, faces)| FrameResult {
                time: start_time + (i as f64 * sample_interval),
                faces,
                activity_scores: None,
                active_speaker: None,
            })
            .collect();

        Ok(DetectionResult {
            frames,
            speaker_segments: None,
            tier_used: DetectionTier::Basic,
            width,
            height,
            fps,
            duration: video_info.duration,
        })
    }

    fn tier(&self) -> DetectionTier {
        DetectionTier::Basic
    }

    fn name(&self) -> &'static str {
        "basic"
    }
}

// SpeakerAware Tier Pipeline
// ============================================================================

/// Pipeline for `DetectionTier::SpeakerAware` - full detection stack.
struct SpeakerAwarePipeline {
    face_provider: YuNetFaceProvider,
    audio_provider: StandardAudioProvider,
    face_activity_provider: Arc<std::sync::Mutex<VisualFaceActivityProvider>>,
    face_analyzer: Option<Arc<dyn FaceDetailAnalyzer + Send + Sync>>,
}

impl SpeakerAwarePipeline {
    fn new() -> Self {
        // Face mesh analyzer is optional; if model missing we still run.
        let face_analyzer = OrtFaceMeshAnalyzer::new_default().ok().map(|a| Arc::new(a) as _);

        Self {
            face_provider: YuNetFaceProvider::new(),
            audio_provider: StandardAudioProvider::new(),
            face_activity_provider: Arc::new(std::sync::Mutex::new(
                VisualFaceActivityProvider::new(),
            )),
            face_analyzer,
        }
    }
}

#[async_trait]
impl DetectionPipeline for SpeakerAwarePipeline {
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
            "SpeakerAware pipeline analyzing {}x{} @ {:.2}fps, {:.2}s",
            width, height, fps, duration
        );

        // Prefer YuNet + FaceMesh refinement when OpenCV is available.
        let face_detections = if cfg!(feature = "opencv") {
            self.detect_with_face_mesh(video_path, start_time, end_time, width, height)?
        } else {
            self.face_provider
                .detect_faces(video_path, start_time, end_time, width, height, fps)
                .await?
        };

        // Detect speaker activity
        let speaker_segments = self
            .audio_provider
            .detect_speakers(video_path, duration, width)
            .await?;

        let speaker_detector = SpeakerDetector::new();

        // Compute per-face activity scores
        let sample_interval = 1.0 / 2.0;
        let frames: Vec<FrameResult> = {
            let mut activity_provider = self.face_activity_provider.lock().unwrap();

            face_detections
                .into_iter()
                .enumerate()
                .map(|(i, faces)| {
                    let time = start_time + (i as f64 * sample_interval);

                    // Compute activity scores for each face
                    let activity_scores: Vec<(u32, f64)> = faces
                        .iter()
                        .map(|det| {
                            let score = activity_provider.compute_activity(
                                &det.bbox,
                                det.track_id,
                                time,
                                det.score,
                            );
                            (det.track_id, score)
                        })
                        .collect();

                    let active_speaker = if !speaker_segments.is_empty() {
                        Some(ActiveSpeakerHint::from(
                            speaker_detector.speaker_at_time(&speaker_segments, time),
                        ))
                    } else {
                        None
                    };

                    FrameResult {
                        time,
                        faces,
                        activity_scores: Some(activity_scores),
                        active_speaker,
                    }
                })
                .collect()
        };

        Ok(DetectionResult {
            frames,
            speaker_segments: Some(speaker_segments),
            tier_used: DetectionTier::SpeakerAware,
            width,
            height,
            fps,
            duration: video_info.duration,
        })
    }

    fn tier(&self) -> DetectionTier {
        DetectionTier::SpeakerAware
    }

    fn name(&self) -> &'static str {
        "speaker_aware"
    }
}

impl SpeakerAwarePipeline {
    #[cfg(feature = "opencv")]
    fn detect_with_face_mesh(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        use opencv::prelude::{MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
        use opencv::videoio::{VideoCapture, CAP_ANY, CAP_PROP_POS_MSEC};

        let config = IntelligentCropConfig::default();
        let sample_interval = 1.0 / config.fps_sample;
        let num_samples = ((end_time - start_time) / sample_interval).ceil() as usize;

        let mut cap = VideoCapture::from_file(video_path.to_str().unwrap_or(""), CAP_ANY)
            .map_err(|e| crate::error::MediaError::detection_failed(format!("Open video: {e}")))?;
        if !cap.is_opened().unwrap_or(false) {
            return Err(crate::error::MediaError::detection_failed(
                "Failed to open video for face mesh analysis",
            ));
        }

        let mut detector = YuNetDetector::new(width, height)?;
        let mut tracker = IoUTracker::new(config.iou_threshold, config.max_track_gap);

        let mut all = Vec::with_capacity(num_samples);
        let mut current_time = start_time;

        for _ in 0..num_samples {
            cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Seek: {e}")))?;

            let mut frame = opencv::core::Mat::default();
            if !cap.read(&mut frame).map_err(|e| crate::error::MediaError::detection_failed(format!("Read: {e}")))? {
                all.push(Vec::new());
                current_time += sample_interval;
                continue;
            }
            if frame.empty() {
                all.push(Vec::new());
                current_time += sample_interval;
                continue;
            }

            let dets = detector.detect_in_frame(&frame)?;
            let tracker_input: Vec<(BoundingBox, f64)> = dets
                .into_iter()
                .filter(|(bbox, _)| {
                    let area_ratio = bbox.area() / (width as f64 * height as f64);
                    area_ratio >= config.min_face_size
                })
                .collect();

            let tracked = tracker.update(&tracker_input);

            let mut frame_dets = Vec::with_capacity(tracked.len());
            for (track_id, bbox, score) in tracked {
                let mouth = if let Some(analyzer) = &self.face_analyzer {
                    if score >= config.min_detection_confidence {
                        bbox_to_rect(&bbox, width, height)
                            .ok()
                            .and_then(|r| analyzer.analyze(&frame, &r).ok())
                            .map(|res| res.mouth_openness as f64)
                    } else {
                        None
                    }
                } else {
                    None
                };
                frame_dets.push(Detection::with_mouth(
                    current_time,
                    bbox,
                    score,
                    track_id,
                    mouth,
                ));
            }

            all.push(frame_dets);
            current_time += sample_interval;
        }

        Ok(all)
    }

    #[cfg(not(feature = "opencv"))]
    fn detect_with_face_mesh(
        &self,
        _video_path: &Path,
        _start_time: f64,
        _end_time: f64,
        _width: u32,
        _height: u32,
    ) -> MediaResult<Vec<Vec<Detection>>> {
        Err(crate::error::MediaError::detection_failed(
            "OpenCV not available for face mesh detection",
        ))
    }
}

#[cfg(feature = "opencv")]
fn bbox_to_rect(b: &BoundingBox, frame_w: u32, frame_h: u32) -> MediaResult<opencv::core::Rect> {
    let x = b.x.max(0.0).min(frame_w as f64 - 1.0) as i32;
    let y = b.y.max(0.0).min(frame_h as f64 - 1.0) as i32;
    let w = b.width.max(1.0).min(frame_w as f64 - b.x).round() as i32;
    let h = b.height.max(1.0).min(frame_h as f64 - b.y).round() as i32;
    Ok(opencv::core::Rect::new(x, y, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_builder_none() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::None)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::None);
        assert_eq!(pipeline.name(), "none");
    }

    #[test]
    fn test_pipeline_builder_basic() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::Basic)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::Basic);
        assert_eq!(pipeline.name(), "basic");
    }

    #[test]
    fn test_pipeline_builder_speaker_aware() {
        let pipeline = PipelineBuilder::for_tier(DetectionTier::SpeakerAware)
            .build()
            .unwrap();
        assert_eq!(pipeline.tier(), DetectionTier::SpeakerAware);
        assert_eq!(pipeline.name(), "speaker_aware");
    }

    #[test]
    fn test_all_tiers_can_build() {
        for tier in DetectionTier::ALL {
            let result = PipelineBuilder::for_tier(*tier).build();
            assert!(result.is_ok(), "Failed to build pipeline for tier {:?}", tier);
        }
    }
}
