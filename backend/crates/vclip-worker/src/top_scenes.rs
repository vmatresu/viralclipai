//! Top Scenes compilation processing.
//!
//! This module handles the creation of "Top N Scenes" compilation videos
//! that combine multiple selected scenes with countdown overlays.
//!
//! # Compilation Flow
//!
//! 1. Order scenes according to user selection (reversed for countdown)
//! 2. Ensure all raw segments are available (cache or source extraction)
//! 3. Optionally apply silence removal
//! 4. Build `TopSceneEntry` list for the streamer processor
//! 5. Render compilation with countdown overlays
//! 6. Upload to R2 and save metadata to Firestore
//!
//! # Architecture
//!
//! This follows the Single Responsibility Principle - this module ONLY handles
//! Top Scenes compilation. Scene processing and reprocessing logic remain in
//! the `reprocessing` module.

use std::path::PathBuf;

use tracing::{debug, info, warn};

use vclip_models::{Highlight, VideoHighlights};
use vclip_queue::ReprocessScenesJob;

use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;
use crate::raw_segment_cache::raw_segment_r2_key;
use crate::silence_cache::apply_silence_removal_cached;

/// Process Top Scenes compilation job - creates a single video from all selected scenes
/// with countdown overlay (5, 4, 3, 2, 1).
pub async fn process_top_scenes_compilation(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    selected_highlights: &[Highlight],
    video_highlights: &VideoHighlights,
) -> WorkerResult<()> {
    let scene_count = selected_highlights.len();

    ctx.progress
        .log(
            &job.job_id,
            format!("Creating Top Scenes compilation with {} scenes", scene_count),
        )
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 10).await.ok();

    // Create work directories
    let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
    tokio::fs::create_dir_all(&work_dir).await?;
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Order highlights for countdown display
    let ordered_highlights = order_highlights_for_countdown(selected_highlights, &job.scene_ids);

    // Ensure all segments are available
    let raw_segment_paths = ensure_segments_available(
        ctx,
        job,
        &ordered_highlights,
        &work_dir,
        video_highlights.video_url.as_deref(),
    )
    .await?;

    ctx.progress.progress(&job.job_id, 30).await.ok();

    // Apply silence removal if requested
    let final_segment_paths = apply_silence_removal_if_needed(
        ctx,
        job,
        &raw_segment_paths,
        &ordered_highlights,
    )
    .await;

    ctx.progress.progress(&job.job_id, 40).await.ok();

    // Verify all segments exist
    verify_segments_exist(&final_segment_paths, job).await?;

    // Render the compilation
    let (output_path, output_filename) = render_compilation(
        ctx,
        job,
        &clips_dir,
        &final_segment_paths,
        &ordered_highlights,
        scene_count,
    )
    .await?;

    ctx.progress.progress(&job.job_id, 80).await.ok();

    // Upload and finalize
    finalize_compilation(
        ctx,
        job,
        &work_dir,
        &output_path,
        &output_filename,
        &ordered_highlights,
        scene_count,
    )
    .await?;

    Ok(())
}

/// Order highlights for countdown display.
///
/// User selects scenes in order: first selected = #1, last selected = #N
/// In output video: last selected scene appears FIRST with highest countdown number (N)
/// So we reverse the order: [1, 5, 4, 2, 7] becomes [7, 2, 4, 5, 1]
fn order_highlights_for_countdown(
    selected_highlights: &[Highlight],
    scene_ids: &[u32],
) -> Vec<Highlight> {
    scene_ids
        .iter()
        .filter_map(|id| selected_highlights.iter().find(|h| h.id == *id).cloned())
        .rev() // Reverse so last selected appears first in video
        .collect()
}

/// Ensure all raw segments are available (local, R2 cache, or extracted from source).
async fn ensure_segments_available(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    ordered_highlights: &[Highlight],
    work_dir: &PathBuf,
    video_url: Option<&str>,
) -> WorkerResult<Vec<PathBuf>> {
    use vclip_media::intelligent::parse_timestamp;

    ctx.progress
        .log(&job.job_id, "Preparing scene segments...")
        .await
        .ok();

    let mut raw_segment_paths: Vec<PathBuf> = Vec::new();

    for highlight in ordered_highlights {
        let scene_id = highlight.id;
        let raw_segment = work_dir.join(format!("raw_{}.mp4", scene_id));

        // Check if segment already exists locally
        if raw_segment.exists() {
            info!(
                scene_id = scene_id,
                path = ?raw_segment,
                "Using existing local raw segment"
            );
            raw_segment_paths.push(raw_segment);
            continue;
        }

        // Try to download from R2 cache
        let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);

        if ctx.raw_cache.check_raw_exists(&r2_key).await {
            match ctx.storage.download_file(&r2_key, &raw_segment).await {
                Ok(_) => {
                    info!(scene_id = scene_id, "Downloaded raw segment from R2 cache");
                    raw_segment_paths.push(raw_segment);
                    continue;
                }
                Err(e) => {
                    warn!(
                        scene_id = scene_id,
                        error = %e,
                        "Failed to download from R2, will extract from source"
                    );
                }
            }
        }

        // Need to extract from source - download source if needed
        let video_file = work_dir.join("source.mp4");
        if !video_file.exists() {
            if let Some(url) = video_url {
                ctx.progress
                    .log(&job.job_id, "Downloading source video...")
                    .await
                    .ok();
                vclip_media::download_video(url, &video_file)
                    .await
                    .map_err(|e| {
                        WorkerError::job_failed(&format!("Failed to download source: {}", e))
                    })?;
            } else {
                return Err(WorkerError::job_failed("No source video available"));
            }
        }

        // Extract segment
        let start_secs = parse_timestamp(&highlight.start).unwrap_or(0.0);
        let end_secs = parse_timestamp(&highlight.end).unwrap_or(30.0);
        let padded_start = (start_secs - highlight.pad_before).max(0.0);
        let padded_end = end_secs + highlight.pad_after;
        let padded_start_ts = format_timestamp(padded_start);
        let padded_end_ts = format_timestamp(padded_end);

        let (seg_path, _) = ctx
            .raw_cache
            .get_or_create_with_outcome(
                &job.user_id,
                job.video_id.as_str(),
                scene_id,
                &video_file,
                &padded_start_ts,
                &padded_end_ts,
                work_dir,
            )
            .await?;

        raw_segment_paths.push(seg_path);
    }

    Ok(raw_segment_paths)
}

