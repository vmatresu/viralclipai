pub(crate) mod analyzer;
pub(crate) mod layout_planner;
pub(crate) mod renderer;

use std::path::Path;

use analyzer::ActivityAnalyzer;
use layout_planner::{LayoutMode, LayoutPlanner, LayoutSpan};
use renderer::ActivitySplitRenderer;
use tracing::info;
use vclip_models::{ClipTask, EncodingConfig, Style};

use crate::clip::extract_segment;
use crate::error::{MediaError, MediaResult};
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::detector::FaceDetector;
use crate::intelligent::models::FrameDetections;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Activity observation for a single sampled frame.
#[derive(Debug, Clone)]
pub(crate) struct TimelineFrame {
    pub time: f64,
    pub detections: Vec<crate::intelligent::models::Detection>,
    /// (track_id, raw_activity_score)
    pub raw_activity: Vec<(u32, f64)>,
}

/// Create a Smart Split (Activity) clip with dynamic full/split layouts.
pub async fn create_activity_split_clip<P, F>(
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
    if task.style != Style::IntelligentSplitActivity {
        return Err(MediaError::InvalidVideo(
            "Smart Split (Activity) can only process the intelligent_split_activity style"
                .to_string(),
        ));
    }

    let input = input.as_ref();
    let output = output.as_ref();

    let start_secs = (crate::intelligent::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = crate::intelligent::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    let segment_path = output.with_extension("segment.mp4");
    info!(
        "Extracting segment for Smart Split (Activity): {:.2}s - {:.2}s",
        start_secs, end_secs
    );
    extract_segment(input, &segment_path, start_secs, duration).await?;

    let video_info = probe_video(&segment_path).await?;
    let width = video_info.width;
    let height = video_info.height;
    let fps = video_info.fps;

    let config = IntelligentCropConfig::default();
    let sample_interval = if config.fps_sample > 0.0 {
        1.0 / config.fps_sample
    } else {
        0.125
    };
    let detector = FaceDetector::new(config.clone());
    let mut detections = detector
        .detect_in_video(&segment_path, 0.0, duration, width, height, fps)
        .await?;

    // Log face detection statistics for debugging
    let max_faces_per_frame = detections.iter().map(|f| f.len()).max().unwrap_or(0);
    let total_detections: usize = detections.iter().map(|f| f.len()).sum();
    let unique_tracks: std::collections::HashSet<u32> = detections
        .iter()
        .flatten()
        .map(|d| d.track_id)
        .collect();
    info!(
        max_faces_per_frame = max_faces_per_frame,
        total_detections = total_detections,
        unique_tracks = unique_tracks.len(),
        frames = detections.len(),
        duration = format!("{:.2}s", duration),
        "Smart Split (Activity) face detection complete"
    );

    let analyzer = ActivityAnalyzer::new(config.clone(), width, height);
    let planner = LayoutPlanner::new(config.clone());

    let timeline = match analyzer.build_timeline(&detections, duration) {
        Ok(timeline) => Some(timeline),
        Err(MediaError::DetectionFailed(msg)) => {
            info!(
                "Smart Split (Activity): face activity timeline unavailable ({}); using motion fallback",
                msg
            );
            None
        }
        Err(e) => return Err(e),
    };

    let spans: Vec<LayoutSpan>;
    if let Some(timeline) = timeline {
        match planner.plan(&timeline, duration) {
            Ok(plan) if !plan.is_empty() => {
                spans = plan;
            }
            Ok(_) => {
                info!("Smart Split (Activity): planner returned no spans; using motion fallback");
                (detections, spans) = motion_fallback(
                    &detector,
                    &segment_path,
                    duration,
                    width,
                    height,
                )
                .await?;
            }
            Err(MediaError::DetectionFailed(msg)) => {
                info!(
                    "Smart Split (Activity): planner could not determine layout ({}); using motion fallback",
                    msg
                );
                (detections, spans) = motion_fallback(
                    &detector,
                    &segment_path,
                    duration,
                    width,
                    height,
                )
                .await?;
            }
            Err(e) => return Err(e),
        }
    } else {
        (detections, spans) = motion_fallback(&detector, &segment_path, duration, width, height).await?;
    }

    let renderer = ActivitySplitRenderer::new(
        config.clone(),
        encoding.clone(),
        width,
        height,
        sample_interval,
    );
    renderer
        .render(&segment_path, output, &detections, &spans)
        .await?;

    let thumb_path = output.with_extension("jpg");
    if let Err(e) = generate_thumbnail(output, &thumb_path).await {
        tracing::warn!("Failed to generate thumbnail: {}", e);
    }

    if segment_path.exists() {
        let _ = tokio::fs::remove_file(&segment_path).await;
    }

    Ok(())
}

async fn motion_fallback(
    detector: &FaceDetector,
    segment_path: &Path,
    duration: f64,
    width: u32,
    height: u32,
) -> MediaResult<(Vec<FrameDetections>, Vec<LayoutSpan>)> {
    info!(
        duration = format!("{:.2}s", duration),
        width = width,
        height = height,
        "Smart Split (Activity): using motion-based framing fallback"
    );

    let detections = detector
        .detect_motion_tracks(segment_path, 0.0, duration, width, height)
        .await?;

    // Log motion detection results
    let detection_count: usize = detections.iter().map(|f| f.len()).sum();
    let unique_tracks: std::collections::HashSet<u32> = detections
        .iter()
        .flatten()
        .map(|d| d.track_id)
        .collect();
    info!(
        frames = detections.len(),
        total_detections = detection_count,
        unique_tracks = unique_tracks.len(),
        "Motion fallback detection complete"
    );

    let primary_track = detections
        .iter()
        .find_map(|frame| frame.first().map(|d| d.track_id))
        .unwrap_or(0);

    info!(
        primary_track = primary_track,
        "Motion fallback using Full layout"
    );

    let spans = vec![LayoutSpan {
        start: 0.0,
        end: duration,
        layout: LayoutMode::Full { primary: primary_track },
    }];

    Ok((detections, spans))
}

