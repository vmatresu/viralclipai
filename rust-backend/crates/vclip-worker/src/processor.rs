//! Job processing logic.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::info;

use vclip_firestore::FirestoreClient;
use vclip_media::{create_clip, download_video};
use vclip_models::{ClipMetadata, ClipTask, EncodingConfig, VideoId, VideoMetadata};
use vclip_queue::{ProcessVideoJob, ProgressChannel, ReprocessScenesJob};
use vclip_storage::R2Client;

use crate::config::WorkerConfig;
use crate::error::{WorkerError, WorkerResult};

/// Context for job processing.
pub struct ProcessingContext {
    pub config: WorkerConfig,
    pub storage: R2Client,
    pub firestore: FirestoreClient,
    pub progress: ProgressChannel,
    pub ffmpeg_semaphore: Arc<Semaphore>,
}

impl ProcessingContext {
    pub async fn new(config: WorkerConfig) -> WorkerResult<Self> {
        let storage = R2Client::from_env()
            .await
            .map_err(|e| WorkerError::Storage(e))?;

        let firestore = FirestoreClient::from_env()
            .await
            .map_err(|e| WorkerError::Firestore(e))?;

        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let progress =
            ProgressChannel::new(&redis_url).map_err(|e| WorkerError::Queue(e))?;

        let ffmpeg_semaphore = Arc::new(Semaphore::new(config.max_ffmpeg_processes));

        Ok(Self {
            config,
            storage,
            firestore,
            progress,
            ffmpeg_semaphore,
        })
    }
}

/// Process a new video job.
pub async fn process_video(ctx: &ProcessingContext, job: &ProcessVideoJob) -> WorkerResult<()> {
    info!("Processing video job: {}", job.job_id);

    // Create work directory
    let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
    tokio::fs::create_dir_all(&work_dir).await?;

    // Send initial progress
    ctx.progress
        .log(&job.job_id, "Starting video processing...")
        .await
        .ok();

    // Download video
    ctx.progress
        .log(&job.job_id, "Downloading video...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 5).await.ok();

    let video_file = work_dir.join("source.mp4");
    download_video(&job.video_url, &video_file)
        .await
        .map_err(|e| WorkerError::DownloadFailed(e.to_string()))?;

    ctx.progress.progress(&job.job_id, 15).await.ok();

    // Create video metadata in Firestore
    let video_meta = VideoMetadata::new(
        job.video_id.clone(),
        &job.user_id,
        &job.video_url,
        "Processing...", // Will be updated after AI analysis
    );

    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    video_repo
        .create(&video_meta)
        .await
        .map_err(|e| WorkerError::Firestore(e))?;

    // TODO: Run AI analysis to extract highlights
    // For now, we'll just create a placeholder
    ctx.progress
        .log(&job.job_id, "Analyzing video content...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 30).await.ok();

    // TODO: Generate clip tasks from highlights

    // Cleanup
    if work_dir.exists() {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    info!("Completed video job: {}", job.job_id);
    Ok(())
}

/// Process a reprocess scenes job.
pub async fn reprocess_scenes(
    ctx: &ProcessingContext,
    job: &ReprocessScenesJob,
) -> WorkerResult<()> {
    info!("Processing reprocess job: {}", job.job_id);

    ctx.progress
        .log(&job.job_id, "Loading video data...")
        .await
        .ok();

    // Load existing highlights
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

    // TODO: Download source video from R2 or use existing local copy
    // TODO: Process each scene with each style
    // TODO: Upload clips and update Firestore

    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    info!("Completed reprocess job: {}", job.job_id);
    Ok(())
}

/// Process a single clip task.
pub async fn process_clip_task(
    ctx: &ProcessingContext,
    job_id: &vclip_models::JobId,
    video_id: &VideoId,
    user_id: &str,
    video_file: &Path,
    clips_dir: &Path,
    task: &ClipTask,
    clip_index: usize,
    total_clips: usize,
) -> WorkerResult<ClipMetadata> {
    // Acquire FFmpeg semaphore
    let _permit = ctx.ffmpeg_semaphore.acquire().await.unwrap();

    let filename = task.output_filename();
    let output_path = clips_dir.join(&filename);

    info!(
        "Processing clip {}/{}: {}",
        clip_index + 1,
        total_clips,
        filename
    );

    // Create clip
    let encoding = EncodingConfig::default();
    create_clip(video_file, &output_path, task, &encoding, |_progress| {
        // Could emit granular progress here
    })
    .await
    .map_err(|e| WorkerError::Media(e))?;

    // Get file size
    let file_size = output_path.metadata()?.len();
    let thumb_exists = output_path.with_extension("jpg").exists();

    // Upload clip to R2
    let r2_key = ctx
        .storage
        .upload_clip(&output_path, user_id, video_id.as_str(), &filename)
        .await
        .map_err(|e| WorkerError::Storage(e))?;

    // Upload thumbnail if exists
    let thumb_key = if thumb_exists {
        let thumb_path = output_path.with_extension("jpg");
        let thumb_filename = filename.replace(".mp4", ".jpg");
        Some(
            ctx.storage
                .upload_clip(&thumb_path, user_id, video_id.as_str(), &thumb_filename)
                .await
                .map_err(|e| WorkerError::Storage(e))?,
        )
    } else {
        None
    };

    // Emit clip uploaded progress
    ctx.progress
        .clip_uploaded(job_id, video_id.as_str(), clip_index as u32 + 1, total_clips as u32)
        .await
        .ok();

    // Create clip metadata
    let clip_meta = ClipMetadata {
        clip_id: format!("{}_{}_{}", video_id, task.scene_id, task.style),
        video_id: video_id.clone(),
        user_id: user_id.to_string(),
        scene_id: task.scene_id,
        scene_title: task.scene_title.clone(),
        scene_description: None,
        filename,
        style: task.style.to_string(),
        priority: task.priority,
        start_time: task.start.clone(),
        end_time: task.end.clone(),
        duration_seconds: 0.0, // TODO: calculate
        file_size_bytes: file_size,
        file_size_mb: file_size as f64 / (1024.0 * 1024.0),
        has_thumbnail: thumb_exists,
        r2_key,
        thumbnail_r2_key: thumb_key,
        status: vclip_models::ClipStatus::Completed,
        created_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        updated_at: Some(chrono::Utc::now()),
        created_by: user_id.to_string(),
    };

    Ok(clip_meta)
}