/// Apply silence removal if requested.
async fn apply_silence_removal_if_needed(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    raw_segment_paths: &[PathBuf],
    ordered_highlights: &[Highlight],
) -> Vec<PathBuf> {
    if !job.cut_silent_parts {
        return raw_segment_paths.to_vec();
    }

    ctx.progress
        .log(&job.job_id, "Removing silent parts from segments...")
        .await
        .ok();

    let mut processed_paths: Vec<PathBuf> = Vec::new();

    for (idx, (raw_path, highlight)) in raw_segment_paths
        .iter()
        .zip(ordered_highlights.iter())
        .enumerate()
    {
        let scene_id = highlight.id;
        match apply_silence_removal_cached(
            ctx,
            raw_path,
            scene_id,
            &job.job_id,
            &job.user_id,
            job.video_id.as_str(),
        )
        .await
        {
            Ok(Some(silence_removed_path)) => {
                info!(
                    scene_id = scene_id,
                    idx = idx,
                    "Using silence-removed segment for Top Scenes compilation"
                );
                processed_paths.push(silence_removed_path);
            }
            Ok(None) => {
                debug!(
                    scene_id = scene_id,
                    idx = idx,
                    "Silence removal not applied (no significant silence or too short)"
                );
                processed_paths.push(raw_path.clone());
            }
            Err(e) => {
                warn!(
                    scene_id = scene_id,
                    idx = idx,
                    error = %e,
                    "Silence removal failed, using original segment"
                );
                processed_paths.push(raw_path.clone());
            }
        }
    }

    processed_paths
}

/// Verify all segment paths exist and are non-empty.
async fn verify_segments_exist(
    final_segment_paths: &[PathBuf],
    job: &ReprocessScenesJob,
) -> WorkerResult<()> {
    info!(
        video_id = %job.video_id,
        segment_count = final_segment_paths.len(),
        cut_silent_parts = job.cut_silent_parts,
        "Verifying segments for Top Scenes compilation"
    );

    for (idx, path) in final_segment_paths.iter().enumerate() {
        let exists = path.exists();
        let size = if exists {
            tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        info!(
            idx = idx,
            path = ?path,
            exists = exists,
            size_bytes = size,
            "Segment path for compilation"
        );

        if !exists {
            return Err(WorkerError::job_failed(&format!(
                "Segment {} does not exist at {:?}",
                idx, path
            )));
        }
    }

    Ok(())
}

/// Render the Top Scenes compilation video.
async fn render_compilation(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    final_segment_paths: &[PathBuf],
    ordered_highlights: &[Highlight],
    scene_count: usize,
) -> WorkerResult<(PathBuf, String)> {
    use vclip_models::{EncodingConfig, StreamerParams, TopSceneEntry};

    // Build TopSceneEntry list for the streamer processor
    let top_scenes: Vec<TopSceneEntry> = ordered_highlights
        .iter()
        .enumerate()
        .map(|(idx, h)| {
            let countdown_num = (scene_count - idx) as u8; // 5, 4, 3, 2, 1
            TopSceneEntry {
                scene_number: countdown_num,
                start: h.start.clone(),
                end: h.end.clone(),
                title: None,
            }
        })
        .collect();

    // Create unique output filename with timestamp to avoid overwriting
    let first_title = ordered_highlights
        .first()
        .map(|h| vclip_models::sanitize_filename_title(&h.title))
        .unwrap_or_else(|| "compilation".to_string());
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let output_filename = format!(
        "top_{}_scenes_{}_{}_streamer_top_scenes.mp4",
        scene_count, first_title, timestamp
    );
    let output_path = clips_dir.join(&output_filename);

    ctx.progress
        .log(
            &job.job_id,
            format!("Rendering Top {} compilation...", scene_count),
        )
        .await
        .ok();

    // Call the streamer top scenes processor
    let streamer_params = StreamerParams::top_scenes(top_scenes);
    let encoding = EncodingConfig::default().with_crf(24);

    vclip_media::styles::streamer::process_top_scenes_from_segments(
        final_segment_paths,
        &output_path,
        &encoding,
        &streamer_params,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Top Scenes compilation failed");
        WorkerError::job_failed(&format!("Failed to render compilation: {}", e))
    })?;

    Ok((output_path, output_filename))
}

