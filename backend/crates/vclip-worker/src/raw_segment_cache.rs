//! Raw segment cache service.
//!
//! Provides single-flight extraction and caching of raw video segments to R2.
//! This avoids re-extracting the same segment when rendering multiple styles
//! for the same scene.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use redis::Script;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use vclip_storage::R2Client;

use crate::error::{WorkerError, WorkerResult};

/// Lock TTL for single-flight extraction (1 hour).
const LOCK_TTL_SECS: u64 = 3600;

/// Maximum retries when waiting for another worker to finish extraction.
const MAX_LOCK_WAIT_RETRIES: u32 = 5;

/// Delay between lock wait retries.
const LOCK_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Redis key prefix for raw segment locks.
const LOCK_KEY_PREFIX: &str = "vclip:raw_lock";

/// Generate R2 key for raw segment.
///
/// Format: `clips/{user_id}/{video_id}/raw/{scene_id}.mp4`
pub fn raw_segment_r2_key(user_id: &str, video_id: &str, scene_id: u32) -> String {
    format!("clips/{}/{}/raw/{}.mp4", user_id, video_id, scene_id)
}

/// Generate Redis lock key for single-flight extraction.
fn lock_key(user_id: &str, video_id: &str, scene_id: u32) -> String {
    format!("{}:{}:{}:{}", LOCK_KEY_PREFIX, user_id, video_id, scene_id)
}

/// Service for raw segment caching with single-flight locking.
#[derive(Clone)]
pub struct RawSegmentCacheService {
    storage: R2Client,
    redis: redis::Client,
    lock_tokens: Arc<Mutex<HashMap<String, String>>>,
}

impl RawSegmentCacheService {
    /// Create a new raw segment cache service.
    pub fn new(storage: R2Client, redis: redis::Client) -> Self {
        Self {
            storage,
            redis,
            lock_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get or create a raw segment, with optional direct segment download.
    ///
    /// Priority:
    /// 1. Check local file
    /// 2. Check R2 cache
    /// 3. Try direct segment download from URL (if provided and supported)
    /// 4. Fall back to extracting from source video
    ///
    /// This is more efficient for YouTube videos where HLS segment download works.
    pub async fn get_or_create_with_segment_download(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        source_video: Option<&Path>,
        video_url: Option<&str>,
        start_secs: f64,
        end_secs: f64,
        work_dir: &Path,
    ) -> WorkerResult<(PathBuf, bool)> {
        let r2_key = raw_segment_r2_key(user_id, video_id, scene_id);
        let local_path = work_dir.join(format!("raw_{}.mp4", scene_id));

        // 1. Check local file
        if local_path.exists() {
            if let Ok(meta) = std::fs::metadata(&local_path) {
                if meta.len() > 0 {
                    debug!(scene_id = scene_id, "Using existing local raw segment");
                    return Ok((local_path, false));
                }
            }
        }

        // 2. Check R2 cache
        if self.check_raw_exists(&r2_key).await {
            if self.download_raw_segment(&r2_key, &local_path).await? {
                info!(scene_id = scene_id, "Using cached raw segment from R2");
                return Ok((local_path, false));
            }
        }

        // 3. Try direct segment download from URL
        if let Some(url) = video_url {
            if vclip_media::likely_supports_segment_download(url) {
                if self.try_acquire_lock(user_id, video_id, scene_id).await? {
                    info!(scene_id = scene_id, "Trying direct segment download from URL");
                    
                    match vclip_media::download_segment(
                        url,
                        start_secs,
                        end_secs,
                        &local_path,
                        true, // force_keyframes for accurate cuts
                    )
                    .await
                    {
                        Ok(()) => {
                            // Upload to R2 for future use
                            if let Err(e) = self.upload_raw_segment(&local_path, &r2_key).await {
                                warn!(
                                    scene_id = scene_id,
                                    error = %e,
                                    "Failed to upload segment to R2 (non-critical)"
                                );
                            }
                            
                            if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                                warn!("Failed to release raw segment lock: {}", e);
                            }
                            
                            info!(
                                scene_id = scene_id,
                                "Downloaded segment directly from URL"
                            );
                            return Ok((local_path, true));
                        }
                        Err(e) => {
                            // Check if it's a "not supported" error - graceful fallback
                            if e.downcast_ref::<vclip_media::SegmentDownloadNotSupported>().is_some() {
                                info!(
                                    scene_id = scene_id,
                                    "Segment download not supported, falling back to source extraction"
                                );
                            } else {
                                warn!(
                                    scene_id = scene_id,
                                    error = %e,
                                    "Segment download failed, falling back to source extraction"
                                );
                            }
                            // Continue to source extraction - don't release lock yet
                        }
                    }
                    
                    // We still hold the lock - try source extraction
                    if let Some(source) = source_video {
                        if source.exists() {
                            let start_ts = format_timestamp_from_secs(start_secs);
                            let end_ts = format_timestamp_from_secs(end_secs);
                            
                            let result = self
                                .extract_and_upload(source, &start_ts, &end_ts, &local_path, &r2_key)
                                .await;

                            if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                                warn!("Failed to release raw segment lock: {}", e);
                            }

                            result?;
                            return Ok((local_path, true));
                        }
                    }
                    
                    // Neither segment download nor source extraction worked
                    if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                        warn!("Failed to release raw segment lock: {}", e);
                    }
                    
                    return Err(WorkerError::job_failed(format!(
                        "Neither segment download nor source extraction available for scene {}",
                        scene_id
                    )));
                }
            }
        }

