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

    // Run AI analysis to extract highlights
    ctx.progress
        .log(&job.job_id, "Analyzing video content with AI...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 20).await.ok();

    let highlights_data = match analyze_video_highlights(
        ctx,
        &job.job_id,
        &video_file,
        &job.video_url,
        job.custom_prompt.as_deref(),
    )
    .await
    {
        Ok(data) => data,
        Err(e) => {
            video_repo.fail(&job.video_id, &e.to_string()).await.ok();
            return Err(e);
        }
    };

    ctx.progress.progress(&job.job_id, 40).await.ok();

    // Validate highlights exist
    if highlights_data.highlights.is_empty() {
        let err_msg = "No highlights detected in video";
        video_repo.fail(&job.video_id, err_msg).await.ok();
        return Err(WorkerError::job_failed(err_msg));
    }

    // Upload highlights.json to R2
    ctx.progress
        .log(
            &job.job_id,
            format!("Found {} highlights, uploading metadata...", highlights_data.highlights.len()),
        )
        .await
        .ok();

    ctx.storage
        .upload_highlights(&job.user_id, job.video_id.as_str(), &highlights_data)
        .await
        .map_err(|e| WorkerError::Storage(e))?;

    ctx.progress.progress(&job.job_id, 45).await.ok();

    // Generate clip tasks from highlights
    let clip_tasks = generate_clip_tasks(&highlights_data, &job.styles, &job.crop_mode, &job.target_aspect);
    let total_clips = clip_tasks.len();

    ctx.progress
        .log(
            &job.job_id,
            format!("Generating {} clips from {} highlights...", total_clips, highlights_data.highlights.len()),
        )
        .await
        .ok();

    // Create clips directory
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Process clips
    let mut completed_clips = 0u32;
    for (idx, task) in clip_tasks.iter().enumerate() {
        match process_clip_task(
            ctx,
            &job.job_id,
            &job.video_id,
            &job.user_id,
            &video_file,
            &clips_dir,
            task,
            idx,
            total_clips,
        )
        .await
        {
            Ok(_) => {
                completed_clips += 1;
                // Update progress (45% to 95%)
                let progress = 45 + ((idx + 1) * 50 / total_clips) as u32;
                ctx.progress.progress(&job.job_id, progress as u8).await.ok();
            }
            Err(e) => {
                ctx.progress
                    .log(&job.job_id, format!("Failed to process clip {}: {}", task.output_filename(), e))
                    .await
                    .ok();
            }
        }
    }

    // Mark video as completed
    video_repo
        .complete(&job.video_id, completed_clips)
        .await
        .map_err(|e| WorkerError::Firestore(e))?;

    // Cleanup work directory
    if work_dir.exists() {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    ctx.progress.progress(&job.job_id, 100).await.ok();
    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    info!("Completed video job: {} ({}/{} clips)", job.job_id, completed_clips, total_clips);
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

/// Analyze video to extract highlights using AI.
async fn analyze_video_highlights(
    ctx: &ProcessingContext,
    job_id: &vclip_models::JobId,
    video_file: &Path,
    video_url: &str,
    custom_prompt: Option<&str>,
) -> WorkerResult<vclip_storage::HighlightsData> {
    use crate::gemini::GeminiClient;
    use vclip_storage::operations::HighlightEntry;

    ctx.progress
        .log(job_id, "Running AI analysis with Gemini...")
        .await
        .ok();

    // Get base prompt
    let base_prompt = custom_prompt
        .map(|s| s.to_string())
        .or_else(|| load_prompt_from_file())
        .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

    // Create Gemini client
    let client = GeminiClient::new()?;

    // Get work directory for transcript
    let work_dir = video_file.parent().ok_or_else(|| {
        WorkerError::processing_failed("Failed to get work directory")
    })?;

    // Call Gemini API
    let ai_response = client
        .get_highlights(&base_prompt, video_url, work_dir)
        .await?;

    // Convert to storage format
    let highlights: Vec<HighlightEntry> = ai_response
        .highlights
        .into_iter()
        .map(|h| HighlightEntry {
            id: h.id,
            title: h.title,
            description: h.description,
            start: h.start,
            end: h.end,
            duration: h.duration,
            hook_category: h.hook_category,
            reason: h.reason,
        })
        .collect();

    Ok(vclip_storage::HighlightsData {
        highlights,
        video_url: ai_response.video_url.or_else(|| Some(video_url.to_string())),
        video_title: ai_response.video_title.or_else(|| Some(extract_video_title(video_url))),
        custom_prompt: custom_prompt.map(|s| s.to_string()),
    })
}

/// Load prompt from file.
fn load_prompt_from_file() -> Option<String> {
    std::fs::read_to_string("prompt.txt").ok()
}

/// Default prompt for highlight extraction.
const DEFAULT_PROMPT: &str = r#"**Role:**
You are an elite short-form video editor for a "manosphere" commentary channel. The video format is a split-screen: a viral clip (usually a woman) on the Left, and a male commentator on the Right.

**Your Goal:**
Extract a batch of **3 to 10 viral segments** that prioritize Interaction over simple monologues.

**Segment Structure (The "Call & Response" Formula):**
1. **The Setup (Left Side):** Start exactly when the person makes a controversial claim, states a statistic, or complains about men.
2. **The Pivot:** The moment the host pauses the video or speaks up.
3. **The Slam (Right Side):** The host's immediate counter-argument, insult, or reality check.
4. **The End:** Cut after the punchline.

**Constraints:**
* **Quantity:** Extract at least 3 distinct segments.
* **Duration:** Each individual segment must be **20 to 90 seconds** long.
* **Narrative:** [Setup] -> [Reaction] -> [Punchline].
* **Audio:** Ensure the cut timestamp doesn't sever a word.
"#;

/// Generate clip tasks from highlights.
fn generate_clip_tasks(
    highlights: &vclip_storage::HighlightsData,
    styles: &[vclip_models::Style],
    crop_mode: &vclip_models::CropMode,
    target_aspect: &vclip_models::AspectRatio,
) -> Vec<ClipTask> {
    let mut tasks = Vec::new();

    for highlight in &highlights.highlights {
        for style in styles {
            tasks.push(ClipTask {
                scene_id: highlight.id,
                scene_title: highlight.title.clone(),
                start: highlight.start.clone(),
                end: highlight.end.clone(),
                style: style.clone(),
                crop_mode: crop_mode.clone(),
                target_aspect: target_aspect.clone(),
                priority: highlight.id,
                pad_before: 0.0,
                pad_after: 0.0,
            });
        }
    }

    tasks
}


/// Extract video title from URL.
fn extract_video_title(url: &str) -> String {
    // Try to extract YouTube video ID
    if url.contains("youtube.com") || url.contains("youtu.be") {
        if let Some(id) = extract_youtube_id(url) {
            return format!("YouTube Video {}", id);
        }
    }
    "Video".to_string()
}

/// Extract YouTube video ID from URL.
fn extract_youtube_id(url: &str) -> Option<String> {
    if let Some(v_pos) = url.find("v=") {
        let id_start = v_pos + 2;
        let id = url[id_start..]
            .split('&')
            .next()
            .unwrap_or("")
            .to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}
