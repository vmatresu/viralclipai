//! Cinematic processor orchestrating the AutoAI-inspired pipeline.
//!
//! This module provides the main entry point for the Cinematic detection tier,
//! which produces professional-quality smooth camera motion through:
//!
//! 1. Shot detection (histogram-based scene boundaries)
//! 2. Face detection (reuses YuNet)
//! 3. IoU tracking (reuses existing tracker)
//! 4. Signal fusion (weighted saliency)
//! 5. Camera mode analysis per shot (Stationary/Panning/Tracking)
//! 6. Adaptive zoom based on subject count and activity
//! 7. Polynomial trajectory optimization per shot
//! 8. Crop planning and FFmpeg rendering

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};
use vclip_models::{
    CachedObjectDetection, DetectionTier, EncodingConfig, ObjectDetectionsCache,
    SceneNeuralAnalysis,
};

use super::camera_mode::CameraModeAnalyzer;
use super::config::CinematicConfig;
use super::scene_window::SceneWindowAnalyzer;
use super::signals::{FaceSignals, ShotBoundary, ShotSignals};
use super::trajectory::TrajectoryOptimizer;
use super::zoom::AdaptiveZoom;
use crate::clip::extract_segment;
use crate::detection::{ObjectDetector, ObjectDetectorConfig, ObjectDetection, PipelineBuilder};
use crate::error::MediaResult;
use crate::intelligent::config::IntelligentCropConfig;
use crate::intelligent::crop_planner::CropPlanner;
use crate::intelligent::detection_adapter::get_detections;
use crate::intelligent::models::{AspectRatio, BoundingBox, CameraKeyframe, Detection};
use crate::intelligent::single_pass_renderer::SinglePassRenderer;
use crate::probe::probe_video;
use crate::thumbnail::generate_thumbnail;

/// Cinematic processor for AutoAI-inspired smooth camera motion.
///
/// Orchestrates the full pipeline from face detection to rendering,
/// producing professional-quality video with smooth camera paths.
pub struct CinematicProcessor {
    /// Cinematic-specific configuration
    config: CinematicConfig,
    /// Base intelligent crop config (for reused components)
    base_config: IntelligentCropConfig,
    /// Object detector for YOLOv8 inference (optional - requires model)
    object_detector: Option<Arc<ObjectDetector>>,
}

impl CinematicProcessor {
    /// Create a new cinematic processor with default configuration.
    pub fn new() -> Self {
        let config = CinematicConfig::default();
        let object_detector = Self::try_load_object_detector(&config);
        
        Self {
            config,
            base_config: IntelligentCropConfig::for_tier(DetectionTier::Cinematic),
            object_detector,
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: CinematicConfig) -> Self {
        let object_detector = Self::try_load_object_detector(&config);
        
        Self {
            config,
            base_config: IntelligentCropConfig::for_tier(DetectionTier::Cinematic),
            object_detector,
        }
    }

    /// Create optimized for podcasts/interviews.
    pub fn for_podcast() -> Self {
        let config = CinematicConfig::podcast();
        let object_detector = Self::try_load_object_detector(&config);
        
        Self {
            config,
            base_config: IntelligentCropConfig::for_tier(DetectionTier::Cinematic),
            object_detector,
        }
    }
    
    /// Try to load the object detector model.
    fn try_load_object_detector(config: &CinematicConfig) -> Option<Arc<ObjectDetector>> {
        if !config.enable_object_detection {
            info!("[CINEMATIC] Object detection disabled in config");
            return None;
        }

        match ObjectDetector::new(ObjectDetectorConfig::default()) {
            Ok(detector) => {
                info!("[CINEMATIC] Object detection enabled (YOLOv8)");
                Some(Arc::new(detector))
            }
            Err(e) => {
                info!("[CINEMATIC] Object detection not available: {}", e);
                None
            }
        }
    }
    
    /// Check if object detection is available.
    pub fn has_object_detection(&self) -> bool {
        self.object_detector.is_some()
    }

    /// Process a pre-cut video segment with cinematic smoothing.
    ///
    /// # Arguments
    /// * `segment` - Pre-extracted segment (stream copy from source)
    /// * `output` - Final output path
    /// * `encoding` - Encoding configuration
    pub async fn process<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
    ) -> MediaResult<()> {
        self.process_with_cache(segment, output, encoding, None).await
    }

