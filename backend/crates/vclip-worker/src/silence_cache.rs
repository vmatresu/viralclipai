//! Silence removal service with R2 caching.
//!
//! This module provides cached silence removal for video segments.
//! It leverages Silero VAD for speech detection and FFmpeg stream copy
//! for efficient segment manipulation without re-encoding.
//!
//! # Caching Strategy
//!
//! Silence-removed segments are cached in R2 to avoid reprocessing:
//! 1. Check R2 cache first using `silence_removed_r2_key`
//! 2. Check local filesystem (from current session)
//! 3. If not cached, analyze with VAD and apply removal
//! 4. Upload result to R2 for future requests
//!
//! # Architecture
//!
//! This follows the Single Responsibility Principle - this module ONLY handles
//! silence removal caching. The actual silence detection and removal algorithms
//! live in `vclip-media::silence_removal`.

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use vclip_media::silence_removal::{
    analyze_audio_segments, apply_silence_removal, should_apply_silence_removal,
    SilenceRemovalConfig,
};
use vclip_models::JobId;

use crate::error::{WorkerError, WorkerResult};
use crate::processor::EnhancedProcessingContext;
use crate::raw_segment_cache::silence_removed_r2_key;

/// Result of applying silence removal to a segment.
#[derive(Debug)]
pub enum SilenceRemovalResult {
    /// Silence was removed, new file created at path
    Applied(PathBuf),
    /// No significant silence detected, use original segment
    NotNeeded,
    /// Used cached version from R2
    CacheHit(PathBuf),
    /// Used existing local file from current session
    LocalHit(PathBuf),
}

impl SilenceRemovalResult {
    /// Get the path to use for further processing.
    /// Returns `Some(path)` for Applied/CacheHit/LocalHit, `None` for NotNeeded.
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            SilenceRemovalResult::Applied(p) |
            SilenceRemovalResult::CacheHit(p) |
            SilenceRemovalResult::LocalHit(p) => Some(p),
            SilenceRemovalResult::NotNeeded => None,
        }
    }

    /// Convert to Option<PathBuf>, consuming self.
    pub fn into_path(self) -> Option<PathBuf> {
        match self {
            SilenceRemovalResult::Applied(p) |
            SilenceRemovalResult::CacheHit(p) |
            SilenceRemovalResult::LocalHit(p) => Some(p),
            SilenceRemovalResult::NotNeeded => None,
        }
    }
}

/// Configuration for the silence removal service.
#[derive(Debug, Clone)]
pub struct SilenceServiceConfig {
    /// VAD configuration
    pub vad_config: SilenceRemovalConfig,
    /// Whether to upload results to R2 cache
    pub enable_r2_cache: bool,
}

impl Default for SilenceServiceConfig {
    fn default() -> Self {
        Self {
            vad_config: SilenceRemovalConfig::default(),
            // Disabled: stream copy is fast enough, no need to cache silence-removed clips
            // Also fixes issue where cached sizes didn't match due to keyframe alignment
            enable_r2_cache: false,
        }
    }
}

/// Service for applying silence removal with caching.
///
/// This service encapsulates the silence removal workflow including:
/// - R2 cache checking and uploading
/// - Local file caching within a session
/// - VAD analysis and segment processing
pub struct SilenceRemovalService<'a> {
    ctx: &'a EnhancedProcessingContext,
    config: SilenceServiceConfig,
}

