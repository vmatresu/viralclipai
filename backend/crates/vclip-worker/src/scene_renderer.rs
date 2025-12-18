//! Scene rendering and style processing.
//!
//! This module handles rendering scenes with parallel style processing,
//! including raw segment caching and silence removal.
//!
//! # Architecture
//!
//! The scene renderer:
//! 1. Groups clips by scene ID for parallel style processing
//! 2. Gets or creates cached raw segments
//! 3. Applies silence removal if requested
//! 4. Processes each style in parallel for efficiency
//! 5. Tracks storage accounting for new segments

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use tracing::{debug, info, warn};

use vclip_models::{ClipStatus, ClipTask, ProcessingProgress, VideoHighlights};
use vclip_queue::ReprocessScenesJob;

use crate::clip_pipeline;
use crate::error::WorkerResult;
use crate::processor::EnhancedProcessingContext;
use crate::raw_segment_cache::raw_segment_r2_key;
use crate::silence_cache::apply_silence_removal_cached;

/// Minimum interval between Firestore progress updates to avoid excessive writes.
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_secs(5);

/// Progress tracker for Firestore updates with throttling.
pub struct ProgressTracker {
    video_repo: Arc<vclip_firestore::VideoRepository>,
    video_id: vclip_models::VideoId,
    total_scenes: u32,
    total_clips: u32,
    completed_scenes: AtomicU32,
    completed_clips: AtomicU32,
    failed_clips: AtomicU32,
    current_scene_id: Mutex<Option<u32>>,
    current_scene_title: Mutex<Option<String>>,
    last_update: Mutex<Instant>,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl ProgressTracker {
    /// Create a new progress tracker.
    pub fn new(
        video_repo: Arc<vclip_firestore::VideoRepository>,
        video_id: vclip_models::VideoId,
        total_scenes: u32,
        total_clips: u32,
    ) -> Self {
        Self {
            video_repo,
            video_id,
            total_scenes,
            total_clips,
            completed_scenes: AtomicU32::new(0),
            completed_clips: AtomicU32::new(0),
            failed_clips: AtomicU32::new(0),
            current_scene_id: Mutex::new(None),
            current_scene_title: Mutex::new(None),
            last_update: Mutex::new(Instant::now() - PROGRESS_UPDATE_INTERVAL), // Allow immediate first update
            started_at: chrono::Utc::now(),
        }
    }

    /// Mark a scene as started.
    pub async fn scene_started(&self, scene_id: u32, scene_title: Option<String>) {
        *self.current_scene_id.lock().await = Some(scene_id);
        *self.current_scene_title.lock().await = scene_title;
        self.maybe_update_firestore().await;
    }

    /// Mark a scene as completed with clip counts.
    pub async fn scene_completed(&self, clips_succeeded: u32, clips_failed: u32) {
        self.completed_scenes.fetch_add(1, Ordering::Relaxed);
        self.completed_clips.fetch_add(clips_succeeded, Ordering::Relaxed);
        self.failed_clips.fetch_add(clips_failed, Ordering::Relaxed);
        *self.current_scene_id.lock().await = None;
        *self.current_scene_title.lock().await = None;
        // Always update after scene completion (important milestone)
        self.force_update_firestore().await;
    }

    /// Set an error message.
    pub async fn set_error(&self, error_message: &str) {
        if let Err(e) = self.video_repo.set_progress_error(&self.video_id, error_message).await {
            warn!(video_id = %self.video_id, error = %e, "Failed to set progress error in Firestore");
        }
    }

    /// Update Firestore if enough time has elapsed since last update.
    async fn maybe_update_firestore(&self) {
        let mut last_update = self.last_update.lock().await;
        if last_update.elapsed() < PROGRESS_UPDATE_INTERVAL {
            return;
        }
        *last_update = Instant::now();
        drop(last_update);
        self.do_update_firestore().await;
    }

    /// Force update Firestore regardless of throttling.
    async fn force_update_firestore(&self) {
        *self.last_update.lock().await = Instant::now();
        self.do_update_firestore().await;
    }

    /// Perform the actual Firestore update.
    async fn do_update_firestore(&self) {
        let progress = ProcessingProgress {
            total_scenes: self.total_scenes,
            completed_scenes: self.completed_scenes.load(Ordering::Relaxed),
            total_clips: self.total_clips,
            completed_clips: self.completed_clips.load(Ordering::Relaxed),
            failed_clips: self.failed_clips.load(Ordering::Relaxed),
            current_scene_id: *self.current_scene_id.lock().await,
            current_scene_title: self.current_scene_title.lock().await.clone(),
            started_at: self.started_at,
            updated_at: chrono::Utc::now(),
            error_message: None,
        };

        if let Err(e) = self.video_repo.update_progress(&self.video_id, &progress).await {
            warn!(video_id = %self.video_id, error = %e, "Failed to update progress in Firestore");
        }
    }
}

/// Result of processing a batch of scenes.
#[derive(Debug, Default)]
pub struct SceneProcessingResult {
    /// Number of clips that completed successfully
    pub completed_clips: u32,
    /// Number of clips processed (including skipped)
    pub processed_count: usize,
}

/// Process all selected scenes with parallel style processing.
///
/// Phase 4: Uses raw segment caching to avoid re-extracting segments
/// when rendering multiple styles for the same scene.
///
/// If `progress_tracker` is provided, updates Firestore progress after each scene.
pub async fn process_selected_scenes(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    clips_dir: &PathBuf,
    video_file: &PathBuf,
    clip_tasks: &[ClipTask],
    highlights: &VideoHighlights,
    total_clips: usize,
    progress_tracker: Option<&ProgressTracker>,
) -> WorkerResult<u32> {
    use vclip_media::intelligent::parse_timestamp;

    // Load existing completed clips for skip-on-resume
    let existing_completed = load_existing_clips(ctx, job).await;

    // Group clips by scene_id for parallel processing
    let scene_groups = group_clips_by_scene(clip_tasks);
    let scene_ids = sorted_scene_ids(&scene_groups);

    // Get work_dir from clips_dir parent
    let work_dir = clips_dir.parent().unwrap_or(clips_dir);

    // Process clips with parallel style processing within each scene
    let mut completed_clips = 0u32;
    let mut processed_count = 0usize;

    for scene_id in scene_ids {
        let scene_tasks = scene_groups.get(&scene_id).unwrap();
        let first_task = scene_tasks[0];

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

        // Parse original timestamps for progress display
        let original_start_secs = parse_timestamp(&first_task.start).unwrap_or(0.0);
        let original_end_secs = parse_timestamp(&first_task.end).unwrap_or(30.0);
        let original_duration =
            original_end_secs - original_start_secs + first_task.pad_before + first_task.pad_after;

        // Emit scene_started with ORIGINAL video timestamps
        emit_scene_started(ctx, job, scene_id, first_task, scene_tasks.len(), original_start_secs, original_duration).await;

        // Notify progress tracker about scene start
        if let Some(tracker) = progress_tracker {
            tracker.scene_started(scene_id, Some(first_task.scene_title.clone())).await;
        }

        // Get or create cached raw segment
        let (raw_segment, segment_duration) = prepare_raw_segment(
            ctx,
            job,
            first_task,
            scene_id,
            video_file,
            work_dir,
            scene_tasks,
        )
        .await?;

        // Create modified tasks for raw segment processing
        let modified_tasks = create_modified_tasks(scene_tasks, segment_duration);
        let modified_task_refs: Vec<&ClipTask> = modified_tasks.iter().collect();

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

        // Process scene using the raw segment
        let raw_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);

        let scene_results = clip_pipeline::scene::process_scene_with_raw_key(
            ctx,
            &temp_job,
            clips_dir,
            &raw_segment,
            &modified_task_refs,
            &existing_completed,
            total_clips,
            Some(raw_key),
            true, // Skip scene_started - we already emitted with original timestamps
        )
        .await?;

        processed_count += scene_results.processed;
        completed_clips += scene_results.completed;

        // Calculate failed clips for this scene
        let scene_failed = (scene_tasks.len() as u32).saturating_sub(scene_results.completed);

        // Update Firestore progress tracker after each scene
        if let Some(tracker) = progress_tracker {
            tracker.scene_completed(scene_results.completed, scene_failed).await;
        }

        // Update progress after each scene (25% to 95%)
        let progress = 25 + (processed_count * 70 / total_clips) as u32;
        ctx.progress
            .progress(&job.job_id, progress as u8)
            .await
            .ok();
    }

