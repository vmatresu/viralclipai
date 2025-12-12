//! Neural analysis cache service.
//!
//! Provides concurrency-safe caching of expensive neural analysis results
//! (YuNet face detection, FaceMesh landmarks) to R2. Uses Redis single-flight
//! locking to prevent duplicate computation across workers.
//!
//! # Usage
//!
//! ```ignore
//! let service = NeuralCacheService::new(r2_client, redis_client);
//!
//! // Try to get cached analysis, or run detection with lock
//! let analysis = service.get_or_compute(
//!     &user_id,
//!     &video_id,
//!     scene_id,
//!     || async { run_detection().await }
//! ).await?;
//! ```

use redis::Script;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use vclip_models::{DetectionTier, SceneNeuralAnalysis};
use vclip_storage::{
    load_neural_analysis, neural_cache_key, store_neural_analysis, R2Client,
};

use crate::error::{WorkerError, WorkerResult};

/// Lock TTL for neural analysis computation (1 hour).
const LOCK_TTL_SECS: u64 = 3600;

/// Maximum retries when lock is held by another worker.
const MAX_LOCK_RETRIES: u32 = 10;

/// Delay between lock retry attempts.
const LOCK_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Redis key prefix for neural analysis locks.
const LOCK_KEY_PREFIX: &str = "vclip:neural_lock";

/// Service for neural analysis caching with single-flight locking.
#[derive(Clone)]
pub struct NeuralCacheService {
    r2: R2Client,
    redis: redis::Client,
    lock_tokens: Arc<Mutex<HashMap<String, String>>>,
}

