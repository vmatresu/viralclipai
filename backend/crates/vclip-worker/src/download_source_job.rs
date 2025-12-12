//! Download source video job processing.
//!
//! Handles background download of source videos from original URLs to R2 storage.
//! This enables faster reprocessing by caching the source video.

use chrono::{Duration as ChronoDuration, Utc};
use redis::Script;
use tracing::{debug, info, warn};

use vclip_media::download_video;
use vclip_queue::DownloadSourceJob;

use crate::error::{WorkerError, WorkerResult};
use crate::logging::JobLogger;
use crate::processor::EnhancedProcessingContext;

/// TTL for source videos in R2 (24 hours).
const SOURCE_VIDEO_TTL_HOURS: i64 = 24;

/// Lock TTL for single-flight download (1 hour).
const DOWNLOAD_LOCK_TTL_SECS: u64 = 3600;

/// Redis key for download lock.
fn download_lock_key(user_id: &str, video_id: &str) -> String {
    format!("vclip:source_download_lock:{}:{}", user_id, video_id)
}

/// R2 key for source video.
fn source_video_r2_key(user_id: &str, video_id: &str) -> String {
    format!("sources/{}/{}/source.mp4", user_id, video_id)
}

/// Process a download source job.
///
/// Downloads the original video from the URL and uploads to R2 for caching.
/// Uses Redis single-flight locking to prevent duplicate downloads.
pub async fn process_download_source_job(
    ctx: &EnhancedProcessingContext,
    job: &DownloadSourceJob,
) -> WorkerResult<()> {
    let logger = JobLogger::new(&job.job_id, "download_source");
    logger.log_start(&format!(
        "Downloading source video for {} from {}",
        job.video_id, job.video_url
    ));

    // Try to acquire single-flight lock
    let lock_key = download_lock_key(&job.user_id, job.video_id.as_str());
    let lock_token = acquire_download_lock(ctx, &lock_key).await?;

    let lock_token = match lock_token {
        Some(t) => t,
        None => {
            info!(
                video_id = %job.video_id,
                "Download lock already held by another worker, skipping"
            );
            // This is a success case - another worker is handling it
            return Ok(());
        }
    };

    // Lock acquired, proceed with download
    let result = execute_download(ctx, job).await;

    // Release lock on completion (success or failure)
    if let Err(e) = release_download_lock(ctx, &lock_key, &lock_token).await {
        warn!("Failed to release download lock: {}", e);
    }

    result?;

    logger.log_completion("Source video downloaded and uploaded to R2");
    Ok(())
}

/// Try to acquire a single-flight lock for the download.
async fn acquire_download_lock(
    ctx: &EnhancedProcessingContext,
    key: &str,
) -> WorkerResult<Option<String>> {
    let mut conn = ctx
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| WorkerError::job_failed(format!("Redis connection failed: {}", e)))?;

    let lock_value = format!("worker:{}", uuid::Uuid::new_v4());

    // SET key value NX EX ttl
    let result: Option<String> = redis::cmd("SET")
        .arg(key)
        .arg(&lock_value)
        .arg("NX")
        .arg("EX")
        .arg(DOWNLOAD_LOCK_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| WorkerError::job_failed(format!("Redis SET failed: {}", e)))?;

    // SET with NX returns "OK" if set, None if key exists
    if result.is_some() {
        Ok(Some(lock_value))
    } else {
        Ok(None)
    }
}

/// Release the download lock.
async fn release_download_lock(
    ctx: &EnhancedProcessingContext,
    key: &str,
    lock_token: &str,
) -> WorkerResult<()> {
    let mut conn = ctx
        .redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| WorkerError::job_failed(format!("Redis connection failed: {}", e)))?;

    let script = Script::new(
        r#"
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('DEL', KEYS[1])
        else
            return 0
        end
        "#,
    );
    let _deleted: i32 = script
        .key(key)
        .arg(lock_token)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| WorkerError::job_failed(format!("Redis unlock script failed: {}", e)))?;

    debug!("Released download lock: {}", key);
    Ok(())
}