        // 4. Fall back to source extraction (original path)
        if let Some(source) = source_video {
            if source.exists() {
                let start_ts = format_timestamp_from_secs(start_secs);
                let end_ts = format_timestamp_from_secs(end_secs);
                return self.get_or_create_internal(
                    user_id, video_id, scene_id, source, &start_ts, &end_ts, work_dir
                ).await;
            }
        }

        Err(WorkerError::job_failed(format!(
            "No source available for raw segment extraction for scene {}",
            scene_id
        )))
    }

    pub async fn get_or_create_with_outcome(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        source_video: &Path,
        start: &str,
        end: &str,
        work_dir: &Path,
    ) -> WorkerResult<(PathBuf, bool)> {
        self.get_or_create_internal(user_id, video_id, scene_id, source_video, start, end, work_dir)
            .await
    }

    async fn get_or_create_internal(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        source_video: &Path,
        start: &str,
        end: &str,
        work_dir: &Path,
    ) -> WorkerResult<(PathBuf, bool)> {
        let r2_key = raw_segment_r2_key(user_id, video_id, scene_id);
        let local_path = work_dir.join(format!("raw_{}.mp4", scene_id));

        if local_path.exists() {
            if let Ok(meta) = std::fs::metadata(&local_path) {
                if meta.len() > 0 {
                    debug!("Using existing local raw segment: {:?}", local_path);
                    return Ok((local_path, false));
                }
            }
        }

        if self.check_raw_exists(&r2_key).await {
            if self.download_raw_segment(&r2_key, &local_path).await? {
                info!(scene_id = scene_id, "Using cached raw segment from R2");
                return Ok((local_path, false));
            }
        }

        if self.try_acquire_lock(user_id, video_id, scene_id).await? {
            info!(scene_id = scene_id, "Raw segment not found, extracting...");

            let result = self
                .extract_and_upload(source_video, start, end, &local_path, &r2_key)
                .await;

            if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                warn!("Failed to release raw segment lock: {}", e);
            }

            result?;
            return Ok((local_path, true));
        }

        for retry in 0..MAX_LOCK_WAIT_RETRIES {
            debug!(
                "Waiting for another worker to finish extraction (attempt {}/{})",
                retry + 1,
                MAX_LOCK_WAIT_RETRIES
            );
            tokio::time::sleep(LOCK_RETRY_DELAY).await;

            if self.check_raw_exists(&r2_key).await {
                if self.download_raw_segment(&r2_key, &local_path).await? {
                    info!(scene_id = scene_id, "Using cached raw segment from R2 (after wait)");
                    return Ok((local_path, false));
                }
            }
        }

        if self.try_acquire_lock(user_id, video_id, scene_id).await? {
            info!(scene_id = scene_id, "Acquired lock after retry, extracting...");

            let result = self
                .extract_and_upload(source_video, start, end, &local_path, &r2_key)
                .await;

            if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                warn!("Failed to release raw segment lock: {}", e);
            }

            result?;
            return Ok((local_path, true));
        }

        Err(WorkerError::job_failed(format!(
            "Failed to acquire raw segment lock for scene {} after {} retries",
            scene_id, MAX_LOCK_WAIT_RETRIES
        )))
    }

    /// Check if raw segment exists in R2.
    pub async fn check_raw_exists(&self, r2_key: &str) -> bool {
        match self.storage.exists(r2_key).await {
            Ok(true) => {
                debug!("Raw segment exists in R2: {}", r2_key);
                true
            }
            Ok(false) => {
                debug!("Raw segment not found in R2: {}", r2_key);
                false
            }
            Err(e) => {
                warn!("Error checking raw segment existence: {}", e);
                false
            }
        }
    }

    /// Download raw segment from R2 to local path.
    ///
    /// Returns `true` if download succeeded, `false` if not found.
    pub async fn download_raw_segment(&self, r2_key: &str, dest: &Path) -> WorkerResult<bool> {
        match self.storage.download_file(r2_key, dest).await {
            Ok(_) => {
                debug!("Downloaded raw segment from R2: {}", r2_key);
                Ok(true)
            }
            Err(e) => {
                // Check if it's a not-found error
                let err_str = e.to_string();
                if err_str.contains("not found") || err_str.contains("NoSuchKey") {
                    debug!("Raw segment not found in R2: {}", r2_key);
                    Ok(false)
                } else {
                    Err(WorkerError::job_failed(format!(
                        "Failed to download raw segment: {}",
                        e
                    )))
                }
            }
        }
    }

    /// Upload raw segment to R2.
    pub async fn upload_raw_segment(&self, local_path: &Path, r2_key: &str) -> WorkerResult<()> {
        self.storage
            .upload_file(local_path, r2_key, "video/mp4")
            .await
            .map_err(|e| WorkerError::job_failed(format!("Failed to upload raw segment: {}", e)))?;

        info!("Uploaded raw segment to R2: {}", r2_key);
        Ok(())
    }

    /// Try to acquire single-flight lock for extraction.
    ///
    /// Returns `true` if lock was acquired, `false` if lock is held by another worker.
    async fn try_acquire_lock(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
    ) -> WorkerResult<bool> {
        let key = lock_key(user_id, video_id, scene_id);
	    let lock_value = format!("worker:{}", uuid::Uuid::new_v4());

        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| WorkerError::job_failed(format!("Redis connection failed: {}", e)))?;

        // SET key value NX EX ttl
        let result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(&lock_value)
            .arg("NX")
            .arg("EX")
            .arg(LOCK_TTL_SECS)
            .query_async(&mut conn)
            .await
            .map_err(|e| WorkerError::job_failed(format!("Redis SET failed: {}", e)))?;

        // SET with NX returns "OK" if set, None if key exists
        let acquired = result.is_some();
        if acquired {
            debug!("Acquired raw segment lock: {}", key);
        } else {
            debug!("Raw segment lock held by another worker: {}", key);
        }

	    if acquired {
	        let mut tokens = self.lock_tokens.lock().await;
	        tokens.insert(key, lock_value);
	        Ok(true)
	    } else {
	        Ok(false)
	    }
    }

    /// Release extraction lock.
    async fn release_lock(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
    ) -> WorkerResult<()> {
        let key = lock_key(user_id, video_id, scene_id);
	    let lock_token = { self.lock_tokens.lock().await.remove(&key) };
	    let Some(lock_token) = lock_token else {
	        debug!("Released raw segment lock: {}", key);
	        return Ok(());
	    };

        let mut conn = self
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
	        .key(&key)
	        .arg(&lock_token)
	        .invoke_async(&mut conn)
	        .await
	        .map_err(|e| WorkerError::job_failed(format!("Redis unlock script failed: {}", e)))?;

        debug!("Released raw segment lock: {}", key);
        Ok(())
    }

    /// Get or create a raw segment with single-flight locking.
    ///
    /// Flow:
    /// 1. Check if raw exists in R2
    /// 2. If exists: download and return path
    /// 3. If not: acquire lock, re-check, extract, upload, release lock
    ///
    /// # Arguments
    /// * `user_id` - User ID
    /// * `video_id` - Video ID
    /// * `scene_id` - Scene ID
    /// * `source_video` - Path to the full source video
    /// * `start` - Start timestamp (HH:MM:SS format)
    /// * `end` - End timestamp (HH:MM:SS format)
    /// * `work_dir` - Work directory for temporary files
    ///
    /// # Returns
    /// Path to the raw segment (local file)
    pub async fn get_or_create(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        source_video: &Path,
        start: &str,
        end: &str,
        work_dir: &Path,
    ) -> WorkerResult<PathBuf> {
	    let (path, _created) = self
	        .get_or_create_internal(user_id, video_id, scene_id, source_video, start, end, work_dir)
	        .await?;
	    Ok(path)
    }

    /// Extract raw segment from source video and upload to R2.
    async fn extract_and_upload(
        &self,
        source_video: &Path,
        start: &str,
        end: &str,
        local_path: &Path,
        r2_key: &str,
    ) -> WorkerResult<()> {
        // Extract using stream copy (fast, no re-encode)
        extract_raw_segment(source_video, start, end, local_path).await?;

        // Upload to R2
        self.upload_raw_segment(local_path, r2_key).await?;
        Ok(())
    }
}