impl NeuralCacheService {
    /// Create a new neural cache service.
    pub fn new(r2: R2Client, redis: redis::Client) -> Self {
        Self {
            r2,
            redis,
            lock_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Try to load cached neural analysis from R2.
    ///
    /// Returns `Ok(None)` if cache miss (not found, corrupt, or version mismatch).
    pub async fn get_cached(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
    ) -> WorkerResult<Option<SceneNeuralAnalysis>> {
        self.get_cached_for_tier(user_id, video_id, scene_id, DetectionTier::None)
            .await
    }

    pub async fn get_cached_for_tier(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        required_tier: DetectionTier,
    ) -> WorkerResult<Option<SceneNeuralAnalysis>> {
        let key = neural_cache_key(user_id, video_id, scene_id);
        debug!(key = %key, required_tier = %required_tier, "Checking neural cache");

        match load_neural_analysis(&self.r2, user_id, video_id, scene_id).await {
            Some(analysis) => {
                if analysis.detection_tier.speed_rank() < required_tier.speed_rank() {
                    debug!(
                        key = %key,
                        cached_tier = %analysis.detection_tier,
                        required_tier = %required_tier,
                        "Neural cache MISS (tier too low)"
                    );
                    return Ok(None);
                }

                info!(
                    key = %key,
                    frames = analysis.frames.len(),
                    tier = %analysis.detection_tier,
                    "Neural cache HIT"
                );
                Ok(Some(analysis))
            }
            None => {
                debug!(key = %key, "Neural cache MISS");
                Ok(None)
            }
        }
    }

    /// Store neural analysis to R2 cache.
    /// Returns the actual compressed size in bytes for accurate storage accounting.
    pub async fn store(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        analysis: &SceneNeuralAnalysis,
    ) -> WorkerResult<u64> {
        debug!(
            user_id = %user_id,
            video_id = %video_id,
            scene_id = scene_id,
            frames = analysis.frames.len(),
            "Storing neural analysis to cache"
        );

        let result = store_neural_analysis(&self.r2, user_id, video_id, scene_id, analysis)
            .await
            .map_err(|e| WorkerError::Storage(e))?;

        info!(
            key = %result.key,
            frames = analysis.frames.len(),
            compressed_size = result.compressed_size,
            "Neural cache stored"
        );
        Ok(result.compressed_size)
    }

    /// Acquire single-flight lock for neural analysis computation.
    ///
    /// Returns `true` if lock was acquired, `false` if lock is held by another worker.
    pub async fn try_acquire_lock(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
    ) -> WorkerResult<bool> {
        let lock_key = format!("{}:{}:{}:{}", LOCK_KEY_PREFIX, user_id, video_id, scene_id);
        let lock_value = format!("worker:{}", uuid::Uuid::new_v4());

        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| WorkerError::job_failed(format!("Redis connection failed: {}", e)))?;

        // SET NX EX (only if not exists, with expiry)
        let result: Option<String> = redis::cmd("SET")
            .arg(&lock_key)
            .arg(&lock_value)
            .arg("NX")
            .arg("EX")
            .arg(LOCK_TTL_SECS)
            .query_async(&mut conn)
            .await
            .map_err(|e| WorkerError::job_failed(format!("Redis SET failed: {}", e)))?;

        let acquired = result.is_some();
        debug!(
            lock_key = %lock_key,
            acquired = acquired,
            "Neural lock acquisition attempt"
        );

        if acquired {
            let mut tokens = self.lock_tokens.lock().await;
            tokens.insert(lock_key, lock_value);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Release neural analysis lock.
    pub async fn release_lock(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
    ) -> WorkerResult<()> {
        let lock_key = format!("{}:{}:{}:{}", LOCK_KEY_PREFIX, user_id, video_id, scene_id);
        let lock_token = { self.lock_tokens.lock().await.remove(&lock_key) };
        let Some(lock_token) = lock_token else {
            debug!(lock_key = %lock_key, "Neural lock released");
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
            .key(&lock_key)
            .arg(&lock_token)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| WorkerError::job_failed(format!("Redis unlock script failed: {}", e)))?;

        debug!(lock_key = %lock_key, "Neural lock released");
        Ok(())
    }

    /// Get cached analysis or compute with single-flight locking.
    ///
    /// 1. Check cache - if hit, return immediately
    /// 2. Try to acquire lock
    /// 3. If lock acquired: compute, store, release lock, return
    /// 4. If lock not acquired: wait and retry cache check
    ///
    /// # Arguments
    /// * `user_id` - User ID for cache key
    /// * `video_id` - Video ID for cache key
    /// * `scene_id` - Scene ID for cache key
    /// * `compute_fn` - Async function to compute analysis if cache miss
    ///
    /// # Returns
    /// The analysis and optionally the stored bytes (if newly computed and stored)
    pub async fn get_or_compute<F, Fut>(
        &self,
        user_id: &str,
        video_id: &str,
        scene_id: u32,
        required_tier: DetectionTier,
        compute_fn: F,
    ) -> WorkerResult<(SceneNeuralAnalysis, Option<u64>)>
    where
        F: Fn() -> Fut + Clone,
        Fut: std::future::Future<Output = WorkerResult<SceneNeuralAnalysis>>,
    {
        // Step 1: Check cache first
        if let Some(cached) = self
            .get_cached_for_tier(user_id, video_id, scene_id, required_tier)
            .await?
        {
            return Ok((cached, None)); // Cache hit - no new bytes stored
        }

        // Step 2: Try to acquire lock
        let lock_acquired = self.try_acquire_lock(user_id, video_id, scene_id).await?;

        if lock_acquired {
            // We have the lock - compute, store, release
            info!(
                user_id = %user_id,
                video_id = %video_id,
                scene_id = scene_id,
                "Lock acquired, computing neural analysis"
            );

            let result = compute_fn().await;
            let mut stored_bytes = None;

            if let Ok(analysis) = &result {
                // Store to cache and track actual size
                match self.store(user_id, video_id, scene_id, analysis).await {
                    Ok(bytes) => stored_bytes = Some(bytes),
                    Err(e) => {
                        warn!(
                            user_id = %user_id,
                            video_id = %video_id,
                            scene_id = scene_id,
                            error = %e,
                            "Failed to store neural analysis to cache"
                        );
                    }
                }
            }

            // Always release lock, even on error
            if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                warn!(error = %e, "Failed to release lock");
            }

            result.map(|a| (a, stored_bytes))
        } else {
            // Lock held by another worker - wait and retry cache check
            info!(
                user_id = %user_id,
                video_id = %video_id,
                scene_id = scene_id,
                "Lock held by another worker, waiting for cache"
            );

            for retry in 1..=MAX_LOCK_RETRIES {
                tokio::time::sleep(LOCK_RETRY_DELAY).await;

                // Check cache again
                if let Some(cached) = self
                    .get_cached_for_tier(user_id, video_id, scene_id, required_tier)
                    .await?
                {
                    info!(
                        user_id = %user_id,
                        video_id = %video_id,
                        scene_id = scene_id,
                        retry = retry,
                        "Cache populated by another worker"
                    );
                    return Ok((cached, None)); // Cache hit - no new bytes stored by us
                }

                // Try to acquire lock (maybe original holder finished)
                if self.try_acquire_lock(user_id, video_id, scene_id).await? {
                    // We got the lock this time
                    info!(
                        user_id = %user_id,
                        video_id = %video_id,
                        scene_id = scene_id,
                        retry = retry,
                        "Lock acquired on retry"
                    );

                    // Double-check cache in case it was populated
                    if let Some(cached) = self
                        .get_cached_for_tier(user_id, video_id, scene_id, required_tier)
                        .await?
                    {
                        if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                            warn!(error = %e, "Failed to release lock after cache hit");
                        }
                        return Ok((cached, None));
                    }

                    let result = compute_fn().await;
                    let mut stored_bytes = None;

                    if let Ok(analysis) = &result {
                        match self.store(user_id, video_id, scene_id, analysis).await {
                            Ok(bytes) => stored_bytes = Some(bytes),
                            Err(e) => {
                                warn!(
                                    user_id = %user_id,
                                    video_id = %video_id,
                                    scene_id = scene_id,
                                    error = %e,
                                    "Failed to store neural analysis to cache"
                                );
                            }
                        }
                    }

                    if let Err(e) = self.release_lock(user_id, video_id, scene_id).await {
                        warn!(error = %e, "Failed to release lock");
                    }

                    return result.map(|a| (a, stored_bytes));
                }
            }

            // Max retries exceeded
            Err(WorkerError::job_failed(format!(
                "Neural cache lock contention timeout for {}/{}/{}",
                user_id, video_id, scene_id
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_key_format() {
        let lock_key = format!("{}:{}:{}:{}", LOCK_KEY_PREFIX, "user1", "video1", 5);
        assert_eq!(lock_key, "vclip:neural_lock:user1:video1:5");
    }
}
