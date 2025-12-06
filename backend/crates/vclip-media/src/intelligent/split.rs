//! Intelligent Split video processing with face detection.
//!
//! This module implements the "intelligent split" style that:
//! 1. Detects faces in the video
//! 2. If 2+ faces in different horizontal regions → split view (face1 top, face2 bottom)
//! 3. If 1 face or faces in same region → full frame with face tracking
//!
//! # Architecture
//!
//! ```text
//! Video Input
//!     │
//!     ▼
//! ┌─────────────────┐
//! │  Layout Analyzer │ ← Detect if split-screen content
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  Face Detector  │ ← Detect faces in left/right halves
//! └────────┬────────┘
//!          │
//!     ┌────┴────┐
//!     │ Decision │
//!     └────┬────┘
//!          │
//!   ┌──────┴──────┐
//!   │             │
//!   ▼             ▼
//! 2+ faces     1 face
//! diff. pos.   or same pos.
//!   │             │
//!   ▼             ▼
//! Split View   Full Frame
//! (top/bottom) (face tracked)
//! ```

use std::path::Path;
use tracing::{info, warn, debug};

use super::config::IntelligentCropConfig;
use super::models::{BoundingBox, Detection};
use super::IntelligentCropper;
use super::FaceDetector;
use crate::clip::extract_segment;
use crate::command::{FfmpegCommand, FfmpegRunner};
use crate::error::{MediaError, MediaResult};
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use vclip_models::{ClipTask, EncodingConfig};

/// Layout mode for the output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitLayout {
    /// Split view with two faces - top and bottom panels
    SplitTopBottom,
    /// Single full frame with face tracking (when only 1 face or faces in same region)
    FullFrame,
}

/// Result of layout analysis.
#[derive(Debug)]
pub struct LayoutAnalysis {
    /// Determined layout mode
    pub layout: SplitLayout,
    /// Primary face region (left/top in split, main in full)
    pub primary_region: Option<FaceRegion>,
    /// Secondary face region (right/bottom in split, None in full)
    pub secondary_region: Option<FaceRegion>,
    /// Confidence score 0-1
    pub confidence: f64,
}

/// Detected face region within the frame.
#[derive(Debug, Clone)]
pub struct FaceRegion {
    /// Average bounding box of face across frames
    pub bbox: BoundingBox,
    /// Horizontal position category (left, center, right)
    pub horizontal_position: HorizontalPosition,
    /// Detection confidence
    pub confidence: f64,
    /// Track ID for this face
    pub track_id: u32,
}

/// Horizontal position category.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HorizontalPosition {
    Left,
    Center,
    Right,
}

/// Intelligent Split processor.
pub struct IntelligentSplitProcessor {
    config: IntelligentCropConfig,
    detector: FaceDetector,
}

impl IntelligentSplitProcessor {
    /// Create a new processor with the given configuration.
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

    /// Process a video segment with intelligent split.
    ///
    /// # Arguments
    /// * `segment_path` - Path to pre-cut video segment
    /// * `output_path` - Path for output file
    /// * `encoding` - Encoding configuration
    ///
    /// # Returns
    /// The determined layout mode used
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment_path: P,
        output_path: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<SplitLayout> {
        let segment = segment_path.as_ref();
        let output = output_path.as_ref();

        info!("Analyzing video for intelligent split: {:?}", segment);

        // 1. Get video metadata
        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "Video: {}x{} @ {:.2}fps, duration: {:.2}s",
            width, height, fps, duration
        );

        // 2. Detect faces in the video
        info!("Step 1/4: Detecting faces...");
        let detections = self
            .detector
            .detect_in_video(segment, 0.0, duration, width, height, fps)
            .await?;

        // 3. Analyze layout based on face detections
        info!("Step 2/4: Analyzing layout...");
        let layout_analysis = self.analyze_layout(&detections, width, height);

        info!(
            "Layout decision: {:?} (confidence: {:.2})",
            layout_analysis.layout, layout_analysis.confidence
        );

        // 4. Process based on layout
        match layout_analysis.layout {
            SplitLayout::SplitTopBottom => {
                info!("Step 3/4: Processing as split view (top/bottom)...");
                self.process_split_view(
                    segment,
                    output,
                    &layout_analysis,
                    width,
                    height,
                    fps,
                    duration,
                    encoding,
                )
                .await?;
            }
            SplitLayout::FullFrame => {
                info!("Step 3/4: Processing as full frame with face tracking...");
                self.process_full_frame(segment, output).await?;
            }
        }