    Ok(completed_clips)
}

/// Load existing completed clips for skip-on-resume.
async fn load_existing_clips(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
) -> std::collections::HashSet<String> {
    if job.overwrite {
        info!(
            video_id = %job.video_id,
            "Overwrite mode enabled, will re-render existing clips"
        );
        return std::collections::HashSet::new();
    }

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
    }
}

/// Group clips by scene_id.
fn group_clips_by_scene(clip_tasks: &[ClipTask]) -> HashMap<u32, Vec<&ClipTask>> {
    let mut scene_groups: HashMap<u32, Vec<&ClipTask>> = HashMap::new();
    for task in clip_tasks {
        scene_groups.entry(task.scene_id).or_default().push(task);
    }
    scene_groups
}

/// Get sorted scene IDs.
fn sorted_scene_ids(scene_groups: &HashMap<u32, Vec<&ClipTask>>) -> Vec<u32> {
    let mut scene_ids: Vec<u32> = scene_groups.keys().copied().collect();
    scene_ids.sort();
    scene_ids
}

/// Emit scene_started event.
async fn emit_scene_started(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    scene_id: u32,
    first_task: &ClipTask,
    style_count: usize,
    original_start_secs: f64,
    original_duration: f64,
) {
    if let Err(e) = ctx
        .progress
        .scene_started(
            &job.job_id,
            scene_id,
            &first_task.scene_title,
            style_count as u32,
            original_start_secs,
            original_duration,
        )
        .await
    {
        tracing::warn!(
            scene_id = scene_id,
            error = %e,
            "Failed to emit scene_started event"
        );
    }
}

