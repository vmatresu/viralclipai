//! Source video download coordinator.
//!
//! Provides unified download coordination across workers to prevent duplicate downloads.
//! Uses Redis for single-flight locking and Firestore for status tracking.

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use redis::Script;
use tracing::{debug, info, warn};

use vclip_firestore::FirestoreClient;
use vclip_models::{SourceVideoStatus, VideoId};

use crate::error::{WorkerError, WorkerResult};

/// Lock TTL for single-flight download (1 hour).
const DOWNLOAD_LOCK_TTL_SECS: u64 = 3600;

/// Default timeout for waiting on another worker's download (10 minutes).
const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(600);

/// Polling interval when waiting for download completion.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Redis key prefix for download locks.
/// IMPORTANT: Must match the key format in download_source_job.rs for coordination.
fn download_lock_key(user_id: &str, video_id: &str) -> String {
    format!("vclip:source_download_lock:{}:{}", user_id, video_id)
}

/// R2 key for source video.
pub fn source_video_r2_key(user_id: &str, video_id: &str) -> String {
    format!("sources/{}/{}/source.mp4", user_id, video_id)
}

/// Action to take after checking download status.
#[derive(Debug)]
pub enum DownloadAction {
    /// Source video is ready in R2, use cached version.
    UseCache { r2_key: String },
    /// Another worker is downloading, wait for completion.
    WaitForOther,
    /// No download in progress, we should perform the download.
    PerformDownload { lock_token: String },
}

/// Coordinator for source video downloads across distributed workers.
pub struct SourceVideoDownloadCoordinator {
    redis: redis::Client,
    firestore: FirestoreClient,
}

impl SourceVideoDownloadCoordinator {
    /// Create a new download coordinator.
    pub fn new(redis: redis::Client, firestore: FirestoreClient) -> Self {
        Self { redis, firestore }
    }

    /// Check download status and determine action.
    ///
    /// Returns:
    /// - `UseCache` if source is already ready in R2
    /// - `WaitForOther` if another worker is currently downloading
    /// - `PerformDownload` if no download in progress (we acquired the lock)
    pub async fn acquire_or_wait_for_download(
        &self,
        user_id: &str,
        video_id: &str,
    ) -> WorkerResult<DownloadAction> {
        // 1. Check Firestore status first
        let video_repo = vclip_firestore::VideoRepository::new(self.firestore.clone(), user_id);
        
        let video_id_ref = VideoId::from_string(video_id);
        if let Ok(Some(video)) = video_repo.get(&video_id_ref).await {
            // If status is Ready and we have an R2 key, use cache
            if let (Some(SourceVideoStatus::Ready), Some(r2_key)) = 
                (&video.source_video_status, &video.source_video_r2_key) 
            {
                // Check if expired in metadata (but R2 might still have it)
                let expired = video.source_video_expires_at
                    .map(|exp| exp <= Utc::now())
                    .unwrap_or(false);
                
                if !expired {
                    info!(
                        video_id = video_id,
                        r2_key = r2_key.as_str(),
                        "Source video ready in cache"
                    );
                    return Ok(DownloadAction::UseCache { r2_key: r2_key.clone() });
                }
            }
            
            // If status is Downloading, another worker is handling it
            if video.source_video_status == Some(SourceVideoStatus::Downloading) {
                info!(
                    video_id = video_id,
                    "Source video is being downloaded by another worker"
                );
                return Ok(DownloadAction::WaitForOther);
            }
        }

        // 2. Try to acquire Redis lock
        let lock_key = download_lock_key(user_id, video_id);
        match self.try_acquire_lock(&lock_key).await? {
            Some(lock_token) => {
                info!(
                    video_id = video_id,
                    "Acquired download lock, will perform download"
                );
                Ok(DownloadAction::PerformDownload { lock_token })
            }
            None => {
                // Lock held by another worker
                info!(
                    video_id = video_id,
                    "Download lock held by another worker, will wait"
                );
                Ok(DownloadAction::WaitForOther)
            }
        }
    }