        // 5. Generate thumbnail
        info!("Step 4/4: Generating thumbnail...");
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            warn!("Failed to generate thumbnail: {}", e);
        }

        info!("Intelligent split complete: {:?}", output);
        Ok(layout_analysis.layout)
    }

    /// Analyze face detections to determine optimal layout.
    fn analyze_layout(
        &self,
        detections: &[Vec<Detection>],
        width: u32,
        _height: u32,
    ) -> LayoutAnalysis {
        // Aggregate face detections by track ID
        let mut track_aggregates: std::collections::HashMap<u32, Vec<&Detection>> =
            std::collections::HashMap::new();

        for frame_dets in detections {
            for det in frame_dets {
                track_aggregates
                    .entry(det.track_id)
                    .or_insert_with(Vec::new)
                    .push(det);
            }
        }

        if track_aggregates.is_empty() {
            // No faces detected - use center-weighted full frame
            debug!("No faces detected, using center-weighted full frame");
            return LayoutAnalysis {
                layout: SplitLayout::FullFrame,
                primary_region: None,
                secondary_region: None,
                confidence: 0.3,
            };
        }

        // Calculate average position for each track
        let mut face_regions: Vec<FaceRegion> = track_aggregates
            .into_iter()
            .map(|(track_id, dets)| {
                let avg_cx: f64 = dets.iter().map(|d| d.bbox.cx()).sum::<f64>() / dets.len() as f64;
                let avg_cy: f64 = dets.iter().map(|d| d.bbox.cy()).sum::<f64>() / dets.len() as f64;
                let avg_width: f64 =
                    dets.iter().map(|d| d.bbox.width).sum::<f64>() / dets.len() as f64;
                let avg_height: f64 =
                    dets.iter().map(|d| d.bbox.height).sum::<f64>() / dets.len() as f64;
                let avg_confidence: f64 =
                    dets.iter().map(|d| d.score).sum::<f64>() / dets.len() as f64;

                let horizontal_position = self.classify_horizontal_position(avg_cx, width);

                FaceRegion {
                    bbox: BoundingBox::new(
                        avg_cx - avg_width / 2.0,
                        avg_cy - avg_height / 2.0,
                        avg_width,
                        avg_height,
                    ),
                    horizontal_position,
                    confidence: avg_confidence,
                    track_id,
                }
            })
            .collect();

        // Sort by detection count/confidence (most prominent faces first)
        face_regions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Decision logic
        if face_regions.len() >= 2 {
            let face1 = &face_regions[0];
            let face2 = &face_regions[1];

            // Check if faces are in different horizontal regions
            let different_regions = face1.horizontal_position != face2.horizontal_position
                && face1.horizontal_position != HorizontalPosition::Center
                && face2.horizontal_position != HorizontalPosition::Center;

            if different_regions {
                // Determine which face goes on top (left face → top, right face → bottom)
                let (primary, secondary) =
                    if face1.horizontal_position == HorizontalPosition::Left {
                        (face1.clone(), face2.clone())
                    } else {
                        (face2.clone(), face1.clone())
                    };

                return LayoutAnalysis {
                    layout: SplitLayout::SplitTopBottom,
                    primary_region: Some(primary),
                    secondary_region: Some(secondary),
                    confidence: (face1.confidence + face2.confidence) / 2.0,
                };
            }
        }

        // Default: single face or faces in same region → full frame
        LayoutAnalysis {
            layout: SplitLayout::FullFrame,
            primary_region: face_regions.into_iter().next(),
            secondary_region: None,
            confidence: 0.7,
        }
    }

    /// Classify horizontal position of a face based on center x.
    fn classify_horizontal_position(&self, cx: f64, width: u32) -> HorizontalPosition {
        let w = width as f64;
        let third = w / 3.0;

        if cx < third {
            HorizontalPosition::Left
        } else if cx > 2.0 * third {
            HorizontalPosition::Right
        } else {
            HorizontalPosition::Center
        }
    }

    /// Process as split view with two panels.
    async fn process_split_view(
        &self,
        segment: &Path,
        output: &Path,
        _layout: &LayoutAnalysis,
        _width: u32,
        _height: u32,
        _fps: f64,
        _duration: f64,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        // Create temp directory for intermediate files
        let temp_dir = tempfile::tempdir()?;

        // Step 1: Extract left and right halves
        let left_half = temp_dir.path().join("left.mp4");
        let right_half = temp_dir.path().join("right.mp4");

        // Crop left half
        let cmd_left = FfmpegCommand::new(segment, &left_half)
            .video_filter("crop=iw/2:ih:0:0")
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("copy");

        FfmpegRunner::new().run(&cmd_left).await?;

        // Crop right half
        let cmd_right = FfmpegCommand::new(segment, &right_half)
            .video_filter("crop=iw/2:ih:iw/2:0")
            .video_codec(&encoding.codec)
            .preset(&encoding.preset)
            .crf(encoding.crf)
            .audio_codec("copy");

        FfmpegRunner::new().run(&cmd_right).await?;

        // Step 2: Apply intelligent crop to each half
        let left_cropped = temp_dir.path().join("left_crop.mp4");
        let right_cropped = temp_dir.path().join("right_crop.mp4");

        // Create cropper for face tracking on each half
        let cropper = IntelligentCropper::new(self.config.clone());

        // Process left half (will become top panel)
        info!("  Processing left half (top panel)...");
        cropper.process(&left_half, &left_cropped).await?;

        // Process right half (will become bottom panel)
        info!("  Processing right half (bottom panel)...");
        cropper.process(&right_half, &right_cropped).await?;

        // Step 3: Stack halves vertically (left=top, right=bottom)
        info!("  Stacking panels...");
        let final_crf = encoding.crf.saturating_add(4);

        let stack_args = vec![
            "-y".to_string(),
            "-i".to_string(),
            left_cropped.to_string_lossy().to_string(),
            "-i".to_string(),
            right_cropped.to_string_lossy().to_string(),
            "-filter_complex".to_string(),
            "[0:v][1:v]vstack=inputs=2".to_string(),
            "-c:v".to_string(),
            encoding.codec.clone(),
            "-preset".to_string(),
            encoding.preset.clone(),
            "-crf".to_string(),
            final_crf.to_string(),
            "-c:a".to_string(),
            encoding.audio_codec.clone(),
            "-b:a".to_string(),
            encoding.audio_bitrate.clone(),
            output.to_string_lossy().to_string(),
        ];

        let output_status = tokio::process::Command::new("ffmpeg")
            .args(&stack_args)
            .output()
            .await?;

        if !output_status.status.success() {
            return Err(MediaError::ffmpeg_failed(
                "Stacking failed",
                Some(String::from_utf8_lossy(&output_status.stderr).to_string()),
                output_status.status.code(),
            ));
        }

        Ok(())
    }

    /// Process as full frame with single face tracking.
    async fn process_full_frame(&self, segment: &Path, output: &Path) -> MediaResult<()> {
        // Use the standard intelligent crop on the full frame
        let cropper = IntelligentCropper::new(self.config.clone());
        cropper.process(segment, output).await
    }
}