/// Extract raw segment from source video using stream copy (fast, no re-encode).
///
/// Uses single input seeking which is the correct approach for stream copy:
/// - `-ss` before `-i`: Fast seek to nearest keyframe at or before start time
/// - `-c copy`: Stream copy (no decoding/encoding)
/// - Output starts at the keyframe (may be slightly before requested time)
///
/// **Important**: Do NOT use two `-ss` flags with `-c copy` - the second `-ss`
/// after `-i` drops packets without their keyframes, causing black/frozen frames.
pub async fn extract_raw_segment(
    source_video: &Path,
    start: &str,
    end: &str,
    output: &Path,
) -> WorkerResult<()> {
    use tokio::process::Command;

    info!(
        "Extracting raw segment: {:?} [{} -> {}] => {:?}",
        source_video, start, end, output
    );

    let start_secs = parse_timestamp_to_secs(start);
    let end_secs = parse_timestamp_to_secs(end);
    let duration = end_secs - start_secs;

    // Single input seek with stream copy - the ONLY correct way for -c copy
    // Output will start at the nearest keyframe at or before start_secs
    let output_status = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel", "error",
            // Input seeking - seeks to nearest keyframe at or before start
            "-ss",
            &format!("{:.3}", start_secs),
            "-i",
        ])
        .arg(source_video)
        .args([
            // Duration to extract
            "-t",
            &format!("{:.3}", duration),
            // Stream copy (no re-encode) - fast and lossless
            "-c", "copy",
            // Fix timestamp issues from stream copy
            "-avoid_negative_ts", "make_zero",
            // Ensure proper muxing
            "-movflags", "+faststart",
        ])
        .arg(output)
        .output()
        .await
        .map_err(|e| WorkerError::job_failed(format!("Failed to run ffmpeg: {}", e)))?;

    if !output_status.status.success() {
        let stderr = String::from_utf8_lossy(&output_status.stderr);
        return Err(WorkerError::job_failed(format!(
            "FFmpeg stream copy failed: {}",
            stderr
        )));
    }

    // Verify output exists
    if !output.exists() {
        return Err(WorkerError::job_failed(
            "FFmpeg completed but output file not found".to_string(),
        ));
    }

    let metadata = tokio::fs::metadata(output).await.map_err(|e| {
        WorkerError::job_failed(format!("Failed to get output file metadata: {}", e))
    })?;

    info!(
        "Extracted raw segment: {:?} ({} bytes)",
        output,
        metadata.len()
    );

    Ok(())
}

