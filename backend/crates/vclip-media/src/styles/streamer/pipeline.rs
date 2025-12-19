//! Processing pipeline for Streamer style.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};
use vclip_models::{ClipTask, EncodingConfig, StreamerParams, TopSceneEntry};

use crate::clip::extract_segment;
use crate::error::{MediaError, MediaResult};
use crate::intelligent::parse_timestamp;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;
use crate::watermark::{append_watermark_filter_complex, WatermarkConfig};

use super::config::StreamerConfig;
use super::filters::build_streamer_filter;

/// Process a single scene with Streamer style (landscape-in-portrait).
pub async fn process_single(
    input: &Path,
    output: &Path,
    task: &ClipTask,
    encoding: &EncodingConfig,
    config: &StreamerConfig,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    let pipeline_start = std::time::Instant::now();

    info!("[STREAMER] ========================================");
    info!("[STREAMER] START: {:?}", input);

    // Probe video
    let video_info = probe_video(input).await?;
    info!(
        "[STREAMER] Video: {}x{} @ {:.2}fps, {:.2}s",
        video_info.width, video_info.height, video_info.fps, video_info.duration
    );

    // Extract segment with padding
    let start_secs = (parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = parse_timestamp(&task.end)? + task.pad_after;
    let clip_duration = end_secs - start_secs;
    let needs_extract = start_secs > 0.001 || (clip_duration + 0.05) < video_info.duration;
    if needs_extract {
        let segment_path = output.with_extension("segment.mp4");
        extract_segment(input, &segment_path, start_secs, clip_duration).await?;
        // Render the streamer format
        render_streamer_format(&segment_path, output, encoding, config, None, None, watermark)
            .await?;
        // Cleanup segment
        cleanup_file(&segment_path).await;
    } else {
        // Render the streamer format
        render_streamer_format(input, output, encoding, config, None, None, watermark).await?;
    }
    // Generate thumbnail
    generate_thumbnail_safe(output).await;
    log_completion("[STREAMER]", pipeline_start, output).await;

    Ok(())
}

/// Process Top Scenes compilation with countdown overlay.
pub async fn process_top_scenes(
    input: &Path,
    output: &Path,
    encoding: &EncodingConfig,
    params: &StreamerParams,
    config: &StreamerConfig,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    let pipeline_start = std::time::Instant::now();

    info!("[STREAMER_TOP_SCENES] ========================================");
    info!("[STREAMER_TOP_SCENES] START: {:?}", input);
    info!(
        "[STREAMER_TOP_SCENES] Processing {} scenes",
        params.top_scenes.len()
    );

    // Limit to max scenes
    let scenes: Vec<&TopSceneEntry> = params
        .top_scenes
        .iter()
        .take(config.max_top_scenes)
        .collect();

    if scenes.is_empty() {
        return Err(MediaError::InvalidVideo(
            "No scenes provided for Top Scenes compilation".to_string(),
        ));
    }

    // Create temp directory for intermediate files
    let temp_dir = output.parent().unwrap_or(Path::new("/tmp"));
    let mut segment_paths: Vec<std::path::PathBuf> = Vec::new();

    // Process each scene
    for (idx, scene) in scenes.iter().enumerate() {
        let countdown_number = (scenes.len() - idx) as u8; // 5, 4, 3, 2, 1
        info!(
            "[STREAMER_TOP_SCENES] Processing scene {} (countdown: {})",
            scene.scene_number, countdown_number
        );

        let styled_path = process_scene_with_countdown(
            input,
            scene,
            countdown_number,
            temp_dir,
            encoding,
            config,
            watermark,
        )
        .await?;

        segment_paths.push(styled_path);
    }

    // Concatenate all styled segments
    concatenate_segments(&segment_paths, output).await?;

    // Cleanup styled segments
    for path in &segment_paths {
        cleanup_file(path).await;
    }

    // Generate thumbnail
    generate_thumbnail_safe(output).await;

    log_completion("[STREAMER_TOP_SCENES]", pipeline_start, output).await;

    Ok(())
}

/// Process a single scene with countdown overlay.
async fn process_scene_with_countdown(
    input: &Path,
    scene: &TopSceneEntry,
    countdown_number: u8,
    temp_dir: &Path,
    encoding: &EncodingConfig,
    config: &StreamerConfig,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<std::path::PathBuf> {
    // Extract segment
    let start_secs = parse_timestamp(&scene.start)?;
    let end_secs = parse_timestamp(&scene.end)?;
    let duration = end_secs - start_secs;

    let segment_path = temp_dir.join(format!(
        "streamer_scene_{}_segment.mp4",
        scene.scene_number
    ));
    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Render with countdown overlay
    let styled_path = temp_dir.join(format!(
        "streamer_scene_{}_styled.mp4",
        scene.scene_number
    ));
    render_streamer_format(
        &segment_path,
        &styled_path,
        encoding,
        config,
        Some(countdown_number),
        scene.title.as_deref(),
        watermark,
    )
    .await?;

    // Cleanup intermediate segment
    cleanup_file(&segment_path).await;

    Ok(styled_path)
}

/// Render video in streamer format (landscape-in-portrait with blurred background).
/// 
/// This function is public so it can be called directly from reprocessing layer
/// for per-scene progress updates.
pub async fn render_streamer_format(
    segment: &Path,
    output: &Path,
    encoding: &EncodingConfig,
    config: &StreamerConfig,
    countdown_number: Option<u8>,
    scene_title: Option<&str>,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    // Get video dimensions
    let video_info = probe_video(segment).await?;
    
    // Build filter complex
    let base_filter_complex = build_streamer_filter(
        config,
        video_info.width,
        video_info.height,
        countdown_number,
        scene_title,
    );
    let (filter_complex, map_label) = if let Some(config) = watermark {
        if let Some(watermarked) = append_watermark_filter_complex(&base_filter_complex, "vout", config) {
            (watermarked.filter_complex, watermarked.output_label)
        } else {
            (base_filter_complex, "vout".to_string())
        }
    } else {
        (base_filter_complex, "vout".to_string())
    };

    debug!("[STREAMER] Filter complex: {}", filter_complex);

    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-i".to_string(),
        segment.to_str().unwrap_or("").to_string(),
        "-filter_complex".to_string(),
        filter_complex,
        "-map".to_string(),
        format!("[{}]", map_label),
        "-map".to_string(),
        "0:a?".to_string(),
        "-c:v".to_string(),
        encoding.codec.clone(),
        "-preset".to_string(),
        encoding.preset.clone(),
        if encoding.use_nvenc {
            "-cq".to_string()
        } else {
            "-crf".to_string()
        },
        encoding.crf.to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-c:a".to_string(),
        encoding.audio_codec.clone(),
        "-b:a".to_string(),
        encoding.audio_bitrate.clone(),
    ];

    if video_info.fps > 30.5 {
        args.extend_from_slice(&["-r".to_string(), "30".to_string()]);
    }
    args.extend_from_slice(&[
        "-maxrate".to_string(),
        "6M".to_string(),
        "-bufsize".to_string(),
        "12M".to_string(),
    ]);

    if !encoding.extra_args.is_empty() {
        args.extend(encoding.extra_args.clone());
    }

    // Force a keyframe at the very first frame to fix frozen video issues when concatenating
    // This ensures each segment starts cleanly when using stream copy concatenation
    args.extend_from_slice(&[
        "-force_key_frames".to_string(),
        "expr:eq(n,0)".to_string(),
    ]);

    args.extend_from_slice(&[
        "-movflags".to_string(),
        "+faststart".to_string(),
        output.to_str().unwrap_or("").to_string(),
    ]);

    let result = Command::new("ffmpeg")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg: {}", e), None, None)
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        tracing::error!(
            stderr = %stderr,
            exit_code = ?result.status.code(),
            input = ?segment.to_string_lossy(),
            output = ?output.to_string_lossy(),
            "[STREAMER] FFmpeg render failed"
        );
        return Err(MediaError::ffmpeg_failed(
            "Streamer render failed",
            Some(stderr.to_string()),
            result.status.code(),
        ));
    }

    info!("[STREAMER] Rendered streamer format successfully");
    Ok(())
}

