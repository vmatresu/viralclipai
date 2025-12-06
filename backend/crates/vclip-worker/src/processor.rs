//! Job processing logic.

use vclip_firestore::types::ToFirestoreValue;
use vclip_media::{create_clip, create_intelligent_split_clip, download_video};
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
    ctx.progress.progress(&job.job_id, 5).await.ok();

    // Get transcript first (fast)
    ctx.progress
        .log(&job.job_id, "Fetching video transcript...")
        .await
        .ok();

    let gemini_client = crate::gemini::GeminiClient::new()?;
    let base_prompt = job.custom_prompt.as_deref()
        .map(|s| s.to_string())
        .or_else(|| load_prompt_from_file())
        .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

    // Get real video metadata (title and canonical URL) from yt-dlp
    let (real_video_title, canonical_video_url) = gemini_client.get_video_metadata(&job.video_url).await
        .map_err(|e| WorkerError::ai_failed(format!("Failed to get video metadata: {}", e)))?;

    // Get transcript using yt-dlp (fast, no video download needed)
    let transcript = gemini_client.get_transcript_only(&job.video_url, &work_dir).await
        .map_err(|e| WorkerError::ai_failed(format!("Failed to get transcript: {}", e)))?;

    ctx.progress.progress(&job.job_id, 10).await.ok();

    // Now run video download and AI analysis in parallel
    ctx.progress
        .log(&job.job_id, "Downloading video and analyzing with AI...")
        .await
        .ok();

    let video_file = work_dir.join("source.mp4");

    // Start video download and AI analysis in parallel using tokio::join!
    let video_url = job.video_url.clone();
    let video_file_clone = video_file.clone();
    
    let (download_result, analysis_result) = tokio::join!(
        download_video(&video_url, &video_file_clone),
        gemini_client.analyze_transcript(&base_prompt, &job.video_url, &transcript)
    );

    // Check results
    if let Err(e) = download_result {
        return Err(WorkerError::DownloadFailed(e.to_string()));
    }
    let ai_response = analysis_result?;

    ctx.progress.progress(&job.job_id, 35).await.ok();

    // Create video metadata in Firestore (or update if already exists from retry)
    let video_meta = VideoMetadata::new(
        job.video_id.clone(),
        &job.user_id,
        &canonical_video_url, // Use canonical URL from yt-dlp
        &real_video_title,     // Use real title from yt-dlp
    );

    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    
    // Check if video already exists (e.g., from a retry)
    match video_repo.get(&job.video_id).await {
        Ok(Some(mut existing_video)) => {
            // Video exists, update title and status
            existing_video.video_title = real_video_title.clone();
            existing_video.video_url = canonical_video_url.clone();
            existing_video.status = vclip_models::VideoStatus::Processing;
            existing_video.updated_at = chrono::Utc::now();
            
            // Update in Firestore
            let mut fields = std::collections::HashMap::new();
            fields.insert("video_title".to_string(), real_video_title.clone().to_firestore_value());
            fields.insert("video_url".to_string(), canonical_video_url.clone().to_firestore_value());
            fields.insert("status".to_string(), vclip_models::VideoStatus::Processing.as_str().to_firestore_value());
            fields.insert("updated_at".to_string(), chrono::Utc::now().to_firestore_value());
            
            ctx.firestore
                .update_document(
                    &format!("users/{}/videos", job.user_id),
                    job.video_id.as_str(),
                    fields,
                    Some(vec!["video_title".to_string(), "video_url".to_string(), "status".to_string(), "updated_at".to_string()]),
                )
                .await
                .ok(); // Ignore errors for now
        }
        Ok(None) => {
            // Create new video record
            video_repo
                .create(&video_meta)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
        Err(e) => {
            // Log error but try to continue
            tracing::warn!("Failed to check video existence: {}", e);
            // Try to create anyway
            video_repo.create(&video_meta).await.ok();
        }
    }

    // Convert AI response to storage format
    let highlights: Vec<vclip_storage::operations::HighlightEntry> = ai_response
        .highlights
        .into_iter()
        .map(|h| vclip_storage::operations::HighlightEntry {
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

    let highlights_data = vclip_storage::HighlightsData {
        highlights,
        video_url: Some(canonical_video_url.clone()), // Use canonical URL from yt-dlp
        video_title: Some(real_video_title.clone()),   // Use real title from yt-dlp
        custom_prompt: job.custom_prompt.clone(),
    };

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

    ctx.progress.progress(&job.job_id, 40).await.ok();

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
        // Log clip name to progress channel for UI
        ctx.progress
            .log(&job.job_id, format!("Processing clip {}/{}: {}", idx + 1, total_clips, task.output_filename()))
            .await
            .ok();

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
                // Update progress (40% to 95%)
                let progress = 40 + ((idx + 1) * 55 / total_clips) as u32;
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
    match video_repo.complete(&job.video_id, completed_clips).await {
        Ok(_) => {
            info!("Successfully marked video {} as completed with {} clips", job.video_id, completed_clips);
        }
        Err(e) => {
            tracing::error!("Failed to mark video {} as completed: {}", job.video_id, e);
            return Err(WorkerError::Firestore(e));
        }
    }

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
    ctx.progress.progress(&job.job_id, 5).await.ok();

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

    // Create work directory
    let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
    tokio::fs::create_dir_all(&work_dir).await?;

    // Download source video from R2
    ctx.progress
        .log(&job.job_id, "Downloading source video...")
        .await
        .ok();
    ctx.progress.progress(&job.job_id, 15).await.ok();

    let video_file = work_dir.join("source.mp4");
    
    // Try to get source video from R2 first
    let source_key = format!("{}/{}/source.mp4", job.user_id, job.video_id.as_str());
    let video_downloaded = match ctx.storage.download_file(&source_key, &video_file).await {
        Ok(_) => {
            info!("Downloaded source video from R2: {}", source_key);
            true
        }
        Err(r2_error) => {
            info!("Source video not found in R2 ({}), trying original URL from highlights", r2_error);
            
            // Fall back to original video URL from highlights data
            if let Some(ref video_url) = highlights.video_url {
                ctx.progress
                    .log(&job.job_id, "Downloading original video from source URL...")
                    .await
                    .ok();
                
                match download_video(video_url, &video_file).await {
                    Ok(_) => {
                        info!("Downloaded source video from original URL: {}", video_url);
                        true
                    }
                    Err(url_error) => {
                        let err_msg = format!("Source video not found in R2: {}. Failed to download from original URL {}: {}", 
                            r2_error, video_url, url_error);
                        ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                        return Err(WorkerError::job_failed(&err_msg));
                    }
                }
            } else {
                let err_msg = format!("Source video not found in R2: {}. No original video URL available in highlights data.", r2_error);
                ctx.progress.error(&job.job_id, err_msg.clone()).await.ok();
                return Err(WorkerError::job_failed(&err_msg));
            }
        }
    };

    if !video_downloaded {
        return Err(WorkerError::job_failed("Failed to download source video"));
    }

    ctx.progress.progress(&job.job_id, 25).await.ok();

    // Generate clip tasks from selected highlights
    let clip_tasks = generate_clip_tasks_from_highlights(
        &selected_highlights,
        &job.styles,
        &job.crop_mode,
        &job.target_aspect,
    );

    ctx.progress
        .log(
            &job.job_id,
            format!("Generating {} clips...", total_clips),
        )
        .await
        .ok();

    // Create clips directory
    let clips_dir = work_dir.join("clips");
    tokio::fs::create_dir_all(&clips_dir).await?;

    // Process clips
    let mut completed_clips = 0u32;
    for (idx, task) in clip_tasks.iter().enumerate() {
        // Log clip name to progress channel for UI
        ctx.progress
            .log(&job.job_id, format!("Processing clip {}/{}: {}", idx + 1, total_clips, task.output_filename()))
            .await
            .ok();

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
                // Update progress (25% to 95%)
                let progress = 25 + ((idx + 1) * 70 / total_clips) as u32;
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

    // Update video metadata
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    
    // Get current clip count and add new clips
    match video_repo.get(&job.video_id).await {
        Ok(Some(video)) => {
            let new_total = video.clips_count + completed_clips;
            video_repo
                .complete(&job.video_id, new_total)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
        Ok(None) => {
            // Video not found, just mark as complete with new count
            video_repo
                .complete(&job.video_id, completed_clips)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
        Err(e) => {
            warn!("Failed to get video metadata: {}", e);
            // Still try to complete
            video_repo
                .complete(&job.video_id, completed_clips)
                .await
                .map_err(|e| WorkerError::Firestore(e))?;
        }
    }

    // Cleanup work directory
    if work_dir.exists() {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    ctx.progress.progress(&job.job_id, 100).await.ok();
    ctx.progress
        .done(&job.job_id, job.video_id.as_str())
        .await
        .ok();

    info!("Completed reprocess job: {} ({}/{} clips)", job.job_id, completed_clips, total_clips);
    Ok(())
}

/// Process a single clip task.
///
/// # Style Routing Logic
///
/// This function implements the style routing pattern from the Python implementation:
///
/// ## Traditional Styles (Split, LeftFocus, RightFocus, Original)
/// - Processed via `create_clip()`
/// - Single-pass FFmpeg with video filters
/// - Fast and efficient
///
/// ## IntelligentSplit Style
/// - Processed via `create_intelligent_split_clip()`
/// - Multi-step pipeline:
///   1. Extract left and right halves from source
///   2. Apply intelligent crop to each half (currently placeholder scaling)
///   3. Stack halves vertically (left on top, right on bottom)
/// - Future: Will integrate ML-based face tracking
///
/// This matches Python's `run_ffmpeg_clip_with_crop()` logic (lines 729-772 in clipper.py).
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

    // Create clip - route to appropriate function based on style
    let encoding = EncodingConfig::default();
    
    // IntelligentSplit requires special processing (extract halves, crop each, stack)
    if task.style == vclip_models::Style::IntelligentSplit {
        create_intelligent_split_clip(video_file, &output_path, task, &encoding, |_progress| {
            // Could emit granular progress here
        })
        .await
        .map_err(|e| WorkerError::Media(e))?;
    } else {
        // All other styles use standard clip creation
        create_clip(video_file, &output_path, task, &encoding, |_progress| {
            // Could emit granular progress here
        })
        .await
        .map_err(|e| WorkerError::Media(e))?;
    }

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

/// Generate clip tasks from a filtered list of highlights (for reprocessing).
fn generate_clip_tasks_from_highlights(
    highlights: &[&vclip_storage::operations::HighlightEntry],
    styles: &[vclip_models::Style],
    crop_mode: &vclip_models::CropMode,
    target_aspect: &vclip_models::AspectRatio,
) -> Vec<ClipTask> {
    let mut tasks = Vec::new();

    for highlight in highlights {
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
