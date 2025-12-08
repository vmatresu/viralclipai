pub(crate) mod analyzer;
pub(crate) mod layout_planner;
pub(crate) mod renderer;

use std::path::Path;

use analyzer::ActivityAnalyzer;
use layout_planner::LayoutPlanner;
use renderer::ActivitySplitRenderer;
use tracing::info;
use vclip_models::{ClipTask, EncodingConfig, Style};

use crate::clip::extract_segment;
use crate::error::{MediaError, MediaResult};
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::detector::FaceDetector;
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
    let detector = FaceDetector::new(config.clone());
    let detections = detector
        .detect_in_video(&segment_path, 0.0, duration, width, height, fps)
        .await?;

    let analyzer = ActivityAnalyzer::new(config.clone(), width, height);
    let timeline = analyzer.build_timeline(&detections, duration)?;

    let planner = LayoutPlanner::new(config.clone());
    let spans = planner.plan(&timeline, duration)?;
    if spans.is_empty() {
        return Err(MediaError::detection_failed(
            "Smart Split (Activity) could not determine any layout spans",
        ));
    }

    let sample_interval = if config.fps_sample > 0.0 {
        1.0 / config.fps_sample
    } else {
        0.125
    };
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