/// Concatenate multiple video segments into a single output using stream copy.
/// 
/// This function is public so it can be called directly from reprocessing layer.
/// Uses stream copy (-c copy) since all segments should be in the same format.
pub async fn concatenate_segments(
    segments: &[std::path::PathBuf],
    output: &Path,
) -> MediaResult<()> {
    if segments.is_empty() {
        return Err(MediaError::InvalidVideo(
            "No segments to concatenate".to_string(),
        ));
    }

    if segments.len() == 1 {
        // Just copy the single segment
        tokio::fs::copy(&segments[0], output).await.map_err(|e| {
            MediaError::InvalidVideo(format!("Failed to copy segment: {}", e))
        })?;
        return Ok(());
    }

    // Create a concat list file
    let concat_list_path = output.with_extension("concat.txt");
    let concat_content: String = segments
        .iter()
        .map(|p| format!("file '{}'", p.display()))
        .collect::<Vec<_>>()
        .join("\n");

    tokio::fs::write(&concat_list_path, &concat_content)
        .await
        .map_err(|e| MediaError::InvalidVideo(format!("Failed to write concat list: {}", e)))?;

    // Use stream copy (-c copy) since all segments are already encoded in the same format
    // This is ~10x faster than re-encoding and produces smaller files
    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            concat_list_path.to_str().unwrap_or(""),
            "-c",
            "copy",  // Stream copy - no re-encoding
            "-movflags",
            "+faststart",
            output.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            MediaError::ffmpeg_failed(format!("Failed to run FFmpeg concat: {}", e), None, None)
        })?;

    // Cleanup concat list
    let _ = tokio::fs::remove_file(&concat_list_path).await;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(MediaError::ffmpeg_failed(
            "Segment concatenation failed",
            Some(stderr.to_string()),
            result.status.code(),
        ));
    }

    info!(
        "[STREAMER] Concatenated {} segments successfully",
        segments.len()
    );
    Ok(())
}