/// Create an intelligent split clip from a video file.
///
/// This is the main entry point for the IntelligentSplit style.
///
/// # Behavior
/// - **2+ faces in different horizontal regions**: Split view (left face → top, right face → bottom)
/// - **1 face or faces in same region**: Full frame with face tracking
///
/// # Arguments
/// * `input` - Path to the input video file (full source video)
/// * `output` - Path for the output file
/// * `task` - Clip task with timing and style information
/// * `encoding` - Encoding configuration
/// * `progress_callback` - Callback for progress updates
pub async fn create_intelligent_split_clip<P, F>(
    input: P,
    output: P,
    task: &ClipTask,
    encoding: &EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();

    info!(
        "Creating intelligent split clip: {} -> {}",
        input.display(),
        output.display()
    );

    // Parse timestamps and apply padding
    let start_secs = (super::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = super::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    // Step 1: Extract segment to temporary file
    let segment_path = output.with_extension("segment.mp4");
    info!(
        "Extracting segment for intelligent split: {:.2}s - {:.2}s",
        start_secs, end_secs
    );

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Process with intelligent split
    let config = IntelligentCropConfig::default();
    let processor = IntelligentSplitProcessor::new(config);
    let result = processor.process(segment_path.as_path(), output, encoding).await;

    // Step 3: Cleanup temporary segment file
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            warn!(
                "Failed to delete temporary segment file {}: {}",
                segment_path.display(),
                e
            );
        } else {
            info!("Deleted temporary segment: {}", segment_path.display());
        }
    }

    result.map(|_| ())
}
