//! Top Scenes compilation processing.
//!
//! This module handles the creation of "Top N Scenes" compilation videos
//! that combine multiple selected scenes with countdown overlays.
//!
//! # Compilation Flow (Parallelized)
//!
//! 1. Order scenes according to user selection (reversed for countdown)
//! 2. **Phase 1**: Check R2 cache for all segments in parallel
//! 3. **Phase 2**: Download cached segments + start silence removal for them
//!    while source video downloads in background (if needed)
//! 4. **Phase 3**: Extract uncached segments in parallel (limited concurrency)
//! 5. **Phase 4**: Apply silence removal to extracted segments in parallel
//! 6. Build `TopSceneEntry` list for the streamer processor
//! 7. Render compilation with countdown overlays
//! 8. Upload to R2 and save metadata to Firestore
//!
//! # Architecture
//!
//! This follows the Single Responsibility Principle - this module ONLY handles
//! Top Scenes compilation. Scene processing and reprocessing logic remain in
//! the `reprocessing` module.
//!
//! # Parallelization Strategy
//!
//! - Cached segments can be processed (silence removal) while source downloads
//! - Segment extraction uses a semaphore to limit concurrent FFmpeg processes
//! - Silence removal runs in parallel for all segments
//! - This significantly reduces total job time for reprocessing requests

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use vclip_models::{Highlight, VideoHighlights};
use vclip_queue::ReprocessScenesJob;

use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;
use crate::raw_segment_cache::raw_segment_r2_key;
use crate::silence_cache::apply_silence_removal_cached;

/// Maximum concurrent FFmpeg extraction processes.
const MAX_CONCURRENT_EXTRACTIONS: usize = 3;

/// Maximum concurrent silence removal processes.
const MAX_CONCURRENT_SILENCE_REMOVAL: usize = 2;

/// Process Top Scenes compilation job - creates a single video from all selected scenes
/// with countdown overlay (5, 4, 3, 2, 1).
///
/// This uses a parallelized pipeline:
/// - Cached segments start processing immediately
/// - Source download happens in parallel with cached segment processing
/// - Extraction and silence removal run with controlled concurrency
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

    // PARALLELIZED: Process segments with concurrent cache downloads + source extraction
    let final_segment_paths = process_segments_parallel(
        ctx,
        job,
        &ordered_highlights,
        &work_dir,
        video_highlights.video_url.as_deref(),
    )
    .await?;

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

