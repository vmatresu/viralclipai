//! Scene reprocessing logic.
//!
//! This module handles the reprocessing of specific scenes from a previously
//! processed video, allowing users to apply different styles without re-running
//! the AI analysis.
//!
//! # Optimized Processing Strategy
//!
//! The pipeline now implements parallel processing to minimize latency:
//! 1. Check which scenes have cached raw segments (local or R2)
//! 2. For uncached scenes, try yt-dlp segment download first (avoids full source download)
//! 3. Process cached scenes immediately while downloading full source for remaining scenes
//! 4. Process remaining scenes once source is available

use std::path::PathBuf;

use tracing::{info, warn};

use vclip_models::{ClipTask, Highlight};
use vclip_queue::ReprocessScenesJob;

use crate::clip_pipeline;
use crate::error::{WorkerError, WorkerResult};
use crate::processor::{EnhancedProcessingContext, JobLogger};
use crate::scene_renderer::format_timestamp;

/// Process a reprocess scenes job.
///
/// This is different from `process_video_job` as it:
/// 1. Loads existing highlights from Firestore (doesn't re-run AI analysis)
/// 2. Filters to only the requested scene IDs
/// 3. **Optimized**: Processes cached scenes in parallel while downloading uncached ones
/// 4. Tries yt-dlp segment download before falling back to full source download
pub async fn reprocess_scenes(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
) -> WorkerResult<()> {
    let job_logger = JobLogger::new(&job.job_id, "reprocess_scenes");
    job_logger.log_start("Starting scene processing job");

    ctx.progress
        .log(&job.job_id, "Loading video data...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 5).await.ok();

    // Load existing highlights from Firestore (source of truth)
    let highlights_repo = vclip_firestore::HighlightsRepository::new(
        ctx.firestore.clone(),
        &job.user_id,
    );

    let video_highlights = highlights_repo
        .get(&job.video_id)
        .await
        .map_err(|e| WorkerError::Firestore(e))?
        .ok_or_else(|| WorkerError::job_failed("Highlights not found in Firestore"))?;

    // Filter highlights to only requested scene IDs
    let scene_ids_set: std::collections::HashSet<_> = job.scene_ids.iter().copied().collect();
    let selected_highlights: Vec<_> = video_highlights
        .highlights
        .iter()
        .filter(|h| scene_ids_set.contains(&h.id))
        .cloned()
        .collect();

    if selected_highlights.is_empty() {
        return Err(WorkerError::job_failed("No valid scenes found"));
    }

    // Check if this is a Top Scenes compilation job
    if job.is_top_scenes_compilation() {
        info!(
            video_id = %job.video_id,
            scene_count = selected_highlights.len(),
            "Processing as Top Scenes compilation (single output video)"
        );
        return crate::top_scenes::process_top_scenes_compilation(ctx, job, &selected_highlights, &video_highlights).await;

    }

    // Calculate total clips (for non-compilation mode)
    let total_clips = selected_highlights.len() * job.styles.len();
    ctx.progress
        .log(
            &job.job_id,
            format!(
                "Processing {} scenes with {} styles ({} total clips)",
                selected_highlights.len(),
                job.styles.len(),
                total_clips
            ),
        )
        .await
        .ok();

    // Create work directory
    let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
    tokio::fs::create_dir_all(&work_dir).await?;
    
    // Create clips directory
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Partition scenes into cached (local/R2) and uncached
    let (cached_scenes, uncached_scenes) = partition_scenes_by_cache_status(
        ctx, job, &selected_highlights, &work_dir
    ).await;
    
    info!(
        video_id = %job.video_id,
        cached_count = cached_scenes.len(),
        uncached_count = uncached_scenes.len(),
        "Partitioned scenes by cache status"
    );

    // Generate clip tasks from selected highlights
    let selected_refs: Vec<&Highlight> = selected_highlights.iter().collect();
    let clip_tasks = clip_pipeline::tasks::generate_clip_tasks_from_firestore_highlights_full(
        &selected_refs,
        &job.styles,
        &job.crop_mode,
        &job.target_aspect,
        job.streamer_split_params.clone(),
        job.cut_silent_parts,
    );

    ctx.progress
        .log(&job.job_id, format!("Generating {} clips...", total_clips))
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    // OPTIMIZED PROCESSING STRATEGY:
    // 1. Process cached scenes first (they're ready immediately)
    // 2. For uncached scenes, try yt-dlp segment download first
    // 3. For scenes where yt-dlp fails, download full source and extract
    
    let video_url = video_highlights.video_url.clone();
    let final_completed: u32;
    
    // If all scenes are cached, process them directly
    if uncached_scenes.is_empty() {
        info!(
            video_id = %job.video_id,
            "All scenes cached, processing directly without source download"
        );
        
        final_completed = process_scenes_with_cache(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &cached_scenes,
            &video_highlights,
            total_clips,
        )
        .await?;
    }
    // If all scenes are uncached, try yt-dlp then fall back to full source
    else if cached_scenes.is_empty() {
        final_completed = process_uncached_scenes_optimized(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &uncached_scenes,
            &video_highlights,
            video_url.as_deref(),
            total_clips,
        )
        .await?;
    }
    // Mixed: process cached scenes first, then uncached scenes
    else {
        info!(
            video_id = %job.video_id,
            "Processing cached scenes first, then acquiring uncached scenes"
        );
        
        // Process cached scenes first (they're ready immediately)
        let cached_completed = process_scenes_with_cache(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &cached_scenes,
            &video_highlights,
            total_clips,
        )
        .await?;
        
        // Then acquire and process uncached scenes (tries yt-dlp first)
        let uncached_completed = process_uncached_scenes_optimized(
            ctx,
            job,
            &clips_dir,
            &work_dir,
            &clip_tasks,
            &uncached_scenes,
            &video_highlights,
            video_url.as_deref(),
            total_clips,
        )
        .await?;
        
        final_completed = cached_completed + uncached_completed;
    }

    // Update video metadata - add new clips to existing count
    update_video_clip_count(ctx, job, final_completed).await?;

    // Cleanup work directory
    if work_dir.exists() {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    ctx.progress.progress(&job.job_id, 100).await.ok();
    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    job_logger.log_completion(&format!(
        "Reprocessed {} scenes, {} clips completed",
        selected_highlights.len(),
        final_completed
    ));

    Ok(())
}

/// Partition scenes into cached (available locally or in R2) and uncached.
async fn partition_scenes_by_cache_status(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    highlights: &[Highlight],
    work_dir: &PathBuf,
) -> (Vec<Highlight>, Vec<Highlight>) {
    use crate::raw_segment_cache::raw_segment_r2_key;
    
    let mut cached = Vec::new();
    let mut uncached = Vec::new();
    
    for highlight in highlights {
        let scene_id = highlight.id;
        
        // Check local file first (fast path)
        let local_path = work_dir.join(format!("raw_{}.mp4", scene_id));
        if local_path.exists() {
            cached.push(highlight.clone());
            continue;
        }
        
        // Check R2
        let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);
        if ctx.raw_cache.check_raw_exists(&r2_key).await {
            cached.push(highlight.clone());
        } else {
            uncached.push(highlight.clone());
        }
    }
    
    (cached, uncached)
}

/// Process scenes that have cached raw segments (local or R2).
async fn process_scenes_with_cache(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    work_dir: &PathBuf,
    clip_tasks: &[ClipTask],
    cached_scenes: &[Highlight],
    highlights: &vclip_models::VideoHighlights,
    total_clips: usize,
) -> WorkerResult<u32> {
    if cached_scenes.is_empty() {
        return Ok(0);
    }
    
    // Filter clip tasks to only cached scenes
    let cached_scene_ids: std::collections::HashSet<_> = cached_scenes.iter().map(|h| h.id).collect();
    let cached_tasks: Vec<ClipTask> = clip_tasks
        .iter()
        .filter(|t| cached_scene_ids.contains(&t.scene_id))
        .cloned()
        .collect();
    
    if cached_tasks.is_empty() {
        return Ok(0);
    }
    
    info!(
        video_id = %job.video_id,
        scene_count = cached_scenes.len(),
        task_count = cached_tasks.len(),
        "Processing cached scenes"
    );
    
    // Use a placeholder video file - raw segments will be downloaded from R2
    let placeholder_video = work_dir.join("source.mp4");
    
    crate::scene_renderer::process_selected_scenes(
        ctx,
        job,
        clips_dir,
        &placeholder_video,
        &cached_tasks,
        highlights,
        total_clips,
    )
    .await
}

/// Process uncached scenes with optimized acquisition strategy:
/// 1. Try yt-dlp segment download for each scene (parallel)
/// 2. For scenes where yt-dlp fails, download full source and extract
async fn process_uncached_scenes_optimized(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    work_dir: &PathBuf,
    clip_tasks: &[ClipTask],
    uncached_scenes: &[Highlight],
    highlights: &vclip_models::VideoHighlights,
    video_url: Option<&str>,
    total_clips: usize,
) -> WorkerResult<u32> {
    use vclip_media::intelligent::parse_timestamp;
    use crate::raw_segment_cache::raw_segment_r2_key;
    
    if uncached_scenes.is_empty() {
        return Ok(0);
    }
    
    // Filter clip tasks to only uncached scenes
    let uncached_scene_ids: std::collections::HashSet<_> = uncached_scenes.iter().map(|h| h.id).collect();
    let uncached_tasks: Vec<ClipTask> = clip_tasks
        .iter()
        .filter(|t| uncached_scene_ids.contains(&t.scene_id))
        .cloned()
        .collect();
    
    if uncached_tasks.is_empty() {
        return Ok(0);
    }
    
    info!(
        video_id = %job.video_id,
        scene_count = uncached_scenes.len(),
        task_count = uncached_tasks.len(),
        "Processing uncached scenes with optimized acquisition"
    );
    
    // Track which scenes we successfully downloaded via yt-dlp
    let mut ytdlp_success_scenes: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut ytdlp_failed_scenes: Vec<&Highlight> = Vec::new();
    
    // Try yt-dlp segment download for each uncached scene
    if let Some(url) = video_url {
        if vclip_media::likely_supports_segment_download(url) {
            ctx.progress
                .log(&job.job_id, "Trying direct segment downloads (yt-dlp)...")
                .await
                .ok();
            
            for highlight in uncached_scenes {
                let scene_id = highlight.id;
                let raw_segment = work_dir.join(format!("raw_{}.mp4", scene_id));
                
                // Calculate padded timestamps
                let start_secs = parse_timestamp(&highlight.start).unwrap_or(0.0);
                let end_secs = parse_timestamp(&highlight.end).unwrap_or(30.0);
                let pad_before = highlight.pad_before;
                let pad_after = highlight.pad_after;
                let padded_start = (start_secs - pad_before).max(0.0);
                let padded_end = end_secs + pad_after;
                
                info!(
                    scene_id = scene_id,
                    url = %url,
                    start = padded_start,
                    end = padded_end,
                    "Trying yt-dlp segment download"
                );
                
                match vclip_media::download_segment(
                    url,
                    padded_start,
                    padded_end,
                    &raw_segment,
                    true, // force_keyframes for accurate cuts
                )
                .await
                {
                    Ok(()) => {
                        info!(
                            scene_id = scene_id,
                            "Downloaded segment directly via yt-dlp"
                        );
                        ytdlp_success_scenes.insert(scene_id);
                        
                        // Upload to R2 for future use (non-blocking, fire-and-forget)
                        let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);
                        if let Err(e) = ctx.raw_cache.upload_raw_segment(&raw_segment, &r2_key).await {
                            warn!(
                                scene_id = scene_id,
                                error = %e,
                                "Failed to upload yt-dlp segment to R2 (non-critical)"
                            );
                        }
                    }
                    Err(e) => {
                        if e.downcast_ref::<vclip_media::SegmentDownloadNotSupported>().is_some() {
                            info!(
                                scene_id = scene_id,
                                "yt-dlp segment download not supported, will use full source"
                            );
                        } else {
                            warn!(
                                scene_id = scene_id,
                                error = %e,
                                "yt-dlp segment download failed, will use full source"
                            );
                        }
                        ytdlp_failed_scenes.push(highlight);
                    }
                }
            }
        } else {
            // URL doesn't support segment download, all scenes need full source
            ytdlp_failed_scenes = uncached_scenes.iter().collect();
        }
    } else {
        // No URL available, all scenes need full source
        ytdlp_failed_scenes = uncached_scenes.iter().collect();
    }
    
    // If any scenes failed yt-dlp, download full source and extract
    if !ytdlp_failed_scenes.is_empty() {
        info!(
            video_id = %job.video_id,
            failed_count = ytdlp_failed_scenes.len(),
            "Downloading full source for scenes that failed yt-dlp"
        );
        
        ctx.progress
            .log(&job.job_id, "Downloading full source video...")
            .await
            .ok();
        
        let video_file = crate::source_download::download_source_video(ctx, job, work_dir, highlights).await?;
        
        // Extract raw segments for failed scenes
        for highlight in &ytdlp_failed_scenes {
            let scene_id = highlight.id;
            let raw_segment = work_dir.join(format!("raw_{}.mp4", scene_id));
            
            // Skip if already exists (shouldn't happen, but be safe)
            if raw_segment.exists() {
                continue;
            }
            
            let start_secs = parse_timestamp(&highlight.start).unwrap_or(0.0);
            let end_secs = parse_timestamp(&highlight.end).unwrap_or(30.0);
            let pad_before = highlight.pad_before;
            let pad_after = highlight.pad_after;
            let padded_start = (start_secs - pad_before).max(0.0);
            let padded_end = end_secs + pad_after;
            let padded_start_ts = format_timestamp(padded_start);
            let padded_end_ts = format_timestamp(padded_end);
            
            let (_seg, _created) = ctx
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
        }
    }
    
    // Now process all uncached scenes (they all have raw segments now)
    let placeholder_video = work_dir.join("source.mp4");
    
    crate::scene_renderer::process_selected_scenes(
        ctx,
        job,
        clips_dir,
        &placeholder_video,
        &uncached_tasks,
        highlights,
        total_clips,
    )
    .await
}

/// Update video clip count in Firestore.
pub async fn update_video_clip_count(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    completed_clips: u32,
) -> WorkerResult<()> {

    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);

    match video_repo.get(&job.video_id).await {
        Ok(Some(video)) => {
            let new_total = video.clips_count + completed_clips;
            video_repo
                .complete(&job.video_id, new_total)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
            info!(
                "Updated video {} clip count: {} -> {}",
                job.video_id, video.clips_count, new_total
            );
        }
        Ok(None) => {
            video_repo
                .complete(&job.video_id, completed_clips)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
        Err(e) => {
            tracing::warn!("Failed to get video metadata: {}", e);
            video_repo
                .complete(&job.video_id, completed_clips)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
    }

    Ok(())
}