/// Finalize the compilation: upload, save metadata, cleanup.
async fn finalize_compilation(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    work_dir: &PathBuf,
    output_path: &PathBuf,
    output_filename: &str,
    ordered_highlights: &[Highlight],
    scene_count: usize,
) -> WorkerResult<()> {
    use vclip_media::intelligent::parse_timestamp;
    use vclip_models::{ClipMetadata, ClipStatus, Style};

    // Generate thumbnail
    let thumb_path = output_path.with_extension("jpg");
    if let Err(e) = vclip_media::thumbnail::generate_thumbnail(output_path, &thumb_path).await {
        warn!(error = %e, "Failed to generate thumbnail for compilation");
    }

    // Upload to R2
    ctx.progress
        .log(&job.job_id, "Uploading compilation...")
        .await
        .ok();

    let r2_key = ctx
        .storage
        .upload_clip(output_path, &job.user_id, job.video_id.as_str(), output_filename)
        .await
        .map_err(|e| WorkerError::Storage(e))?;

    // Upload thumbnail if exists
    let thumb_key = if thumb_path.exists() {
        let thumb_filename = output_filename.replace(".mp4", ".jpg");
        match ctx
            .storage
            .upload_clip(&thumb_path, &job.user_id, job.video_id.as_str(), &thumb_filename)
            .await
        {
            Ok(key) => Some(key),
            Err(e) => {
                warn!(error = %e, "Failed to upload thumbnail (non-critical)");
                None
            }
        }
    } else {
        None
    };

    ctx.progress.progress(&job.job_id, 90).await.ok();

    // Calculate total duration
    let total_duration: f64 = ordered_highlights
        .iter()
        .map(|h| {
            let start = parse_timestamp(&h.start).unwrap_or(0.0);
            let end = parse_timestamp(&h.end).unwrap_or(30.0);
            (end - start) + h.pad_before + h.pad_after
        })
        .sum();

    // Get file size
    let file_size = tokio::fs::metadata(output_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // Create clip metadata
    let timestamp_str = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let clip_id = format!(
        "{}_compilation_{}_{}",
        job.video_id, timestamp_str, "streamer_top_scenes"
    );

    let clip_meta = ClipMetadata {
        clip_id: clip_id.clone(),
        video_id: job.video_id.clone(),
        user_id: job.user_id.clone(),
        scene_id: 0, // Special: 0 indicates compilation
        scene_title: format!("Top {} Scenes", scene_count),
        scene_description: Some(format!(
            "Compilation of scenes: {}",
            ordered_highlights
                .iter()
                .map(|h| h.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        filename: output_filename.to_string(),
        style: Style::StreamerTopScenes.to_string(),
        priority: 0,
        start_time: "00:00:00".to_string(),
        end_time: format_timestamp(total_duration),
        duration_seconds: total_duration,
        file_size_bytes: file_size,
        file_size_mb: file_size as f64 / (1024.0 * 1024.0),
        has_thumbnail: thumb_key.is_some(),
        r2_key,
        thumbnail_r2_key: thumb_key,
        raw_r2_key: None,
        status: ClipStatus::Completed,
        created_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        updated_at: Some(chrono::Utc::now()),
        created_by: job.user_id.clone(),
    };

    // Save to Firestore
    let clip_repo = vclip_firestore::ClipRepository::new(
        ctx.firestore.clone(),
        &job.user_id,
        job.video_id.clone(),
    );
    clip_repo
        .create(&clip_meta)
        .await
        .map_err(|e| WorkerError::Firestore(e))?;

    // Update video clip count
    crate::reprocessing::update_video_clip_count(ctx, job, 1).await?;

    // Update storage accounting
    let storage_repo = vclip_firestore::StorageAccountingRepository::new(
        ctx.firestore.clone(),
        &job.user_id,
    );
    if let Err(e) = storage_repo.add_styled_clip(file_size).await {
        warn!(error = %e, "Failed to update storage accounting (non-critical)");
    }

    // Cleanup work directory
    if work_dir.exists() {
        tokio::fs::remove_dir_all(work_dir).await.ok();
    }

    ctx.progress.progress(&job.job_id, 100).await.ok();
    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    info!(
        video_id = %job.video_id,
        scene_count = scene_count,
        duration_sec = total_duration,
        file_size_mb = file_size as f64 / (1024.0 * 1024.0),
        "Top Scenes compilation completed"
    );

    Ok(())
}

/// Format seconds as HH:MM:SS.mmm timestamp for FFmpeg.
fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}