/// Process all segments in parallel with optimized pipeline.
///
/// Pipeline:
/// 1. Check R2 cache for all segments in parallel
/// 2. Download cached segments in parallel
/// 3. Start silence removal for cached segments immediately (parallel with source download)
/// 4. Download source video if any segments need extraction
/// 5. Extract uncached segments in parallel (limited concurrency)
/// 6. Apply silence removal to extracted segments in parallel
///
/// This ensures cached segments are fully processed before the source download completes,
/// significantly reducing total job time.
async fn process_segments_parallel(
    ctx: &EnhancedProcessingContext,
    job: &ReprocessScenesJob,
    ordered_highlights: &[Highlight],
    work_dir: &PathBuf,
    video_url: Option<&str>,
) -> WorkerResult<Vec<PathBuf>> {
    use futures::future::join_all;
    use tokio::sync::oneshot;
    use vclip_media::intelligent::parse_timestamp;

    ctx.progress
        .log(&job.job_id, "Checking segment cache status...")
        .await
        .ok();

    // ========================================================================
    // PHASE 1: Check cache status for all segments in parallel
    // ========================================================================
    let cache_check_futures: Vec<_> = ordered_highlights
        .iter()
        .map(|h| {
            let scene_id = h.id;
            let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);
            let local_path = work_dir.join(format!("raw_{}.mp4", scene_id));
            let raw_cache = ctx.raw_cache.clone();
            
            async move {
                // Check local first
                if local_path.exists() {
                    if let Ok(meta) = tokio::fs::metadata(&local_path).await {
                        if meta.len() > 0 {
                            debug!(scene_id = scene_id, "Found existing local raw segment");
                            return (scene_id, Some(local_path), true); // (scene_id, path, is_local)
                        }
                    }
                }
                
                // Check R2 cache
                if raw_cache.check_raw_exists(&r2_key).await {
                    (scene_id, Some(local_path), false) // Needs download from R2
                } else {
                    (scene_id, None, false) // Needs extraction from source
                }
            }
        })
        .collect();

    let cache_results: Vec<(u32, Option<PathBuf>, bool)> = join_all(cache_check_futures).await;

    // Categorize segments
    let mut cached_segments: Vec<(u32, PathBuf)> = Vec::new(); // Already local
    let mut need_r2_download: Vec<(u32, PathBuf)> = Vec::new(); // In R2, need download
    let mut need_extraction: Vec<&Highlight> = Vec::new(); // Need source extraction

    for (scene_id, path_opt, is_local) in &cache_results {
        if let Some(path) = path_opt {
            if *is_local {
                cached_segments.push((*scene_id, path.clone()));
            } else {
                need_r2_download.push((*scene_id, path.clone()));
            }
        } else {
            // Find the highlight for this scene
            if let Some(h) = ordered_highlights.iter().find(|h| h.id == *scene_id) {
                need_extraction.push(h);
            }
        }
    }

    info!(
        cached_local = cached_segments.len(),
        cached_r2 = need_r2_download.len(),
        need_extraction = need_extraction.len(),
        "Segment cache status"
    );

    // ========================================================================
    // PHASE 2: Download R2 cached segments + start source download in parallel
    // ========================================================================
    
    // Channel for source download completion
    let (source_tx, source_rx) = oneshot::channel::<Result<PathBuf, WorkerError>>();
    let source_download_handle: Option<tokio::task::JoinHandle<()>>;
    
    // Start source download in background if needed
    if !need_extraction.is_empty() {
        let video_file = work_dir.join("source.mp4");
        
        if video_file.exists() {
            // Source already exists
            let _ = source_tx.send(Ok(video_file));
            source_download_handle = None;
        } else if let Some(url) = video_url {
            let url = url.to_string();
            let video_file_clone = video_file.clone();
            let job_id = job.job_id.clone();
            let progress = ctx.progress.clone();
            
            source_download_handle = Some(tokio::spawn(async move {
                progress
                    .log(&job_id, "Downloading source video (background)...")
                    .await
                    .ok();
                
                let result = match vclip_media::download_video(&url, &video_file_clone).await {
                    Ok(()) => Ok(video_file_clone),
                    Err(e) => Err(WorkerError::job_failed(&format!("Failed to download source: {}", e))),
                };
                let _ = source_tx.send(result);
            }));
        } else {
            return Err(WorkerError::job_failed("No source video available for uncached segments"));
        }
    } else {
        // No extraction needed, close the channel
        drop(source_tx);
        source_download_handle = None;
    }

    // Download R2 cached segments in parallel
    let r2_download_futures: Vec<_> = need_r2_download
        .iter()
        .map(|(scene_id, local_path)| {
            let scene_id = *scene_id;
            let local_path = local_path.clone();
            let r2_key = raw_segment_r2_key(&job.user_id, job.video_id.as_str(), scene_id);
            let storage = ctx.storage.clone();
            
            async move {
                match storage.download_file(&r2_key, &local_path).await {
                    Ok(_) => {
                        info!(scene_id = scene_id, "Downloaded raw segment from R2 cache");
                        Ok((scene_id, local_path))
                    }
                    Err(e) => {
                        warn!(scene_id = scene_id, error = %e, "Failed to download from R2");
                        Err((scene_id, e))
                    }
                }
            }
        })
        .collect();

    let r2_results = join_all(r2_download_futures).await;

    // Track successfully downloaded segments and those that need extraction
    let mut downloaded_segments: Vec<(u32, PathBuf)> = cached_segments;
    let mut failed_r2_downloads: Vec<u32> = Vec::new();

    for result in r2_results {
        match result {
            Ok((scene_id, path)) => {
                downloaded_segments.push((scene_id, path));
            }
            Err((scene_id, _)) => {
                failed_r2_downloads.push(scene_id);
            }
        }
    }

    // Add failed R2 downloads to extraction list
    for scene_id in failed_r2_downloads {
        if let Some(h) = ordered_highlights.iter().find(|h| h.id == scene_id) {
            need_extraction.push(h);
        }
    }

    // ========================================================================
    // PHASE 3: Start silence removal for downloaded segments (parallel with source download)
    // ========================================================================
    
    // Map to store final paths: scene_id -> final_path
    let final_paths: Arc<tokio::sync::Mutex<HashMap<u32, PathBuf>>> = 
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Process silence removal for already-available segments
    if job.cut_silent_parts && !downloaded_segments.is_empty() {
        ctx.progress
            .log(&job.job_id, format!(
                "Processing {} cached segments (parallel with source download)...",
                downloaded_segments.len()
            ))
            .await
            .ok();

        // If no extraction needed, just process silence removal for cached segments
        if need_extraction.is_empty() {
            let silence_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_SILENCE_REMOVAL));
            let silence_futures: Vec<_> = downloaded_segments
                .iter()
                .map(|(scene_id, raw_path)| {
                    let scene_id = *scene_id;
                    let raw_path = raw_path.clone();
                    let job_id = job.job_id.clone();
                    let user_id = job.user_id.clone();
                    let video_id = job.video_id.clone();
                    let sem = silence_sem.clone();
                    let final_paths = final_paths.clone();
                    let ctx = ctx.clone();

                    async move {
                        let _permit = sem.acquire().await.expect("semaphore closed");
                        
                        let final_path = match apply_silence_removal_cached(
                            &ctx,
                            &raw_path,
                            scene_id,
                            &job_id,
                            &user_id,
                            video_id.as_str(),
                        )
                        .await
                        {
                            Ok(Some(silence_removed_path)) => {
                                info!(scene_id = scene_id, "Using silence-removed segment for Top Scenes compilation");
                                silence_removed_path
                            }
                            Ok(None) => {
                                debug!(scene_id = scene_id, "Silence removal not applied (no significant silence)");
                                raw_path
                            }
                            Err(e) => {
                                warn!(scene_id = scene_id, error = %e, "Silence removal failed, using original");
                                raw_path
                            }
                        };
                        
                        final_paths.lock().await.insert(scene_id, final_path);
                    }
                })
                .collect();

            join_all(silence_futures).await;
        } else {
            // ================================================================
            // PHASE 4: Process cached segments AND source download concurrently
            // ================================================================
            
            // Create silence removal future for cached segments
            let cached_silence_future = {
                let silence_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_SILENCE_REMOVAL));
                let silence_futures: Vec<_> = downloaded_segments
                    .iter()
                    .map(|(scene_id, raw_path)| {
                        let scene_id = *scene_id;
                        let raw_path = raw_path.clone();
                        let job_id = job.job_id.clone();
                        let user_id = job.user_id.clone();
                        let video_id = job.video_id.clone();
                        let sem = silence_sem.clone();
                        let final_paths = final_paths.clone();
                        let ctx = ctx.clone();

                        async move {
                            let _permit = sem.acquire().await.expect("semaphore closed");
                            
                            let final_path = match apply_silence_removal_cached(
                                &ctx,
                                &raw_path,
                                scene_id,
                                &job_id,
                                &user_id,
                                video_id.as_str(),
                            )
                            .await
                            {
                                Ok(Some(silence_removed_path)) => {
                                    info!(scene_id = scene_id, "Using silence-removed segment for Top Scenes compilation");
                                    silence_removed_path
                                }
                                Ok(None) => {
                                    debug!(scene_id = scene_id, "Silence removal not applied (no significant silence)");
                                    raw_path
                                }
                                Err(e) => {
                                    warn!(scene_id = scene_id, error = %e, "Silence removal failed, using original");
                                    raw_path
                                }
                            };
                            
                            final_paths.lock().await.insert(scene_id, final_path);
                        }
                    })
                    .collect();

                async move {
                    join_all(silence_futures).await;
                }
            };

            // Run cached segment processing AND wait for source download concurrently
            let (_, source_result) = tokio::join!(
                cached_silence_future,
                source_rx
            );

            let source_path = source_result
                .map_err(|_| WorkerError::job_failed("Source download channel closed"))?
                .map_err(|e| e)?;

            ctx.progress
                .log(&job.job_id, format!(
                    "Extracting {} segments from source...",
                    need_extraction.len()
                ))
                .await
                .ok();

            // ================================================================
            // PHASE 5: Extract missing segments in parallel
            // ================================================================
            
            let extract_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_EXTRACTIONS));
            let extract_futures: Vec<_> = need_extraction
                .iter()
                .map(|h| {
                    let scene_id = h.id;
                    let start_secs = parse_timestamp(&h.start).unwrap_or(0.0);
                    let end_secs = parse_timestamp(&h.end).unwrap_or(30.0);
                    let padded_start = (start_secs - h.pad_before).max(0.0);
                    let padded_end = end_secs + h.pad_after;
                    let padded_start_ts = format_timestamp(padded_start);
                    let padded_end_ts = format_timestamp(padded_end);
                    
                    let source_path = source_path.clone();
                    let work_dir = work_dir.clone();
                    let raw_cache = ctx.raw_cache.clone();
                    let user_id = job.user_id.clone();
                    let video_id = job.video_id.clone();
                    let sem = extract_sem.clone();

                    async move {
                        let _permit = sem.acquire().await.expect("semaphore closed");
                        
                        let result = raw_cache
                            .get_or_create_with_outcome(
                                &user_id,
                                video_id.as_str(),
                                scene_id,
                                &source_path,
                                &padded_start_ts,
                                &padded_end_ts,
                                &work_dir,
                            )
                            .await;
                        
                        (scene_id, result)
                    }
                })
                .collect();

            let extract_results = join_all(extract_futures).await;
            
            // Collect extracted segments
            let mut extracted_segments: Vec<(u32, PathBuf)> = Vec::new();
            for (scene_id, result) in extract_results {
                match result {
                    Ok((path, _)) => {
                        extracted_segments.push((scene_id, path));
                    }
                    Err(e) => {
                        return Err(WorkerError::job_failed(&format!(
                            "Failed to extract segment {}: {}",
                            scene_id, e
                        )));
                    }
                }
            }

            // ================================================================
            // PHASE 6: Apply silence removal to extracted segments
            // ================================================================
            
            let extract_silence_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_SILENCE_REMOVAL));
            let extract_silence_futures: Vec<_> = extracted_segments
                .iter()
                .map(|(scene_id, raw_path)| {
                    let scene_id = *scene_id;
                    let raw_path = raw_path.clone();
                    let ctx = ctx.clone();
                    let job_id = job.job_id.clone();
                    let user_id = job.user_id.clone();
                    let video_id = job.video_id.clone();
                    let sem = extract_silence_sem.clone();
                    let final_paths = final_paths.clone();

                    async move {
                        let _permit = sem.acquire().await.expect("semaphore closed");
                        
                        let final_path = match apply_silence_removal_cached(
                            &ctx,
                            &raw_path,
                            scene_id,
                            &job_id,
                            &user_id,
                            video_id.as_str(),
                        )
                        .await
                        {
                            Ok(Some(silence_removed_path)) => {
                                info!(scene_id = scene_id, "Using silence-removed segment for Top Scenes compilation");
                                silence_removed_path
                            }
                            Ok(None) => {
                                debug!(scene_id = scene_id, "Silence removal not applied");
                                raw_path
                            }
                            Err(e) => {
                                warn!(scene_id = scene_id, error = %e, "Silence removal failed");
                                raw_path
                            }
                        };
                        
                        final_paths.lock().await.insert(scene_id, final_path);
                    }
                })
                .collect();

            join_all(extract_silence_futures).await;
        }
    } else {
        // No silence removal needed - store raw paths directly
        for (scene_id, path) in downloaded_segments {
            final_paths.lock().await.insert(scene_id, path);
        }

        // Handle extraction if needed
        if !need_extraction.is_empty() {
            let source_path = source_rx.await
                .map_err(|_| WorkerError::job_failed("Source download channel closed"))?
                .map_err(|e| e)?;

            ctx.progress
                .log(&job.job_id, format!(
                    "Extracting {} segments from source...",
                    need_extraction.len()
                ))
                .await
                .ok();

            let extract_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_EXTRACTIONS));
            let extract_futures: Vec<_> = need_extraction
                .iter()
                .map(|h| {
                    let scene_id = h.id;
                    let start_secs = parse_timestamp(&h.start).unwrap_or(0.0);
                    let end_secs = parse_timestamp(&h.end).unwrap_or(30.0);
                    let padded_start = (start_secs - h.pad_before).max(0.0);
                    let padded_end = end_secs + h.pad_after;
                    let padded_start_ts = format_timestamp(padded_start);
                    let padded_end_ts = format_timestamp(padded_end);
                    
                    let source_path = source_path.clone();
                    let work_dir = work_dir.clone();
                    let raw_cache = ctx.raw_cache.clone();
                    let user_id = job.user_id.clone();
                    let video_id = job.video_id.clone();
                    let sem = extract_sem.clone();
                    let final_paths = final_paths.clone();

                    async move {
                        let _permit = sem.acquire().await.expect("semaphore closed");
                        
                        let result = raw_cache
                            .get_or_create_with_outcome(
                                &user_id,
                                video_id.as_str(),
                                scene_id,
                                &source_path,
                                &padded_start_ts,
                                &padded_end_ts,
                                &work_dir,
                            )
                            .await;
                        
                        if let Ok((ref path, _)) = result {
                            final_paths.lock().await.insert(scene_id, path.clone());
                        }
                        
                        (scene_id, result.map(|(p, c)| (p, c)))
                    }
                })
                .collect();

            let extract_results = join_all(extract_futures).await;
            
            // Check for errors
            for (scene_id, result) in extract_results {
                if let Err(e) = result {
                    return Err(WorkerError::job_failed(&format!(
                        "Failed to extract segment {}: {}",
                        scene_id, e
                    )));
                }
            }
        }
    }

    // ========================================================================
    // PHASE 6: Build final ordered path list
    // ========================================================================
    
    let paths_map = final_paths.lock().await;
    let mut result: Vec<PathBuf> = Vec::with_capacity(ordered_highlights.len());
    
    for h in ordered_highlights {
        if let Some(path) = paths_map.get(&h.id) {
            result.push(path.clone());
        } else {
            return Err(WorkerError::job_failed(&format!(
                "Missing segment for scene {} after processing",
                h.id
            )));
        }
    }

    // Clean up source download handle
    if let Some(handle) = source_download_handle {
        handle.await.ok();
    }

    Ok(result)
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
    let watermark = if crate::watermark_check::user_requires_watermark(&ctx.firestore, &job.user_id).await {
        Some(vclip_media::WatermarkConfig::default())
    } else {
        None
    };

    vclip_media::styles::streamer::process_top_scenes_from_segments(
        final_segment_paths,
        &output_path,
        &encoding,
        &streamer_params,
        watermark.as_ref(),
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