/// Safely cleanup a file, logging any errors.
async fn cleanup_file(path: &Path) {
    if path.exists() {
        if let Err(e) = tokio::fs::remove_file(path).await {
            warn!("[STREAMER] Failed to cleanup {:?}: {}", path, e);
        }
    }
}

/// Generate thumbnail with error handling.
async fn generate_thumbnail_safe(output: &Path) {
    let thumb_path = output.with_extension("jpg");
    if let Err(e) = generate_thumbnail(output, &thumb_path).await {
        warn!("[STREAMER] Failed to generate thumbnail: {}", e);
    }
}

/// Log completion with file size.
async fn log_completion(prefix: &str, start: std::time::Instant, output: &Path) {
    let file_size = tokio::fs::metadata(output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!("{} ========================================", prefix);
    info!(
        "{} COMPLETE in {:.2}s - {:.2} MiB",
        prefix,
        start.elapsed().as_secs_f64(),
        file_size as f64 / (1024.0 * 1024.0)
    );
}

/// Process Top Scenes compilation from pre-extracted raw segments.
///
/// This function takes multiple raw segment files (one per scene) and creates
/// a single compilation video with countdown overlays.
///
/// # Arguments
/// * `segment_paths` - Paths to raw segment files (already extracted), ordered for countdown (highest first)
/// * `output` - Output path for the compilation video
/// * `encoding` - Encoding configuration
/// * `params` - Streamer params containing the TopSceneEntry list
pub async fn process_top_scenes_from_segments(
    segment_paths: &[std::path::PathBuf],
    output: &Path,
    encoding: &EncodingConfig,
    params: &StreamerParams,
    watermark: Option<&WatermarkConfig>,
) -> MediaResult<()> {
    let pipeline_start = std::time::Instant::now();
    let config = super::config::StreamerConfig::default();

    info!("[STREAMER_TOP_SCENES] ========================================");
    info!(
        "[STREAMER_TOP_SCENES] START: Processing {} segments into compilation",
        segment_paths.len()
    );

    if segment_paths.is_empty() || params.top_scenes.is_empty() {
        return Err(MediaError::InvalidVideo(
            "No segments provided for Top Scenes compilation".to_string(),
        ));
    }

    if segment_paths.len() != params.top_scenes.len() {
        return Err(MediaError::InvalidVideo(format!(
            "Mismatch: {} segment paths but {} top_scenes entries",
            segment_paths.len(),
            params.top_scenes.len()
        )));
    }

    let temp_dir = output.parent().unwrap_or(Path::new("/tmp"));
    let mut styled_paths: Vec<std::path::PathBuf> = Vec::new();

    // Process each segment with its countdown overlay
    for (idx, (segment_path, scene_entry)) in segment_paths.iter().zip(params.top_scenes.iter()).enumerate() {
        let countdown_number = scene_entry.scene_number;
        
        // Verify segment exists
        let segment_exists = segment_path.exists();
        let segment_size = if segment_exists {
            std::fs::metadata(segment_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        
        info!(
            "[STREAMER_TOP_SCENES] Processing segment {} (countdown: {}) path={:?} exists={} size={}",
            idx + 1,
            countdown_number,
            segment_path,
            segment_exists,
            segment_size
        );
        
        if !segment_exists {
            return Err(MediaError::InvalidVideo(format!(
                "Segment {} does not exist at {:?}",
                idx + 1, segment_path
            )));
        }

        // Render with streamer format and countdown overlay
        let styled_path = temp_dir.join(format!("top_scene_{}_styled.mp4", countdown_number));
        render_streamer_format(
            segment_path,
            &styled_path,
            encoding,
            &config,
            Some(countdown_number),
            scene_entry.title.as_deref(),
            watermark,
        )
        .await?;

        styled_paths.push(styled_path);
    }

    // Concatenate all styled segments
    concatenate_segments(&styled_paths, output).await?;

    // Cleanup styled segments
    for path in &styled_paths {
        cleanup_file(path).await;
    }

    // Generate thumbnail
    generate_thumbnail_safe(output).await;

    log_completion("[STREAMER_TOP_SCENES]", pipeline_start, output).await;

    Ok(())
}