    /// Wait for an in-progress download to complete.
    ///
    /// Polls Firestore status until Ready, Failed, or timeout.
    /// Returns the final status.
    pub async fn wait_for_download_complete(
        &self,
        user_id: &str,
        video_id: &str,
        timeout: Option<Duration>,
    ) -> WorkerResult<WaitResult> {
        let timeout = timeout.unwrap_or(DEFAULT_WAIT_TIMEOUT);
        let deadline = tokio::time::Instant::now() + timeout;
        let video_repo = vclip_firestore::VideoRepository::new(self.firestore.clone(), user_id);

        info!(
            video_id = video_id,
            timeout_secs = timeout.as_secs(),
            "Waiting for background download to complete"
        );

        loop {
            // Check timeout
            if tokio::time::Instant::now() >= deadline {
                warn!(
                    video_id = video_id,
                    "Timeout waiting for background download"
                );
                return Ok(WaitResult::Timeout);
            }

            // Poll Firestore
            let video_id_ref = VideoId::from_string(video_id);
            if let Ok(Some(video)) = video_repo.get(&video_id_ref).await {
                match video.source_video_status {
                    Some(SourceVideoStatus::Ready) => {
                        if let Some(r2_key) = video.source_video_r2_key {
                            info!(
                                video_id = video_id,
                                r2_key = r2_key.as_str(),
                                "Background download completed"
                            );
                            return Ok(WaitResult::Ready { r2_key });
                        }
                    }
                    Some(SourceVideoStatus::Failed) => {
                        let error = video.source_video_error.unwrap_or_default();
                        warn!(
                            video_id = video_id,
                            error = error.as_str(),
                            "Background download failed"
                        );
                        return Ok(WaitResult::Failed { error });
                    }
                    Some(SourceVideoStatus::Downloading) => {
                        // Still in progress, continue polling
                        debug!(video_id = video_id, "Background download still in progress");
                    }
                    _ => {
                        // Status changed to something unexpected
                        debug!(
                            video_id = video_id,
                            status = ?video.source_video_status,
                            "Unexpected status while waiting"
                        );
                    }
                }
            }

            // Wait before next poll
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Release a previously acquired download lock.
    pub async fn release_lock(
        &self,
        user_id: &str,
        video_id: &str,
        lock_token: &str,
    ) -> WorkerResult<()> {
        let lock_key = download_lock_key(user_id, video_id);
        
        let mut conn = self.redis
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
            .key(&lock_key)
            .arg(lock_token)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| WorkerError::job_failed(format!("Redis unlock failed: {}", e)))?;

        debug!(lock_key = lock_key.as_str(), "Released download lock");
        Ok(())
    }

    /// Mark download as in progress in Firestore.
    pub async fn mark_downloading(&self, user_id: &str, video_id: &str) -> WorkerResult<()> {
        let video_repo = vclip_firestore::VideoRepository::new(self.firestore.clone(), user_id);
        video_repo
            .set_source_video_downloading(&VideoId::from_string(video_id))
            .await
            .map_err(|e| WorkerError::Firestore(e))
    }

    /// Mark download as complete in Firestore.
    pub async fn mark_ready(
        &self,
        user_id: &str,
        video_id: &str,
        r2_key: &str,
        ttl_hours: i64,
    ) -> WorkerResult<()> {
        let video_repo = vclip_firestore::VideoRepository::new(self.firestore.clone(), user_id);
        let expires_at = Utc::now() + ChronoDuration::hours(ttl_hours);
        video_repo
            .set_source_video_ready(&VideoId::from_string(video_id), r2_key, expires_at)
            .await
            .map_err(|e| WorkerError::Firestore(e))
    }

    /// Mark download as failed in Firestore.
    pub async fn mark_failed(
        &self,
        user_id: &str,
        video_id: &str,
        error: &str,
    ) -> WorkerResult<()> {
        let video_repo = vclip_firestore::VideoRepository::new(self.firestore.clone(), user_id);
        video_repo
            .set_source_video_failed(&VideoId::from_string(video_id), Some(error))
            .await
            .map_err(|e| WorkerError::Firestore(e))
    }

    /// Try to acquire the download lock.
    async fn try_acquire_lock(&self, key: &str) -> WorkerResult<Option<String>> {
        let mut conn = self.redis
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

        if result.is_some() {
            Ok(Some(lock_value))
        } else {
            Ok(None)
        }
    }
}

/// Result of waiting for download completion.
#[derive(Debug)]
pub enum WaitResult {
    /// Download completed successfully, source available at R2 key.
    Ready { r2_key: String },
    /// Download failed with error message.
    Failed { error: String },
    /// Timed out waiting for download.
    Timeout,
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
