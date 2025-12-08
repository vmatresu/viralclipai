//! Scene reprocessing logic.
//!
//! This module handles the reprocessing of specific scenes from a previously
//! processed video, allowing users to apply different styles without re-running
//! the AI analysis.

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::info;

use vclip_media::download_video;
use vclip_models::{ClipStatus, ClipTask};
use vclip_queue::ReprocessScenesJob;

use crate::clip_pipeline;
use crate::error::{WorkerError, WorkerResult};
use crate::processor::{EnhancedProcessingContext, JobLogger};

/// Process a reprocess scenes job.
///
/// This is different from `process_video_job` as it:
/// 1. Loads existing highlights from storage (doesn't re-run AI analysis)
/// 2. Filters to only the requested scene IDs
/// 3. Downloads video from R2 or original URL (doesn't re-download from YouTube)
/// 4. Only processes the selected scenes with the requested styles
pub async fn reprocess_scenes(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
) -> WorkerResult<()> {
    let job_logger = JobLogger::new(&job.job_id, "reprocess_scenes");
    job_logger.log_start("Starting scene reprocessing job");

    ctx.progress
        .log(&job.job_id, "Loading video data...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 5).await.ok();

    // Load existing highlights from storage
    let highlights = ctx
        .storage
        .load_highlights(&job.user_id, job.video_id.as_str())
        .await
        .map_err(|e| WorkerError::Storage(e))?;

    // Filter highlights to only requested scene IDs
    let scene_ids_set: std::collections::HashSet<_> = job.scene_ids.iter().copied().collect();
    let selected_highlights: Vec<_> = highlights
        .highlights
        .iter()
        .filter(|h| scene_ids_set.contains(&h.id))
        .collect();

    if selected_highlights.is_empty() {
        return Err(WorkerError::job_failed("No valid scenes found"));
    }

    // Calculate total clips
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

    // Download source video
    let video_file = download_source_video(ctx, job, &work_dir, &highlights).await?;

    ctx.progress.progress(&job.job_id, 25).await.ok();

    // Generate clip tasks from selected highlights only
    let clip_tasks = clip_pipeline::tasks::generate_clip_tasks_from_highlights(
        &selected_highlights,
        &job.styles,
        &job.crop_mode,
        &job.target_aspect,
    );

    ctx.progress
        .log(&job.job_id, format!("Generating {} clips...", total_clips))
        .await
        .ok();

    // Create clips directory
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Process scenes
    let completed_clips = process_selected_scenes(
        ctx,
        job,
        &clips_dir,
        &video_file,
        &clip_tasks,
        &highlights,
        total_clips,
    )
    .await?;

    // Update video metadata - add new clips to existing count
    update_video_clip_count(ctx, job, completed_clips).await?;

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
        completed_clips
    ));

    Ok(())
}

/// Download source video from R2 or original URL.
async fn download_source_video(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    work_dir: &PathBuf,
    highlights: &vclip_storage::HighlightsData,
) -> WorkerResult<PathBuf> {
    ctx.progress
        .log(&job.job_id, "Downloading source video...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    let video_file = work_dir.join("source.mp4");

    // Try to get source video from R2 first
    let source_key = format!("{}/{}/source.mp4", job.user_id, job.video_id.as_str());
    match ctx.storage.download_file(&source_key, &video_file).await {
        Ok(_) => {
            info!("Downloaded source video from R2: {}", source_key);
            Ok(video_file)
        }
        Err(r2_error) => {
            info!(
                "Source video not found in R2 ({}), trying original URL from highlights",
                r2_error
            );

            // Fall back to original video URL from highlights data
            if let Some(ref video_url) = highlights.video_url {
                ctx.progress
                    .log(&job.job_id, "Downloading original video from source URL...")
                    .await
                    .ok();

                match download_video(video_url, &video_file).await {
                    Ok(_) => {
                        info!("Downloaded source video from original URL: {}", video_url);
                        Ok(video_file)
                    }
                    Err(url_error) => {
                        let err_msg = format!(
                            "Source video not found in R2: {}. Failed to download from original URL {}: {}",
                            r2_error, video_url, url_error
                        );
                        ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                        Err(WorkerError::job_failed(&err_msg))
                    }
                }
            } else {
                let err_msg = format!(
                    "Source video not found in R2: {}. No original video URL available in highlights data.",
                    r2_error
                );
                ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                Err(WorkerError::job_failed(&err_msg))
            }
        }
    }
}

/// Process all selected scenes with parallel style processing.
async fn process_selected_scenes(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    video_file: &PathBuf,
    clip_tasks: &[ClipTask],
    highlights: &vclip_storage::HighlightsData,
    total_clips: usize,
) -> WorkerResult<u32> {
    // Load existing completed clips for skip-on-resume.
    let existing_completed: std::collections::HashSet<String> =
        match vclip_firestore::ClipRepository::new(
            ctx.firestore.clone(),
            &job.user_id,
            job.video_id.clone(),
        )
        .list(Some(ClipStatus::Completed))
        .await
        {
            Ok(clips) => clips.into_iter().map(|c| c.clip_id).collect(),
            Err(e) => {
                tracing::info!("Failed to list completed clips (processing all): {}", e);
                std::collections::HashSet::new()
            }
        };

    // Group clips by scene_id for parallel processing of styles within each scene
    let mut scene_groups: HashMap<u32, Vec<&ClipTask>> = HashMap::new();
    for task in clip_tasks {
        scene_groups.entry(task.scene_id).or_default().push(task);
    }

    // Sort scenes by scene_id for deterministic ordering
    let mut scene_ids: Vec<u32> = scene_groups.keys().copied().collect();
    scene_ids.sort();

    // Process clips with parallel style processing within each scene
    let mut completed_clips = 0u32;
    let mut processed_count = 0usize;

    for scene_id in scene_ids {
        let scene_tasks = scene_groups.get(&scene_id).unwrap();

        ctx.progress
            .log(
                &job.job_id,
                format!(
                    "Processing scene {} ({} styles in parallel)...",
                    scene_id,
                    scene_tasks.len()
                ),
            )
            .await
            .ok();

        // Create a temporary ProcessVideoJob for the scene processor
        let temp_job = vclip_queue::ProcessVideoJob {
            job_id: job.job_id.clone(),
            user_id: job.user_id.clone(),
            video_id: job.video_id.clone(),
            video_url: highlights.video_url.clone().unwrap_or_default(),
            styles: job.styles.clone(),
            crop_mode: job.crop_mode.clone(),
            target_aspect: job.target_aspect.clone(),
            custom_prompt: None,
        };

        // Process scene using the shared scene processor
        let scene_results = clip_pipeline::process_scene(
            ctx,
            &temp_job,
            clips_dir,
            video_file,
            scene_tasks,
            &existing_completed,
            total_clips,
        )
        .await?;

        processed_count += scene_results.processed;
        completed_clips += scene_results.completed;

        // Update progress after each scene (25% to 95%)
        let progress = 25 + (processed_count * 70 / total_clips) as u32;
        ctx.progress
            .progress(&job.job_id, progress as u8)
            .await
            .ok();
    }

    Ok(completed_clips)
}

/// Update video clip count in Firestore.
async fn update_video_clip_count(
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