    /// Process with optional cached neural analysis.
    ///
    /// This is the main entry point that allows skipping expensive ML inference
    /// when cached detections are available.
    pub async fn process_with_cache<P: AsRef<Path>>(
        &self,
        segment: P,
        output: P,
        encoding: &EncodingConfig,
        cached_analysis: Option<&SceneNeuralAnalysis>,
    ) -> MediaResult<()> {
        let segment = segment.as_ref();
        let output = output.as_ref();
        let pipeline_start = std::time::Instant::now();

        info!("[CINEMATIC] ========================================");
        info!("[CINEMATIC] START: {:?}", segment);
        info!("[CINEMATIC] Cached analysis: {}", cached_analysis.is_some());

        // Step 1: Get video metadata
        let step_start = std::time::Instant::now();
        info!("[CINEMATIC] Step 1/6: Probing video metadata...");

        let video_info = probe_video(segment).await?;
        let width = video_info.width;
        let height = video_info.height;
        let fps = video_info.fps;
        let duration = video_info.duration;

        info!(
            "[CINEMATIC] Step 1/7 DONE in {:.2}s - {}x{} @ {:.2}fps, {:.2}s",
            step_start.elapsed().as_secs_f64(),
            width, height, fps, duration
        );

        let start_time = 0.0;
        let end_time = duration;

        // Step 2: Shot detection (cached or fresh)
        let step_start = std::time::Instant::now();
        let shots = if self.config.enable_shot_detection {
            info!("[CINEMATIC] Step 2/7: Detecting shots...");
            let shots = self.detect_shots_or_use_cached(
                segment, cached_analysis, start_time, end_time
            ).await?;
            info!(
                "[CINEMATIC] Step 2/7 DONE in {:.2}s - {} shots detected",
                step_start.elapsed().as_secs_f64(), shots.len()
            );
            shots
        } else {
            info!("[CINEMATIC] Step 2/7: Shot detection disabled, using single shot");
            vec![ShotBoundary::new(start_time, end_time)]
        };

        // Step 3: Get face detections (cached or fresh)
        let step_start = std::time::Instant::now();
        info!(
            "[CINEMATIC] Step 3/7: Getting detections (cached: {})...",
            cached_analysis.is_some()
        );

        let mut detections = get_detections(
            cached_analysis,
            segment,
            DetectionTier::Cinematic,
            start_time,
            end_time,
            width,
            height,
            fps,
        )
        .await?;

        // Filter out tiny faces (common in screen recordings with a corner webcam).
        // This also prevents cached analysis from dominating framing with unusably-small faces.
        if width > 0 && height > 0 {
            let frame_area = (width as f64) * (height as f64);
            for frame in &mut detections {
                frame.retain(|d| (d.bbox.area() / frame_area) >= self.config.min_face_size);
            }
        }

        // Derive actual sampling rate from detections length.
        // This avoids mismatches if detection sampling differs from config.
        let duration = end_time - start_time;
        let sample_interval = if detections.len() > 1 {
            duration / (detections.len() - 1) as f64
        } else {
            1.0 / self.config.detection_fps.max(1e-3)
        };
        let detection_fps = if sample_interval > 0.0 {
            1.0 / sample_interval
        } else {
            self.config.detection_fps.max(1e-3)
        };

        let total_detections: usize = detections.iter().map(|d| d.len()).sum();
        info!(
            "[CINEMATIC] Step 3/8 DONE in {:.2}s - {} face detections in {} frames",
            step_start.elapsed().as_secs_f64(),
            total_detections,
            detections.len()
        );

        // Step 3.5: Get object detections (cached or fresh)
        let step_start = std::time::Instant::now();
        let (object_detections, object_cache_for_save): (Vec<Vec<ObjectDetection>>, Option<ObjectDetectionsCache>) = 
            if let Some(cached) = self.try_get_cached_object_detections(cached_analysis, width, height) {
                let total_objects: usize = cached.iter().map(|d| d.len()).sum();
                info!(
                    "[CINEMATIC] Step 3.5/8: Using cached object detections ({} objects in {} frames)",
                    total_objects, cached.len()
                );
                (cached, None) // Already cached, no need to save
            } else if self.object_detector.is_some() {
                info!("[CINEMATIC] Step 3.5/8: Running object detection (YOLOv8)...");
                let obj_dets = self
                    .run_object_detection(segment, &detections, start_time, sample_interval)
                    .await?;
                let total_objects: usize = obj_dets.iter().map(|d| d.len()).sum();
                info!(
                    "[CINEMATIC] Step 3.5/8 DONE in {:.2}s - {} objects in {} frames",
                    step_start.elapsed().as_secs_f64(),
                    total_objects,
                    obj_dets.len()
                );
                // Build cache for saving
                let cache = self.build_object_detections_cache(&obj_dets, sample_interval, start_time, width, height);
                (obj_dets, Some(cache))
            } else {
                info!("[CINEMATIC] Step 3.5/8: Object detection skipped (no model)");
                (vec![vec![]; detections.len()], None)
            };
        
        // Note: object_cache_for_save can be used to update the cached_analysis
        // This is typically done by the caller (neural_cache.rs) after processing
        if object_cache_for_save.is_some() {
            info!("[CINEMATIC] Object detections ready for caching ({} frames)", object_detections.len());
        }

        // If we have no usable faces, compute a motion-aware fallback once.
        let mut motion_detections: Option<Vec<Vec<Detection>>> = None;
        if detections.iter().all(|f| f.is_empty()) {
            match PipelineBuilder::for_tier(DetectionTier::MotionAware).build() {
                Ok(pipeline) => {
                    if let Ok(result) = pipeline.analyze(segment, start_time, end_time).await {
                        let mut frames: Vec<Vec<Detection>> =
                            result.frames.into_iter().map(|f| f.faces).collect();
                        frames.resize_with(detections.len(), Vec::new);
                        if frames.len() > detections.len() {
                            frames.truncate(detections.len());
                        }
                        motion_detections = Some(frames);
                    }
                }
                Err(_) => {}
            }
        }

        // Step 4: Build activity scores from mouth openness
        let step_start = std::time::Instant::now();
        info!("[CINEMATIC] Step 4/8: Computing activity scores...");

        let activities = self.compute_activity_scores(&detections);
        info!(
            "[CINEMATIC] Step 4/8 DONE in {:.2}s - {} tracked subjects",
            step_start.elapsed().as_secs_f64(),
            activities.len()
        );

        // Step 5: Per-shot processing (camera mode + keyframes + trajectory)
        let step_start = std::time::Instant::now();
        info!("[CINEMATIC] Step 5/8: Per-shot camera analysis ({} shots)...", shots.len());

        let _face_signals = FaceSignals::with_weights(
            self.config.face_weight,
            self.config.activity_boost,
        );
        let _mode_analyzer = CameraModeAnalyzer::new(&self.config);
        let trajectory_optimizer = TrajectoryOptimizer::new(&self.config);
        let mut adaptive_zoom = AdaptiveZoom::new(&self.config, width, height);

        let mut all_smoothed_keyframes = Vec::new();
        for (shot_idx, shot) in shots.iter().enumerate() {
            // Filter face detections for this shot
            let shot_face_detections = self.filter_detections_for_shot(
                &detections, shot, sample_interval, start_time
            );
            
            // Filter object detections for this shot (same indices)
            let shot_frame_start = ((shot.start_time - start_time) / sample_interval).floor() as usize;
            let shot_frame_end = ((shot.end_time - start_time) / sample_interval).ceil() as usize;
            let shot_object_detections: Vec<Vec<ObjectDetection>> = object_detections
                .get(shot_frame_start..shot_frame_end.min(object_detections.len()))
                .map(|s| s.to_vec())
                .unwrap_or_default();

            let shot_motion_detections: Option<Vec<Vec<Detection>>> = motion_detections.as_ref().map(|md| {
                md.get(shot_frame_start..shot_frame_end.min(md.len()))
                    .map(|s| s.to_vec())
                    .unwrap_or_default()
            });

            if shot_face_detections.is_empty() {
                continue;
            }
            
            // Compute fused saliency signals from faces + objects
            let has_objects = shot_object_detections.iter().any(|o| !o.is_empty());
            if has_objects {
                info!("[CINEMATIC]   Shot {}: Fusing {} face frames + {} object frames", 
                    shot_idx + 1, shot_face_detections.len(), shot_object_detections.len());
            }
            
            // Build per-frame detections used by the window analyzer.
            // We inject object detections (and motion fallback) as pseudo-detections so
            // screen recordings without usable faces still get meaningful camera motion.
            let mut shot_detections: Vec<Vec<Detection>> = Vec::with_capacity(shot_face_detections.len());
            for (i, frame_faces) in shot_face_detections.into_iter().enumerate() {
                let mut frame_dets = frame_faces;

                // Only inject object pseudo-detections if there are no usable faces.
                // This avoids widening the crop when we already have a good face signal.
                if frame_dets.is_empty() {
                    if let Some(frame_objects) = shot_object_detections.get(i) {
                        let mut ranked: Vec<(f64, usize, BoundingBox, f64)> = frame_objects
                            .iter()
                            .enumerate()
                            .filter_map(|(j, obj)| {
                                let person_multiplier = if obj.is_person() { 1.5 } else { 1.0 };
                                let score = (obj.confidence as f64)
                                    * self.config.object_weight
                                    * person_multiplier;
                                if score <= 0.0 {
                                    return None;
                                }

                                let bbox = BoundingBox::new(
                                    (obj.x as f64) * (width as f64),
                                    (obj.y as f64) * (height as f64),
                                    (obj.width as f64) * (width as f64),
                                    (obj.height as f64) * (height as f64),
                                )
                                .clamp(width, height);
                                let rank = score * bbox.area();
                                Some((rank, j, bbox, score))
                            })
                            .collect();

                        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
                        for (_rank, j, bbox, score) in ranked.into_iter().take(3) {
                            let time = shot.start_time + i as f64 * sample_interval;
                            frame_dets
                                .push(Detection::new(time, bbox, score, 1_000_000 + j as u32));
                        }
                    }
                }

                if frame_dets.is_empty() {
                    if let Some(md) = &shot_motion_detections {
                        if let Some(motion_frame) = md.get(i) {
                            frame_dets.extend(motion_frame.iter().cloned());
                        }
                    }
                }

                shot_detections.push(frame_dets);
            }

            // Use SceneWindowAnalyzer for multi-frame temporal smoothing
            let mut window_analyzer = SceneWindowAnalyzer::new(
                self.config.lookahead_frames,
                width,
                height,
                self.config.stationary_threshold,
                self.config.panning_threshold,
            );

            // Pre-analyze the shot to get window-smoothed focus points
            let window_analyses = window_analyzer.analyze_shot(
                &shot_detections,
                sample_interval,
                shot.start_time,
            );

            // Create keyframes from window analyses (temporally smoothed)
            let shot_keyframes: Vec<CameraKeyframe> = window_analyses
                .iter()
                .map(|wa| wa.to_keyframe(wa.median_cx / width as f64 * sample_interval + shot.start_time))
                .enumerate()
                .map(|(i, mut kf)| {
                    // Use proper time from the window analysis loop
                    kf.time = shot.start_time + i as f64 * sample_interval;
                    kf
                })
                .collect();

            if shot_keyframes.is_empty() {
                continue;
            }

            // Use window-derived camera mode (most recent window's mode)
            let camera_mode = window_analyses
                .last()
                .map(|wa| wa.camera_mode)
                .unwrap_or(super::camera_mode::CameraMode::Stationary);

            // Apply adaptive zoom
            let zoomed = adaptive_zoom.apply_to_keyframes(
                &shot_keyframes,
                &shot_detections,
                &activities,
                shot.start_time,
                detection_fps,
            );

            // Apply trajectory optimization
            let smoothed = trajectory_optimizer.optimize(&zoomed, camera_mode);

            info!(
                "[CINEMATIC]   Shot {}: {:.2}s-{:.2}s, mode={}, {} keyframes (window smoothed)",
                shot_idx + 1, shot.start_time, shot.end_time,
                camera_mode.description(), smoothed.len()
            );

            all_smoothed_keyframes.extend(smoothed);
        }

        info!(
            "[CINEMATIC] Step 5/8 DONE in {:.2}s - {} total keyframes",
            step_start.elapsed().as_secs_f64(),
            all_smoothed_keyframes.len()
        );

        // Step 6: Compute crop windows and render
        let step_start = std::time::Instant::now();
        info!("[CINEMATIC] Step 6/8: Computing crops and rendering...");

        let target_aspect = AspectRatio::new(9, 16);
        let planner = CropPlanner::new(self.base_config.clone(), width, height);
        let crop_windows = planner.compute_crop_windows(&all_smoothed_keyframes, &target_aspect);

        info!(
            "[CINEMATIC] Generated {} crop windows",
            crop_windows.len()
        );

        // Render with single pass
        let renderer = SinglePassRenderer::new(self.base_config.clone());
        renderer
            .render_full(segment, output, &crop_windows, encoding)
            .await?;

        info!(
            "[CINEMATIC] Step 6/8 DONE in {:.2}s",
            step_start.elapsed().as_secs_f64()
        );

        // Step 7: Generate thumbnail
        let thumb_path = output.with_extension("jpg");
        if let Err(e) = generate_thumbnail(output, &thumb_path).await {
            tracing::warn!("[CINEMATIC] Failed to generate thumbnail: {}", e);
        }

        let file_size = tokio::fs::metadata(output)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        info!("[CINEMATIC] ========================================");
        info!(
            "[CINEMATIC] COMPLETE in {:.2}s - {:.2} MB",
            pipeline_start.elapsed().as_secs_f64(),
            file_size as f64 / 1_000_000.0
        );

        Ok(())
    }