impl<'a> SilenceRemovalService<'a> {
    /// Create a new silence removal service.
    pub fn new(ctx: &'a EnhancedProcessingContext) -> Self {
        Self {
            ctx,
            config: SilenceServiceConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(ctx: &'a EnhancedProcessingContext, config: SilenceServiceConfig) -> Self {
        Self { ctx, config }
    }

    /// Apply silence removal to a raw segment with R2 caching.
    ///
    /// # Arguments
    /// * `raw_segment` - Path to the input video segment
    /// * `scene_id` - Scene identifier for caching
    /// * `job_id` - Job ID for progress reporting
    /// * `user_id` - User ID for R2 key generation
    /// * `video_id` - Video ID for R2 key generation
    ///
    /// # Returns
    /// * `Ok(SilenceRemovalResult)` - Result indicating what happened
    /// * `Err` - If a critical error occurred
    pub async fn apply_cached(
        &self,
        raw_segment: &Path,
        scene_id: u32,
        job_id: &JobId,
        user_id: &str,
        video_id: &str,
    ) -> WorkerResult<SilenceRemovalResult> {
        let output_path = self.output_path(raw_segment, scene_id);
        let r2_key = silence_removed_r2_key(user_id, video_id, scene_id);

        // 1. Check R2 cache
        if let Some(result) = self.try_r2_cache(&r2_key, &output_path, scene_id).await {
            return Ok(result);
        }

        // 2. Check local cache
        if let Some(result) = self.try_local_cache(&output_path, scene_id).await {
            return Ok(result);
        }

        // 3. Analyze and apply
        let result = self
            .analyze_and_apply(raw_segment, &output_path, scene_id, job_id)
            .await?;

        // 4. Upload to R2 if applied and caching enabled
        if let SilenceRemovalResult::Applied(ref path) = result {
            if self.config.enable_r2_cache {
                self.upload_to_r2(path, &r2_key, scene_id).await;
            }
        }

        Ok(result)
    }

    /// Generate output path for silence-removed segment.
    fn output_path(&self, raw_segment: &Path, scene_id: u32) -> PathBuf {
        let parent = raw_segment.parent().unwrap_or(Path::new("."));
        parent.join(format!("raw_{}_silence_removed.mp4", scene_id))
    }

    /// Try to use R2 cached version.
    async fn try_r2_cache(
        &self,
        r2_key: &str,
        output_path: &Path,
        scene_id: u32,
    ) -> Option<SilenceRemovalResult> {
        if !self.ctx.raw_cache.check_raw_exists(r2_key).await {
            return None;
        }

        match self.ctx.storage.download_file(r2_key, output_path).await {
            Ok(_) => {
                info!(
                    scene_id = scene_id,
                    r2_key = %r2_key,
                    "Using cached silence-removed segment from R2"
                );
                Some(SilenceRemovalResult::CacheHit(output_path.to_path_buf()))
            }
            Err(e) => {
                debug!(
                    scene_id = scene_id,
                    error = %e,
                    "Failed to download cached silence-removed segment, will regenerate"
                );
                None
            }
        }
    }

    /// Try to use local cached version.
    async fn try_local_cache(
        &self,
        output_path: &Path,
        scene_id: u32,
    ) -> Option<SilenceRemovalResult> {
        if !output_path.exists() {
            return None;
        }

        match tokio::fs::metadata(output_path).await {
            Ok(meta) if meta.len() > 0 => {
                info!(
                    scene_id = scene_id,
                    path = ?output_path,
                    "Using existing local silence-removed segment"
                );
                Some(SilenceRemovalResult::LocalHit(output_path.to_path_buf()))
            }
            _ => None,
        }
    }

    /// Analyze audio and apply silence removal if needed.
    async fn analyze_and_apply(
        &self,
        raw_segment: &Path,
        output_path: &Path,
        scene_id: u32,
        job_id: &JobId,
    ) -> WorkerResult<SilenceRemovalResult> {
        debug!(
            scene_id = scene_id,
            segment = ?raw_segment,
            "Analyzing audio for silence detection"
        );

        // Analyze audio segments
        let segments = match analyze_audio_segments(raw_segment, self.config.vad_config.clone()).await {
            Ok(s) => s,
            Err(e) => {
                debug!(
                    scene_id = scene_id,
                    error = %e,
                    "Silence analysis failed (may be too short or no audio)"
                );
                return Ok(SilenceRemovalResult::NotNeeded);
            }
        };

        // Check if silence removal should be applied
        if !should_apply_silence_removal(&segments, &self.config.vad_config) {
            return Ok(SilenceRemovalResult::NotNeeded);
        }

        // Log progress
        self.ctx
            .progress
            .log(job_id, format!("Removing silent parts from scene {}...", scene_id))
            .await
            .ok();

        // Apply silence removal
        apply_silence_removal(raw_segment, output_path, &segments)
            .await
            .map_err(|e| WorkerError::job_failed(&format!("Silence removal failed: {}", e)))?;

        // Verify output exists
        if !output_path.exists() {
            return Err(WorkerError::job_failed("Silence removal output file not created"));
        }

        // Log statistics
        self.log_statistics(raw_segment, output_path, scene_id).await;

        Ok(SilenceRemovalResult::Applied(output_path.to_path_buf()))
    }

    /// Log file size statistics after silence removal.
    async fn log_statistics(&self, original: &Path, processed: &Path, scene_id: u32) {
        let original_size = tokio::fs::metadata(original)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let new_size = tokio::fs::metadata(processed)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let reduction_pct = if original_size > 0 {
            ((original_size as f64 - new_size as f64) / original_size as f64) * 100.0
        } else {
            0.0
        };

        info!(
            scene_id = scene_id,
            original_size_mb = original_size as f64 / 1_048_576.0,
            new_size_mb = new_size as f64 / 1_048_576.0,
            reduction_pct = format!("{:.1}%", reduction_pct),
            "Silence removal completed"
        );
    }

    /// Upload to R2 cache (fire-and-forget, non-blocking for caller).
    async fn upload_to_r2(&self, path: &Path, r2_key: &str, scene_id: u32) {
        if let Err(e) = self.ctx.raw_cache.upload_raw_segment(path, r2_key).await {
            warn!(
                scene_id = scene_id,
                error = %e,
                "Failed to upload silence-removed segment to R2 (non-critical)"
            );
        } else {
            info!(
                scene_id = scene_id,
                r2_key = %r2_key,
                "Uploaded silence-removed segment to R2 cache"
            );
        }
    }
}

/// Convenience function for applying silence removal with caching.
///
/// This is a thin wrapper around `SilenceRemovalService::apply_cached` for
/// backwards compatibility and simpler call sites.
pub async fn apply_silence_removal_cached(
    ctx: &EnhancedProcessingContext,
    raw_segment: &Path,
    scene_id: u32,
    job_id: &JobId,
    user_id: &str,
    video_id: &str,
) -> WorkerResult<Option<PathBuf>> {
    let service = SilenceRemovalService::new(ctx);
    let result = service
        .apply_cached(raw_segment, scene_id, job_id, user_id, video_id)
        .await?;
    Ok(result.into_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_removal_result_path() {
        let path = PathBuf::from("/tmp/test.mp4");
        
        let applied = SilenceRemovalResult::Applied(path.clone());
        assert_eq!(applied.path(), Some(&path));
        
        let cache_hit = SilenceRemovalResult::CacheHit(path.clone());
        assert_eq!(cache_hit.path(), Some(&path));
        
        let local_hit = SilenceRemovalResult::LocalHit(path.clone());
        assert_eq!(local_hit.path(), Some(&path));
        
        let not_needed = SilenceRemovalResult::NotNeeded;
        assert_eq!(not_needed.path(), None);
    }

    #[test]
    fn test_silence_removal_result_into_path() {
        let path = PathBuf::from("/tmp/test.mp4");
        
        assert_eq!(
            SilenceRemovalResult::Applied(path.clone()).into_path(),
            Some(path.clone())
        );
        assert_eq!(
            SilenceRemovalResult::NotNeeded.into_path(),
            None
        );
    }

    #[test]
    fn test_config_default() {
        let config = SilenceServiceConfig::default();
        // R2 caching disabled by default - stream copy is fast enough
        assert!(!config.enable_r2_cache);
    }
}