/// Parse timestamp string to seconds.
/// Supports formats: "HH:MM:SS.mmm", "MM:SS.mmm", "SS.mmm"
fn parse_timestamp_to_secs(ts: &str) -> f64 {
    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        1 => parts[0].parse().unwrap_or(0.0),
        2 => {
            let mins: f64 = parts[0].parse().unwrap_or(0.0);
            let secs: f64 = parts[1].parse().unwrap_or(0.0);
            mins * 60.0 + secs
        }
        3 => {
            let hours: f64 = parts[0].parse().unwrap_or(0.0);
            let mins: f64 = parts[1].parse().unwrap_or(0.0);
            let secs: f64 = parts[2].parse().unwrap_or(0.0);
            hours * 3600.0 + mins * 60.0 + secs
        }
        _ => 0.0,
    }
}

/// Format seconds as HH:MM:SS.mmm timestamp for FFmpeg.
fn format_timestamp_from_secs(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_r2_key_format() {
        let key = raw_segment_r2_key("user123", "video456", 1);
        assert_eq!(key, "clips/user123/video456/raw/1.mp4");
    }

    #[test]
    fn test_lock_key_format() {
        let key = lock_key("user123", "video456", 2);
        assert_eq!(key, "vclip:raw_lock:user123:video456:2");
    }
}