    /// Compute activity scores per track from mouth openness.
    ///
    /// Higher scores indicate more speaking activity.
    fn compute_activity_scores(&self, detections: &[Vec<Detection>]) -> HashMap<u32, f64> {
        let mut track_scores: HashMap<u32, Vec<f64>> = HashMap::new();

        for frame_dets in detections {
            for det in frame_dets {
                let score = det.mouth_openness.unwrap_or(0.0);
                track_scores
                    .entry(det.track_id)
                    .or_default()
                    .push(score);
            }
        }

        // Compute average score per track
        track_scores
            .into_iter()
            .map(|(track_id, scores)| {
                let avg = if scores.is_empty() {
                    0.0
                } else {
                    scores.iter().sum::<f64>() / scores.len() as f64
                };
                (track_id, avg)
            })
            .collect()
    }

    /// Detect shots or use cached shot boundaries.
    async fn detect_shots_or_use_cached(
        &self,
        segment: &Path,
        cached_analysis: Option<&SceneNeuralAnalysis>,
        start_time: f64,
        end_time: f64,
    ) -> MediaResult<Vec<ShotBoundary>> {
        // Check for cached shot boundaries
        if let Some(analysis) = cached_analysis {
            if let Some(ref signals) = analysis.cinematic_signals {
                if signals.is_valid(self.config.shot_threshold, self.config.min_shot_duration) {
                    info!("[CINEMATIC] Using cached shot boundaries ({} shots)", signals.shots.len());
                    return Ok(signals.shots.iter()
                        .map(|s| ShotBoundary::new(s.start_time, s.end_time))
                        .collect());
                }
            }
        }

        // Run shot detection
        let shot_signals = ShotSignals::with_config(
            self.config.shot_detection_fps,
            self.config.shot_threshold,
            self.config.min_shot_duration,
        );

        shot_signals.extract(segment, start_time, end_time).await
    }

