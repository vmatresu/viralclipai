//! Cinematic signal computation for neural analysis caching.
//!
//! This module extracts shot boundaries (histogram-based) and optionally
//! object detections (YOLOv8) during initial neural analysis for Cinematic tier.
//! Results are cached alongside face detections to avoid recomputation.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};
use vclip_models::{
    CachedObjectDetection, CinematicSignalsCache, ObjectDetectionsCache, ShotBoundaryCache,
};

use crate::error::{WorkerError, WorkerResult};

/// Configuration for cinematic signal computation.
#[derive(Debug, Clone)]
pub struct CinematicSignalOptions {
    /// Enable YOLOv8 object detection (default: false)
    pub enable_object_detection: bool,
}

impl Default for CinematicSignalOptions {
    fn default() -> Self {
        Self {
            enable_object_detection: false,
        }
    }
}

impl CinematicSignalOptions {
    pub fn with_object_detection(mut self, enabled: bool) -> Self {
        self.enable_object_detection = enabled;
        self
    }
}

/// Compute cinematic signals: shot boundaries and optionally object detections.
///
/// This is called during initial neural analysis for Cinematic tier to cache
/// these expensive operations alongside face detections.
///
/// # Arguments
/// * `video_path` - Path to video segment
/// * `start_time` - Start time in seconds
/// * `end_time` - End time in seconds
/// * `options` - Configuration options (object detection enabled/disabled)
pub async fn compute_cinematic_signals(
    video_path: &Path,
    start_time: f64,
    end_time: f64,
    options: CinematicSignalOptions,
) -> WorkerResult<CinematicSignalsCache> {
    use vclip_media::intelligent::cinematic::{CinematicConfig, ShotSignals};

    let config = CinematicConfig::default();

    // Step 1: Extract shot boundaries using histogram analysis (always)
    info!("[CINEMATIC_SIGNALS] Extracting shot boundaries via histogram analysis...");
    let shot_signals = ShotSignals::with_config(
        config.shot_detection_fps,
        config.shot_threshold,
        config.min_shot_duration,
    );

    let shot_boundaries = shot_signals
        .extract(video_path, start_time, end_time)
        .await
        .map_err(|e| WorkerError::job_failed(format!("Shot detection failed: {}", e)))?;

    info!(
        shots = shot_boundaries.len(),
        "[CINEMATIC_SIGNALS] Shot boundaries extracted"
    );

    // Convert to cacheable format
    let cached_shots: Vec<ShotBoundaryCache> = shot_boundaries
        .iter()
        .map(|s| ShotBoundaryCache {
            start_time: s.start_time,
            end_time: s.end_time,
        })
        .collect();

    let mut signals = CinematicSignalsCache::with_shots(
        cached_shots,
        config.shot_threshold,
        config.min_shot_duration,
    );

    // Step 2: Run object detection only if explicitly enabled
    if options.enable_object_detection {
        use vclip_media::detection::{ObjectDetector, ObjectDetectorConfig};
        
        match ObjectDetector::new(ObjectDetectorConfig::default()) {
            Ok(detector) => {
                info!("[CINEMATIC_SIGNALS] Running object detection (YOLOv8)...");

                let object_cache = run_object_detection_for_cache(
                    video_path,
                    &detector,
                    start_time,
                    end_time,
                )
                .await?;

                let total_objects: usize = object_cache.frames.iter().map(|f| f.objects.len()).sum();
                info!(
                    frames = object_cache.frames.len(),
                    objects = total_objects,
                    "[CINEMATIC_SIGNALS] Object detection complete"
                );

                signals = signals.with_object_detections(object_cache);
            }
            Err(e) => {
                debug!(
                    error = %e,
                    "[CINEMATIC_SIGNALS] Object detection skipped (model not available)"
                );
            }
        }
    } else {
        debug!("[CINEMATIC_SIGNALS] Object detection disabled by user preference");
    }

    Ok(signals)
}

/// Run object detection on sampled frames and build cache.
async fn run_object_detection_for_cache(
    video_path: &Path,
    detector: &vclip_media::detection::ObjectDetector,
    start_time: f64,
    end_time: f64,
) -> WorkerResult<ObjectDetectionsCache> {
    let duration = end_time - start_time;
    let sample_fps = 1.6; // Sample every ~5th frame at 8fps detection rate
    let sample_interval = 1.0 / sample_fps;
    let num_samples = (duration * sample_fps).ceil() as usize;

    let mut cache = ObjectDetectionsCache::new(sample_interval, "yolov8n");

    // Create temp dir for frame extraction
    let temp_dir = tempfile::tempdir()
        .map_err(|e| WorkerError::job_failed(format!("Failed to create temp dir: {}", e)))?;

    for i in 0..num_samples {
        let time = start_time + i as f64 * sample_interval;
        let frame_path = temp_dir.path().join(format!("frame_{:06}.jpg", i));

        // Extract single frame using FFmpeg
        let extract_result = Command::new("ffmpeg")
            .args([
                "-ss",
                &format!("{:.3}", time),
                "-i",
                video_path.to_str().unwrap_or(""),
                "-vframes",
                "1",
                "-q:v",
                "2",
                "-y",
                frame_path.to_str().unwrap_or(""),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await;

        if extract_result.is_err() || !frame_path.exists() {
            cache.add_frame(time, vec![]);
            continue;
        }

        // Load frame as image
        let frame_data = match tokio::fs::read(&frame_path).await {
            Ok(data) => data,
            Err(_) => {
                cache.add_frame(time, vec![]);
                continue;
            }
        };

        let img = match image::load_from_memory(&frame_data) {
            Ok(img) => img,
            Err(_) => {
                cache.add_frame(time, vec![]);
                continue;
            }
        };

        // Run object detection
        let detections = match detector.detect_image(&img) {
            Ok(dets) => dets,
            Err(e) => {
                debug!(frame = i, error = %e, "Object detection failed for frame");
                cache.add_frame(time, vec![]);
                continue;
            }
        };

        // Convert to cached format - ObjectDetector already returns normalized (0-1) coords
        let cached_objects: Vec<CachedObjectDetection> = detections
            .iter()
            .map(|obj| CachedObjectDetection {
                x: obj.x,
                y: obj.y,
                width: obj.width,
                height: obj.height,
                class_id: obj.class_id,
                confidence: obj.confidence,
            })
            .collect();

        cache.add_frame(time, cached_objects);

        // Clean up frame
        let _ = tokio::fs::remove_file(&frame_path).await;
    }

    Ok(cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_default_object_detection_off() {
        let options = CinematicSignalOptions::default();
        assert!(!options.enable_object_detection);
    }

    #[test]
    fn test_options_builder() {
        let options = CinematicSignalOptions::default().with_object_detection(true);
        assert!(options.enable_object_detection);
    }
}