/// Execute the download and upload workflow.
async fn execute_download(
    ctx: &EnhancedProcessingContext,
    job: &DownloadSourceJob,
) -> WorkerResult<()> {
    let video_repo = vclip_firestore::VideoRepository::new(ctx.firestore.clone(), &job.user_id);
    let r2_key = source_video_r2_key(&job.user_id, job.video_id.as_str());

    // Mark as downloading
    if let Err(e) = video_repo.set_source_video_downloading(&job.video_id).await {
        warn!("Failed to set source video status to downloading: {}", e);
        // Continue anyway - status is informational
    }

    // Create temp directory for download
    let temp_dir = tempfile::tempdir()
        .map_err(|e| WorkerError::job_failed(format!("Failed to create temp dir: {}", e)))?;
    let video_file = temp_dir.path().join("source.mp4");

    // Download video from original URL
    info!(
        video_id = %job.video_id,
        url = %job.video_url,
        "Downloading source video from origin"
    );

    if let Err(e) = download_video(&job.video_url, &video_file).await {
        // Mark as failed
        let error_msg = format!("Download failed: {}", e);
        if let Err(fe) = video_repo
            .set_source_video_failed(&job.video_id, Some(&error_msg))
            .await
        {
            warn!("Failed to set source video status to failed: {}", fe);
        }
        return Err(WorkerError::job_failed(error_msg));
    }

    // Upload to R2
    info!(
        video_id = %job.video_id,
        r2_key = %r2_key,
        "Uploading source video to R2"
    );

    if let Err(e) = ctx
        .storage
        .upload_file(&video_file, &r2_key, "video/mp4")
        .await
    {
        // Mark as failed
        let error_msg = format!("R2 upload failed: {}", e);
        if let Err(fe) = video_repo
            .set_source_video_failed(&job.video_id, Some(&error_msg))
            .await
        {
            warn!("Failed to set source video status to failed: {}", fe);
        }
        return Err(WorkerError::job_failed(error_msg));
    }

    // Calculate expiration time
    let expires_at = Utc::now() + ChronoDuration::hours(SOURCE_VIDEO_TTL_HOURS);

    // Mark as ready with retries (critical update)
    let mut retry_count = 0;
    let max_retries = 3;
    loop {
        match video_repo
            .set_source_video_ready(&job.video_id, &r2_key, expires_at)
            .await
        {
            Ok(()) => {
                info!(
                    video_id = %job.video_id,
                    r2_key = %r2_key,
                    expires_at = %expires_at,
                    "Source video ready in R2"
                );
                break;
            }
            Err(e) => {
                retry_count += 1;
                if retry_count >= max_retries {
                    // Don't fail the job - the R2 upload succeeded
                    // Just log and return success
                    warn!(
                        "Failed to update Firestore after {} retries: {}. R2 upload succeeded, continuing.",
                        max_retries, e
                    );
                    break;
                }
                warn!(
                    "Firestore update failed (attempt {}/{}): {}, retrying...",
                    retry_count, max_retries, e
                );
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }

    // Phase 5: Update storage accounting (non-billable source video cache)
    // Get file size for accounting
    if let Ok(metadata) = tokio::fs::metadata(&video_file).await {
        let file_size = metadata.len();
        let storage_repo = vclip_firestore::StorageAccountingRepository::new(
            ctx.firestore.clone(),
            &job.user_id,
        );
        if let Err(e) = storage_repo.add_source_video(file_size).await {
            warn!(
                user_id = %job.user_id,
                size_bytes = file_size,
                error = %e,
                "Failed to update storage accounting for source video (non-critical)"
            );
        }
    }

    // Temp dir is automatically cleaned up when it goes out of scope
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_key_format() {
        let key = download_lock_key("user123", "video456");
        assert_eq!(key, "vclip:source_download_lock:user123:video456");
    }

    #[test]
    fn test_r2_key_format() {
        let key = source_video_r2_key("user123", "video456");
        assert_eq!(key, "sources/user123/video456/source.mp4");
    }
}
