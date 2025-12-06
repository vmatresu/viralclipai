//! Intelligent video cropping with face detection and tracking.
//!
//! This module implements the smart reframe pipeline from Python in Rust:
//! 1. Face detection using OpenCV's Haar cascade or DNN
//! 2. IoU-based tracking for identity persistence
//! 3. Camera path smoothing for professional motion
//! 4. Crop window computation for target aspect ratios
//! 5. FFmpeg rendering with dynamic/static crops
//!
//! # Architecture
//!
//! ```text
//! Video Input
//!     │
//!     ▼
//! ┌─────────────────┐
//! │  Frame Extractor │ ← Extract frames at sample rate
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  Face Detector  │ ← Detect faces in each frame
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │   IoU Tracker   │ ← Track faces across frames
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │ Camera Smoother │ ← Smooth camera path
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  Crop Planner   │ ← Compute crop windows
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │    Renderer     │ ← FFmpeg rendering
//! └────────┬────────┘
//!          │
//!          ▼
//!    Output Video
//! ```

pub mod config;
pub mod crop_planner;
pub mod detector;
pub mod models;
pub mod renderer;
pub mod smoother;
pub mod split;
pub mod tracker;

pub use config::IntelligentCropConfig;
pub use crop_planner::CropPlanner;
pub use detector::FaceDetector;
pub use models::*;
pub use renderer::IntelligentRenderer;
pub use smoother::CameraSmoother;
pub use split::{create_intelligent_split_clip, IntelligentSplitProcessor, SplitLayout};
pub use tracker::IoUTracker;

use crate::error::MediaResult;
use crate::probe::probe_video;
use std::path::Path;
use tracing::info;
use vclip_models::ClipTask;

/// Parse a timestamp string (HH:MM:SS.mmm or SS.mmm) to seconds.
fn parse_timestamp(ts: &str) -> MediaResult<f64> {
    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        1 => {
            // Just seconds
            parts[0].parse::<f64>()
                .map_err(|_| crate::error::MediaError::InvalidTimestamp(ts.to_string()))
        }
        2 => {
            // MM:SS
            let mins: f64 = parts[0].parse()
                .map_err(|_| crate::error::MediaError::InvalidTimestamp(ts.to_string()))?;
            let secs: f64 = parts[1].parse()
                .map_err(|_| crate::error::MediaError::InvalidTimestamp(ts.to_string()))?;
            Ok(mins * 60.0 + secs)
        }
        3 => {
            // HH:MM:SS
            let hours: f64 = parts[0].parse()
                .map_err(|_| crate::error::MediaError::InvalidTimestamp(ts.to_string()))?;
            let mins: f64 = parts[1].parse()
                .map_err(|_| crate::error::MediaError::InvalidTimestamp(ts.to_string()))?;
            let secs: f64 = parts[2].parse()
                .map_err(|_| crate::error::MediaError::InvalidTimestamp(ts.to_string()))?;
            Ok(hours * 3600.0 + mins * 60.0 + secs)
        }
        _ => Err(crate::error::MediaError::InvalidTimestamp(ts.to_string())),
    }
}

/// Main intelligent cropping pipeline.
///
/// Orchestrates the full pipeline from video analysis to rendering.
pub struct IntelligentCropper {
    config: IntelligentCropConfig,
    detector: FaceDetector,
}

impl IntelligentCropper {
    /// Create a new intelligent cropper with the given configuration.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self {
            detector: FaceDetector::new(config.clone()),
            config,
        }
    }

    /// Create with default configuration.
    pub fn default() -> Self {
        Self::new(IntelligentCropConfig::default())
    }

    /// Analyze and render an intelligent crop on a pre-cut segment.
    ///
    /// This is the main entry point for intelligent cropping.
    /// The input should be a pre-cut segment (not the full video).
    pub async fn process<P: AsRef<Path>>(
        &self,
        input: P,
        output: P,
    ) -> MediaResult<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        info!("Starting intelligent crop analysis for {:?}", input);

        // 1. Get video metadata
        let video_info = probe_video(input).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "Video: {}x{} @ {:.2}fps, duration: {:.2}s",
            width, height, fps, duration
        );

        // Process the entire segment (it's already been cut to the right time range)
        let start_time = 0.0;
        let end_time = duration;

        // 2. Detect faces in the video
        info!("Step 1/3: Detecting faces...");
        let detections = self
            .detector
            .detect_in_video(input, start_time, end_time, width, height, fps)
            .await?;

        let total_detections: usize = detections.iter().map(|d| d.len()).sum();
        info!("  Found {} face detections", total_detections);

        // 3. Compute camera plan
        info!("Step 2/3: Computing camera path...");
        let smoother = CameraSmoother::new(self.config.clone(), fps);
        let camera_keyframes = smoother.compute_camera_plan(&detections, width, height, start_time, end_time);
        info!("  Generated {} camera keyframes", camera_keyframes.len());

        // 4. Compute crop windows
        info!("Step 3/3: Computing crop windows...");
        let planner = CropPlanner::new(self.config.clone(), width, height);
        let target_aspect = AspectRatio::new(9, 16); // Portrait 9:16
        let crop_windows = planner.compute_crop_windows(&camera_keyframes, &target_aspect);
        info!("  Generated {} crop windows", crop_windows.len());

        // 5. Render the output
        info!("Rendering output...");
        let renderer = IntelligentRenderer::new(self.config.clone());
        renderer
            .render(input, output, &crop_windows, start_time, duration)
            .await?;

        info!("Intelligent crop complete: {:?}", output);
        
        // Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = crate::thumbnail::generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("Failed to generate thumbnail for intelligent clip: {}", e);
        }
        
        Ok(())
    }
}

/// Create an intelligent clip from a video file.
///
/// This is the main entry point for the Intelligent style.
/// 
/// # Workflow
/// 1. Extract the segment from the source video (fast, no re-encoding)
/// 2. Apply intelligent cropping to the segment
/// 3. Delete the temporary segment file
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `encoding` - Encoding configuration
/// * `progress_callback` - Callback for progress updates
pub async fn create_intelligent_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    _encoding: &vclip_models::EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();
    
    // Parse timestamps and apply padding
    let start_secs = (parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;
    
    // Step 1: Extract segment to temporary file
    let segment_path = output.with_extension("segment.mp4");
    info!("Extracting segment for intelligent crop: {:.2}s - {:.2}s", start_secs, end_secs);
    
    crate::clip::extract_segment(input, &segment_path, start_secs, duration).await?;
    
    // Step 2: Apply intelligent cropping to the segment
    let config = IntelligentCropConfig::default();
    let cropper = IntelligentCropper::new(config);
    let result = cropper.process(segment_path.as_path(), output).await;
    
    // Step 3: Cleanup temporary segment file
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!("Failed to delete temporary segment file {}: {}", segment_path.display(), e);
        } else {
            info!("Deleted temporary segment: {}", segment_path.display());
        }
    }
    
    result
}
