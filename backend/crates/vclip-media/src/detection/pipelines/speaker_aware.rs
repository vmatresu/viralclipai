//! SpeakerAware tier pipeline - YuNet + FaceMesh visual activity.
//!
//! Full detection stack with mouth activity analysis for multi-speaker content.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tracing::debug;
use vclip_models::DetectionTier;

use crate::detection::pipeline::{
    ActiveSpeakerHint, DetectionPipeline, DetectionResult, FrameResult,
};
use crate::detection::providers::{FaceProvider, YuNetFaceProvider};
use crate::error::MediaResult;
use crate::intelligent::face_mesh::{FaceDetailAnalyzer, OrtFaceMeshAnalyzer};
use crate::intelligent::models::{BoundingBox, Detection};
use crate::intelligent::yunet::YuNetDetector;
use crate::intelligent::{IntelligentCropConfig, IoUTracker};
use crate::probe::probe_video;

/// Default sample interval for frame analysis (2 fps).
const DEFAULT_SAMPLE_INTERVAL: f64 = 0.5;

/// Pipeline for `DetectionTier::SpeakerAware` - full detection stack.
///
/// Uses YuNet for face detection and FaceMesh for mouth activity analysis.
/// Best quality for multi-speaker content.
pub struct SpeakerAwarePipeline {
    face_provider: YuNetFaceProvider,
    face_analyzer: Option<Arc<dyn FaceDetailAnalyzer + Send + Sync>>,
}

impl SpeakerAwarePipeline {
    pub fn new() -> Self {
        // Face mesh analyzer is optional; if model missing we still run
        let face_analyzer = OrtFaceMeshAnalyzer::new_default()
            .ok()
            .map(|a| Arc::new(a) as _);

        Self {
            face_provider: YuNetFaceProvider::new(),
            face_analyzer,
        }
    }

    /// Detect faces with FaceMesh refinement for mouth activity.
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
            let read_ok = cap
                .read(&mut frame)
                .map_err(|e| crate::error::MediaError::detection_failed(format!("Read: {e}")))?;

            if !read_ok || frame.empty() {
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

impl Default for SpeakerAwarePipeline {
    fn default() -> Self {
        Self::new()
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

        // Prefer YuNet + FaceMesh refinement when OpenCV is available
        let face_detections = if cfg!(feature = "opencv") {
            self.detect_with_face_mesh(video_path, start_time, end_time, width, height)?
        } else {
            self.face_provider
                .detect_faces(video_path, start_time, end_time, width, height, fps)
                .await?
        };

        // Compute per-face visual activity scores using mouth openness
        let frames: Vec<FrameResult> = face_detections
            .into_iter()
            .enumerate()
            .map(|(i, faces)| {
                let time = start_time + (i as f64 * DEFAULT_SAMPLE_INTERVAL);

                let mut activity_scores: Vec<(u32, f64)> = faces
                    .iter()
                    .map(|det| {
                        let score = det.mouth_openness.unwrap_or(0.0);
                        (det.track_id, score)
                    })
                    .collect();

                // Determine active speaker from mouth activity (visual-only)
                let active_speaker = activity_scores
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(track_id, _)| {
                        if let Some(face) = faces.iter().find(|f| f.track_id == *track_id) {
                            if face.bbox.cx() < (width as f64 / 2.0) {
                                ActiveSpeakerHint::Left
                            } else {
                                ActiveSpeakerHint::Right
                            }
                        } else {
                            ActiveSpeakerHint::Single
                        }
                    });

                // Ensure deterministic order
                activity_scores.sort_by_key(|(track_id, _)| *track_id);

                FrameResult {
                    time,
                    faces,
                    activity_scores: Some(activity_scores),
                    active_speaker,
                }
            })
            .collect();

        Ok(DetectionResult {
            frames,
            speaker_segments: None,
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

#[cfg(feature = "opencv")]
fn bbox_to_rect(b: &BoundingBox, frame_w: u32, frame_h: u32) -> MediaResult<opencv::core::Rect> {
    let x = b.x.max(0.0).min(frame_w as f64 - 1.0) as i32;
    let y = b.y.max(0.0).min(frame_h as f64 - 1.0) as i32;
    let w = b.width.max(1.0).min(frame_w as f64 - b.x).round() as i32;
    let h = b.height.max(1.0).min(frame_h as f64 - b.y).round() as i32;
    Ok(opencv::core::Rect::new(x, y, w, h))
}
