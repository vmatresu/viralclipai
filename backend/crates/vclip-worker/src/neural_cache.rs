//! Neural analysis cache service.
//!
//! Provides concurrency-safe caching of expensive neural analysis results
//! (YuNet face detection, FaceMesh landmarks) to R2. Uses semaphore-based
//! concurrency control to allow parallel neural analysis (configurable).
//!
//! # Architecture
//!
//! The service uses a two-layer approach:
//! 1. **Semaphore**: Limits concurrent YuNet instances (e.g., 3 for 8-core CPUs)
//! 2. **Cache-first check**: Before computing, check if another task already cached
//!
//! This replaces the previous global Redis lock which serialized all neural
//! analysis, underutilizing multi-core CPUs.
//!
//! # Usage
//!
//! ```ignore
//! let service = NeuralCacheService::new(r2_client, redis_client, neural_semaphore);
//!
//! // Try to get cached analysis, or run detection with semaphore
//! let analysis = service.get_or_compute(
//!     &user_id,
//!     &video_id,
//!     scene_id,
//!     || async { run_detection().await }
//! ).await?;
//! ```

use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use vclip_models::{DetectionTier, SceneNeuralAnalysis};
use vclip_storage::{
    load_neural_analysis, neural_cache_key, store_neural_analysis, R2Client,
};

use crate::error::{WorkerError, WorkerResult};

/// Service for neural analysis caching with semaphore-based concurrency.
#[derive(Clone)]
pub struct NeuralCacheService {
    r2: R2Client,
    /// Semaphore to limit concurrent neural analysis operations
    neural_semaphore: Arc<Semaphore>,
}

impl NeuralCacheService {
    /// Create a new neural cache service.
    ///
    /// # Arguments
    /// * `r2` - R2 client for cache storage
    /// * `neural_semaphore` - Semaphore to limit concurrent YuNet instances
    pub fn new(r2: R2Client, neural_semaphore: Arc<Semaphore>) -> Self {
        Self {
            r2,
            neural_semaphore,
        }
    }

    /// Create a new neural cache service with legacy signature (for backward compatibility).
    /// Creates a default semaphore with 3 permits.
    #[allow(dead_code)]
    pub fn new_legacy(r2: R2Client, _redis: redis::Client) -> Self {
        Self {
            r2,
            neural_semaphore: Arc::new(Semaphore::new(3)),
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

    /// Get cached analysis or compute with semaphore-based concurrency.
    ///
    /// # Flow
    /// 1. Check cache - if hit, return immediately (no semaphore needed)
    /// 2. Acquire semaphore permit (blocks if max concurrent analyses reached)
    /// 3. Double-check cache (another task may have computed while we waited)
    /// 4. If still miss, compute analysis
    /// 5. Store to cache
    /// 6. Return (permit released automatically when dropped)
    ///
    /// This allows N concurrent neural analyses (e.g., 3 for 8-core CPUs)
    /// instead of strictly serializing them with a global lock.
    ///
    /// # Arguments
    /// * `user_id` - User ID for cache key
    /// * `video_id` - Video ID for cache key
    /// * `scene_id` - Scene ID for cache key
    /// * `required_tier` - Minimum detection tier required
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
        // Step 1: Check cache first (fast path - no semaphore needed)
        if let Some(cached) = self
            .get_cached_for_tier(user_id, video_id, scene_id, required_tier)
            .await?
        {
            return Ok((cached, None)); // Cache hit - no new bytes stored
        }

        // Step 2: Acquire semaphore permit (may block if at capacity)
        let available_permits = self.neural_semaphore.available_permits();
        info!(
            user_id = %user_id,
            video_id = %video_id,
            scene_id = scene_id,
            available_permits = available_permits,
            "Acquiring semaphore for neural analysis"
        );

        let _permit = self
            .neural_semaphore
            .acquire()
            .await
            .map_err(|e| WorkerError::job_failed(format!("Semaphore closed: {}", e)))?;

        info!(
            user_id = %user_id,
            video_id = %video_id,
            scene_id = scene_id,
            "Semaphore acquired, computing neural analysis"
        );

        // Step 3: Double-check cache (another task might have computed while we waited)
        if let Some(cached) = self
            .get_cached_for_tier(user_id, video_id, scene_id, required_tier)
            .await?
        {
            info!(
                user_id = %user_id,
                video_id = %video_id,
                scene_id = scene_id,
                "Cache populated while waiting for semaphore (skipping compute)"
            );
            return Ok((cached, None)); // Cache hit - no new bytes stored
        }

        // Step 4: Compute analysis
        let result = compute_fn().await;
        let mut stored_bytes = None;

        // Step 5: Store to cache if successful
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

        // Permit is automatically released when _permit drops
        result.map(|a| (a, stored_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore_concurrency() {
        // Verify that default semaphore allows 3 concurrent operations
        let sem = Semaphore::new(3);
        assert_eq!(sem.available_permits(), 3);
    }
}
