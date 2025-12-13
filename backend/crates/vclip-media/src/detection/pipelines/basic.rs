//! Basic tier pipeline - YuNet face detection only.

use async_trait::async_trait;
use std::path::Path;
use tracing::debug;
use vclip_models::DetectionTier;

use crate::detection::pipeline::{DetectionPipeline, DetectionResult, FrameResult};
use crate::detection::providers::{FaceProvider, YuNetFaceProvider};
use crate::error::MediaResult;
use crate::probe::probe_video;

/// Default sample interval for frame analysis (2 fps).
const DEFAULT_SAMPLE_INTERVAL: f64 = 0.5;

/// Pipeline for `DetectionTier::Basic` - YuNet face detection only.
///
/// Provides good balance of speed and quality for single-speaker content.
pub struct BasicPipeline {
    face_provider: YuNetFaceProvider,
}

impl BasicPipeline {
    pub fn new() -> Self {
        Self {
            face_provider: YuNetFaceProvider::new(),
        }
    }
}

impl Default for BasicPipeline {
    fn default() -> Self {
        Self::new()
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

        let face_detections = self
            .face_provider
            .detect_faces(video_path, start_time, end_time, width, height, fps)
            .await?;

        let frames: Vec<FrameResult> = face_detections
            .into_iter()
            .enumerate()
            .map(|(i, faces)| FrameResult {
                time: start_time + (i as f64 * DEFAULT_SAMPLE_INTERVAL),
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