/// Prepare the raw segment for processing.
async fn prepare_raw_segment(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    first_task: &ClipTask,
    scene_id: u32,
    video_file: &PathBuf,
    work_dir: &std::path::Path,
    scene_tasks: &[&ClipTask],
) -> WorkerResult<(PathBuf, f64)> {
    use vclip_media::intelligent::parse_timestamp;

    let pad_before = first_task.pad_before;
    let pad_after = first_task.pad_after;

    let start_secs = parse_timestamp(&first_task.start).unwrap_or(0.0);
    let end_secs = parse_timestamp(&first_task.end).unwrap_or(30.0);
    let padded_start = (start_secs - pad_before).max(0.0);
    let padded_end = end_secs + pad_after;

    let padded_start_ts = format_timestamp(padded_start);
    let padded_end_ts = format_timestamp(padded_end);

    let (raw_segment, raw_created) = ctx
        .raw_cache
        .get_or_create_with_outcome(
            &job.user_id,
            job.video_id.as_str(),
            scene_id,
            video_file,
            &padded_start_ts,
            &padded_end_ts,
            work_dir,
        )
        .await?;

    // Track storage accounting for newly created raw segments
    if raw_created {
        track_raw_segment_storage(ctx, job, &raw_segment).await;
    }

    debug!(
        scene_id = scene_id,
        raw_segment = ?raw_segment,
        raw_created = raw_created,
        "Using raw segment for scene processing"
    );

    // Apply silence removal if requested
    let should_cut_silent = scene_tasks.iter().any(|t| t.cut_silent_parts);
    info!(
        scene_id = scene_id,
        should_cut_silent = should_cut_silent,
        "Checking silence removal flag"
    );
    let raw_segment = if should_cut_silent {
        apply_silence_to_segment(ctx, job, &raw_segment, scene_id).await
    } else {
        raw_segment
    };

    // Calculate segment duration
    let segment_duration = if should_cut_silent {
        match vclip_media::probe::get_duration(&raw_segment).await {
            Ok(d) => d,
            Err(_) => padded_end - padded_start,
        }
    } else {
        padded_end - padded_start
    };

    Ok((raw_segment, segment_duration))
}

/// Track storage accounting for a raw segment.
async fn track_raw_segment_storage(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    raw_segment: &PathBuf,
) {
    if let Ok(metadata) = tokio::fs::metadata(raw_segment).await {
        let file_size = metadata.len();
        let storage_repo =
            vclip_firestore::StorageAccountingRepository::new(ctx.firestore.clone(), &job.user_id);
        if let Err(e) = storage_repo.add_raw_segment(file_size).await {
            warn!(
                user_id = %job.user_id,
                size_bytes = file_size,
                error = %e,
                "Failed to update storage accounting for raw segment (non-critical)"
            );
        }
    }
}

/// Apply silence removal to a segment.
async fn apply_silence_to_segment(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    raw_segment: &PathBuf,
    scene_id: u32,
) -> PathBuf {
    match apply_silence_removal_cached(
        ctx,
        raw_segment,
        scene_id,
        &job.job_id,
        &job.user_id,
        job.video_id.as_str(),
    )
    .await
    {
        Ok(Some(silence_removed_path)) => {
            info!(scene_id = scene_id, "Using silence-removed segment");
            silence_removed_path
        }
        Ok(None) => {
            info!(
                scene_id = scene_id,
                "Silence removal not applied (no significant silence or too short)"
            );
            raw_segment.clone()
        }
        Err(e) => {
            warn!(
                scene_id = scene_id,
                error = %e,
                "Silence removal failed, using original segment"
            );
            raw_segment.clone()
        }
    }
}

/// Create modified tasks for raw segment processing.
fn create_modified_tasks(scene_tasks: &[&ClipTask], segment_duration: f64) -> Vec<ClipTask> {
    scene_tasks
        .iter()
        .map(|task| ClipTask {
            scene_id: task.scene_id,
            scene_title: task.scene_title.clone(),
            scene_description: task.scene_description.clone(),
            // When using raw segment, timestamps are relative to segment start
            start: "00:00:00".to_string(),
            end: format_timestamp(segment_duration),
            style: task.style,
            crop_mode: task.crop_mode.clone(),
            target_aspect: task.target_aspect.clone(),
            priority: task.priority,
            // Padding already applied in raw extraction
            pad_before: 0.0,
            pad_after: 0.0,
            streamer_split_params: task.streamer_split_params.clone(),
            streamer_params: task.streamer_params.clone(),
            cut_silent_parts: task.cut_silent_parts,
        })
        .collect()
}

/// Format seconds as HH:MM:SS.mmm timestamp for FFmpeg.
pub fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}