    /// Filter detections to only include frames within a shot's time range.
    fn filter_detections_for_shot(
        &self,
        detections: &[Vec<Detection>],
        shot: &ShotBoundary,
        sample_interval: f64,
        base_start_time: f64,
    ) -> Vec<Vec<Detection>> {
        detections
            .iter()
            .enumerate()
            .filter_map(|(i, frame_dets)| {
                let frame_time = base_start_time + i as f64 * sample_interval;
                if frame_time >= shot.start_time && frame_time < shot.end_time {
                    Some(frame_dets.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Try to get cached object detections from the neural analysis.
    ///
    /// Returns None if no valid cache exists or if the cache model version doesn't match.
    fn try_get_cached_object_detections(
        &self,
        cached_analysis: Option<&SceneNeuralAnalysis>,
        frame_width: u32,
        frame_height: u32,
    ) -> Option<Vec<Vec<ObjectDetection>>> {
        let analysis = cached_analysis?;
        let signals = analysis.cinematic_signals.as_ref()?;
        let obj_cache = signals.object_detections.as_ref()?;

        // Check model version matches current detector
        const EXPECTED_MODEL: &str = "yolov8n";
        if obj_cache.model_version != EXPECTED_MODEL {
            info!(
                "[CINEMATIC] Cache model mismatch: {} vs expected {}",
                obj_cache.model_version, EXPECTED_MODEL
            );
            return None;
        }

        if obj_cache.frames.is_empty() {
            return None;
        }

        // Convert cached detections back to ObjectDetection format
        let fw = frame_width as f32;
        let fh = frame_height as f32;

        let detections: Vec<Vec<ObjectDetection>> = obj_cache
            .frames
            .iter()
            .map(|frame| {
                frame
                    .objects
                    .iter()
                    .map(|cached| ObjectDetection {
                        x: cached.x * fw,
                        y: cached.y * fh,
                        width: cached.width * fw,
                        height: cached.height * fh,
                        class_id: cached.class_id,
                        confidence: cached.confidence,
                    })
                    .collect()
            })
            .collect();

        Some(detections)
    }

    /// Build an ObjectDetectionsCache from fresh object detections.
    ///
    /// Converts pixel coordinates to normalized coordinates for storage.
    fn build_object_detections_cache(
        &self,
        detections: &[Vec<ObjectDetection>],
        sample_interval: f64,
        start_time: f64,
        frame_width: u32,
        frame_height: u32,
    ) -> ObjectDetectionsCache {
        let fw = frame_width as f32;
        let fh = frame_height as f32;

        let mut cache = ObjectDetectionsCache::new(sample_interval, "yolov8n");

        for (i, frame_dets) in detections.iter().enumerate() {
            let time = start_time + i as f64 * sample_interval;
            let cached_objs: Vec<CachedObjectDetection> = frame_dets
                .iter()
                .map(|obj| CachedObjectDetection {
                    x: obj.x / fw,
                    y: obj.y / fh,
                    width: obj.width / fw,
                    height: obj.height / fh,
                    class_id: obj.class_id,
                    confidence: obj.confidence,
                })
                .collect();
            cache.add_frame(time, cached_objs);
        }

        cache
    }
    
    /// Run object detection on sampled frames using YOLOv8.
    ///
    /// Samples frames at the configured detection FPS and runs inference.
    /// Returns per-frame object detections parallel to the face detections.
    async fn run_object_detection(
        &self,
        segment: &Path,
        face_detections: &[Vec<Detection>],
        base_start_time: f64,
        sample_interval: f64,
    ) -> MediaResult<Vec<Vec<ObjectDetection>>> {
        let detector = match &self.object_detector {
            Some(d) => d,
            None => return Ok(vec![vec![]; face_detections.len()]),
        };
        
        // Create temp dir for frame extraction
        let temp_dir = tempfile::tempdir()
            .map_err(|e| crate::error::MediaError::internal(format!("Failed to create temp dir: {}", e)))?;
        
        let frame_count = face_detections.len();
        
        // Sample every Nth frame to match face detection sampling
        // For efficiency, we sample every 5th detection frame (approx 1.6 fps if detection_fps=8)
        let sample_stride = 5;
        let mut object_detections: Vec<Vec<ObjectDetection>> = vec![vec![]; frame_count];
        
        for (i, _) in face_detections.iter().enumerate().step_by(sample_stride) {
            let time = base_start_time + i as f64 * sample_interval;
            let frame_path = temp_dir.path().join(format!("frame_{:06}.jpg", i));
            
            // Extract single frame using FFmpeg
            let extract_result = tokio::process::Command::new("ffmpeg")
                .args([
                    "-ss", &format!("{:.3}", time),
                    "-i", segment.to_str().unwrap_or(""),
                    "-vframes", "1",
                    "-q:v", "2",
                    "-y",
                    frame_path.to_str().unwrap_or(""),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .await;
            
            if extract_result.is_err() || !frame_path.exists() {
                continue;
            }
            
            // Load frame as image
            let frame_data = match tokio::fs::read(&frame_path).await {
                Ok(data) => data,
                Err(_) => continue,
            };
            
            let img = match image::load_from_memory(&frame_data) {
                Ok(img) => img,
                Err(_) => continue,
            };
            
            // Run object detection
            match detector.detect_image(&img) {
                Ok(detections) => {
                    // Store detections for this frame and propagate to adjacent frames
                    for j in i..(i + sample_stride).min(frame_count) {
                        object_detections[j] = detections.clone();
                    }
                }
                Err(e) => {
                    warn!("[CINEMATIC] Object detection failed for frame {}: {}", i, e);
                }
            }
            
            // Clean up frame
            let _ = tokio::fs::remove_file(&frame_path).await;
        }
        
        Ok(object_detections)
    }

    /// Create raw keyframes using signal fusion for weighted face targeting.
    #[cfg(test)]
    fn create_raw_keyframes_with_fusion(
        &self,
        detections: &[Vec<Detection>],
        face_signals: &FaceSignals,
        width: u32,
        height: u32,
        detection_fps: f64,
        start_time: f64,
    ) -> Vec<CameraKeyframe> {
        let sample_interval = 1.0 / detection_fps.max(1e-3);
        let mut keyframes = Vec::new();

        for (i, frame_dets) in detections.iter().enumerate() {
            let time = start_time + i as f64 * sample_interval;

            let keyframe = if frame_dets.is_empty() {
                // No faces - center frame
                CameraKeyframe::centered(time, width, height)
            } else {
                // Use signal fusion to compute focus
                let focus = face_signals.compute_focus_bounds(
                    frame_dets,
                    width,
                    height,
                    self.config.signal_padding,
                );
                CameraKeyframe {
                    time,
                    cx: focus.cx(),
                    cy: focus.cy(),
                    width: focus.width,
                    height: focus.height,
                }
            };

            keyframes.push(keyframe);
        }

        // Ensure we have at least one keyframe
        if keyframes.is_empty() {
            keyframes.push(CameraKeyframe::centered(start_time, width, height));
        }

        keyframes
    }
}

impl Default for CinematicProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a cinematic clip from a video file.
///
/// # Pipeline (SINGLE ENCODE)
/// 1. `extract_segment()` - Stream copy from source (NO encode)
/// 2. Cinematic processing (detection, smoothing, optimization)
/// 3. `SinglePassRenderer` - ONE encode with crop filter
pub async fn create_cinematic_clip<P, F>(
    input: P,
    output: P,
    task: &vclip_models::ClipTask,
    encoding: &EncodingConfig,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    create_cinematic_clip_with_cache(input, output, task, encoding, None, _progress_callback).await
}

/// Create a cinematic clip with optional cached analysis.
pub async fn create_cinematic_clip_with_cache<P, F>(
    input: P,
    output: P,
    task: &vclip_models::ClipTask,
    encoding: &EncodingConfig,
    cached_analysis: Option<&SceneNeuralAnalysis>,
    _progress_callback: F,
) -> MediaResult<()>
where
    P: AsRef<Path>,
    F: Fn(crate::progress::FfmpegProgress) + Send + 'static,
{
    let input = input.as_ref();
    let output = output.as_ref();
    let total_start = std::time::Instant::now();

    info!("========================================================");
    info!("[PIPELINE] CINEMATIC - START");
    info!("[PIPELINE] Source: {:?}", input);
    info!("[PIPELINE] Output: {:?}", output);
    info!("[PIPELINE] Cached analysis: {}", cached_analysis.is_some());

    // Parse timestamps and apply padding
    let start_secs =
        (crate::intelligent::parse_timestamp(&task.start)? - task.pad_before).max(0.0);
    let end_secs = crate::intelligent::parse_timestamp(&task.end)? + task.pad_after;
    let duration = end_secs - start_secs;

    info!(
        "[PIPELINE] Time: {:.2}s to {:.2}s ({:.2}s duration)",
        start_secs, end_secs, duration
    );

    // Step 1: Extract segment using STREAM COPY (no encode)
    let segment_path = output.with_extension("segment.mp4");
    info!("[PIPELINE] Step 1/2: Extract segment (STREAM COPY - no encode)...");

    extract_segment(input, &segment_path, start_secs, duration).await?;

    // Step 2: Process with cinematic pipeline
    info!("[PIPELINE] Step 2/2: Cinematic processing (SINGLE ENCODE)...");

    let processor = CinematicProcessor::new();
    let result = processor
        .process_with_cache(segment_path.as_path(), output, encoding, cached_analysis)
        .await;

    // Cleanup temporary segment
    if segment_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&segment_path).await {
            tracing::warn!("[PIPELINE] Failed to delete temp segment: {}", e);
        } else {
            info!("[PIPELINE] Cleaned up temp segment");
        }
    }

    let file_size = tokio::fs::metadata(output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    info!("========================================================");
    info!(
        "[PIPELINE] CINEMATIC - COMPLETE in {:.2}s - {:.2} MB",
        total_start.elapsed().as_secs_f64(),
        file_size as f64 / 1_000_000.0
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::signals::FaceSignals;
    use crate::intelligent::models::BoundingBox;

    #[test]
    fn test_processor_creation() {
        let processor = CinematicProcessor::new();
        assert!(processor.config.polynomial_degree >= 2);
    }

    #[test]
    fn test_podcast_config() {
        let processor = CinematicProcessor::for_podcast();
        // Podcast config should have higher stationary threshold
        assert!(processor.config.stationary_threshold >= 0.05);
    }

    #[test]
    fn test_activity_scores_empty() {
        let processor = CinematicProcessor::new();
        let detections: Vec<Vec<Detection>> = vec![];
        let scores = processor.compute_activity_scores(&detections);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_activity_scores_single_track() {
        let processor = CinematicProcessor::new();
        let detections = vec![
            vec![Detection::with_mouth(
                0.0,
                BoundingBox::new(100.0, 100.0, 50.0, 50.0),
                0.9,
                1,
                Some(0.5),
            )],
            vec![Detection::with_mouth(
                0.1,
                BoundingBox::new(100.0, 100.0, 50.0, 50.0),
                0.9,
                1,
                Some(0.7),
            )],
        ];
        let scores = processor.compute_activity_scores(&detections);
        assert_eq!(scores.len(), 1);
        assert!((scores[&1] - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_keyframes_with_fusion_empty() {
        let processor = CinematicProcessor::new();
        let face_signals = FaceSignals::with_weights(
            processor.config.face_weight,
            processor.config.activity_boost,
        );
        let detections: Vec<Vec<Detection>> = vec![];
        let keyframes = processor.create_raw_keyframes_with_fusion(
            &detections, &face_signals, 1920, 1080, processor.config.detection_fps, 0.0
        );
        assert_eq!(keyframes.len(), 1);
        // Should be centered
        assert!((keyframes[0].cx - 960.0).abs() < 1.0);
    }

    #[test]
    fn test_keyframes_with_fusion_single_face() {
        let processor = CinematicProcessor::new();
        let face_signals = FaceSignals::with_weights(
            processor.config.face_weight,
            processor.config.activity_boost,
        );
        let detections = vec![
            vec![Detection::with_mouth(
                0.0,
                BoundingBox::new(800.0, 400.0, 100.0, 120.0),
                0.9,
                1,
                Some(0.5),
            )],
        ];
        let keyframes = processor.create_raw_keyframes_with_fusion(
            &detections, &face_signals, 1920, 1080, processor.config.detection_fps, 0.0
        );
        assert_eq!(keyframes.len(), 1);
        // Should be focused on the face center (800 + 50 = 850)
        assert!((keyframes[0].cx - 850.0).abs() < 100.0);
    }
}
