use std::collections::{HashMap, HashSet};
use std::path::Path;

use tracing::info;
use vclip_firestore::{ClipRepository, VideoRepository};
use vclip_models::ClipStatus;
use vclip_models::ClipTask;
use vclip_queue::ProcessVideoJob;

use crate::error::WorkerResult;
use crate::processor::{AnalysisData, EnhancedProcessingContext};

pub mod tasks;
pub mod scene;
pub mod clip;

pub use scene::SceneProcessingResults;
pub use scene::process_scene;
pub use clip::process_single_clip;

pub struct ClipProcessingResults {
    pub total_processed: usize,
    pub completed_count: u32,
}

/// Orchestrate processing of all clips for a job by grouping per scene and dispatching.
pub async fn process_clips(
    ctx: &EnhancedProcessingContext,
    job: &ProcessVideoJob,
    work_dir: &Path,
    analysis: &AnalysisData,
) -> WorkerResult<ClipProcessingResults> {
    let clip_tasks = tasks::generate_clip_tasks(
        &analysis.highlights,
        &job.styles,
        &job.crop_mode,
        &job.target_aspect,
    );
    let total_clips = clip_tasks.len();
    let video_repo = VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    if let Err(e) = video_repo
        .set_expected_clips(&job.video_id, total_clips as u32)
        .await
    {
        tracing::warn!(
            video_id = %job.video_id,
            total_clips,
            error = %e,
            "Failed to set expected clips; progress tracking may be inaccurate"
        );
    }

    ctx.progress
        .log(
            &job.job_id,
            format!(
                "Generating {} clips from {} highlights...",
                total_clips,
                analysis.highlights.highlights.len()
            ),
        )
        .await
        .ok();

    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Group clips by scene for parallel processing
    let mut scene_groups: HashMap<u32, Vec<&ClipTask>> = HashMap::new();
    for task in &clip_tasks {
        scene_groups.entry(task.scene_id).or_default().push(task);
    }

    // Load existing completed clips to enable skip-on-resume.
    let existing_completed: HashSet<String> = match ClipRepository::new(
        ctx.firestore.clone(),
        &job.user_id,
        job.video_id.clone(),
    )
    .list(Some(ClipStatus::Completed))
    .await
    {
        Ok(clips) => clips.into_iter().map(|c| c.clip_id).collect(),
        Err(e) => {
            info!("Failed to list completed clips (will process all): {}", e);
            HashSet::new()
        }
    };

    if !existing_completed.is_empty() {
        info!(
            existing = existing_completed.len(),
            "Found existing completed clips, will skip those tasks"
        );
    }

    let mut scene_ids: Vec<u32> = scene_groups.keys().copied().collect();
    scene_ids.sort();

    let mut completed_clips = existing_completed.len() as u32;
    let mut processed_count = existing_completed.len();

    // Process each scene
    for scene_id in scene_ids {
        let scene_tasks = scene_groups.get(&scene_id).unwrap();
        let scene_results = scene::process_scene(
            ctx,
            job,
            &clips_dir,
            &analysis.video_file,
            scene_tasks,
            &existing_completed,
            total_clips,
        )
        .await?;

        processed_count += scene_results.processed;
        completed_clips += scene_results.completed;

        if let Err(e) = video_repo
            .update_clips_count(&job.video_id, completed_clips)
            .await
        {
            tracing::warn!(
                video_id = %job.video_id,
                error = %e,
                "Failed to update clips_count after scene"
            );
        }

        let progress = 40 + (processed_count * 55 / total_clips) as u32;
        ctx.progress.progress(&job.job_id, progress as u8).await.ok();
    }

    Ok(ClipProcessingResults {
        total_processed: processed_count,
        completed_count: completed_clips,
    })
}

