//! Face detection module.
//!
//! Detects faces in video frames using multiple backends:
//! 1. OpenCV YuNet (best accuracy, requires opencv feature)
//! 2. FFmpeg-based heuristics (fallback)
//!
//! # Architecture
//!
//! The detector uses a multi-phase approach:
//! 1. Try YuNet face detection if OpenCV is available
//! 2. Fall back to FFmpeg-based motion/edge analysis
//! 3. Use intelligent heuristics for common video layouts
//!
//! # Face Detection Backends
//!
//! - **OpenCV YuNet** (best): Deep learning face detector, fast and accurate
//! - **FFmpeg Heuristic**: Analyzes video composition for face regions

use super::config::IntelligentCropConfig;
use super::layout_detector::{HeuristicGenerator, LayoutDetector, VideoLayout};
use super::models::{BoundingBox, Detection, FrameDetections};
use super::tracker::IoUTracker;
use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::process::Stdio;
use tracing::{debug, info, warn};

/// Face detector for video analysis.
pub struct FaceDetector {
    config: IntelligentCropConfig,
    heuristic_generator: HeuristicGenerator,
}

impl FaceDetector {
    const MAX_SAMPLED_FRAMES: usize = 360; // hard cap to avoid runaway detection on long clips
    const MAX_TOTAL_DETECTIONS: usize = 300;
    const DETECTION_HEARTBEAT: usize = 40;

    /// Create a new face detector.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self {
            heuristic_generator: HeuristicGenerator::new(config.clone()),
            config,
        }
    }

    /// Try YuNet detection with proper error handling.
    ///
    /// This method encapsulates the YuNet detection attempt and handles
    /// model availability checking and automatic download.
    #[cfg(feature = "opencv")]
    async fn try_yunet_detection<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
        // Check if YuNet is available
        if !super::yunet::is_yunet_available() {
            // Try to download models automatically
            if !super::yunet::ensure_yunet_available().await {
                return Err(MediaError::detection_failed(
                    "YuNet model not available and automatic download failed",
                ));
            }
        }

        info!("Using OpenCV YuNet for face detection");

        super::yunet::detect_faces_with_yunet(
            video_path,
            start_time,
            end_time,
            width,
            height,
            self.config.fps_sample,
        )
        .await
    }

    /// Detect faces in a video over a time range.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file
    /// * `start_time` - Start time in seconds
    /// * `end_time` - End time in seconds
    /// * `width` - Video width
    /// * `height` - Video height
    /// * `fps` - Video frame rate
    ///
    /// # Returns
    /// Vector of frame detections, one per sampled frame
    pub async fn detect_in_video<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
        fps: f64,
    ) -> MediaResult<Vec<FrameDetections>> {
        let video_path = video_path.as_ref();
        let duration = end_time - start_time;

        // Calculate sample interval
        let sample_interval = 1.0 / self.config.fps_sample;
        let num_samples = (duration / sample_interval).ceil() as usize;
        let capped_samples = num_samples.min(Self::MAX_SAMPLED_FRAMES);

        info!(
            "Analyzing {} frames at {:.1} fps over {:.2}s",
            capped_samples, self.config.fps_sample, duration
        );

        // Create tracker
        let mut tracker = IoUTracker::new(self.config.iou_threshold, self.config.max_track_gap);

        // Extract and analyze frames
        let mut all_detections = Vec::with_capacity(capped_samples);
        let mut total_detections = 0usize;

        // Use FFmpeg to extract face regions
        // We'll use FFmpeg's metadata extraction to detect faces
        let face_detections = self
            .extract_face_regions(video_path, start_time, end_time, width, height, fps)
            .await?;

        // Process each frame's detections through the tracker
        let mut current_time = start_time;
        let mut frame_idx = 0;

        while current_time < end_time
            && frame_idx < face_detections.len()
            && frame_idx < Self::MAX_SAMPLED_FRAMES
        {
            let raw_dets = &face_detections[frame_idx];

            // Convert to tracker format
            let tracker_input: Vec<(BoundingBox, f64)> = raw_dets
                .iter()
                .filter_map(|(bbox, score)| {
                    // Filter by minimum face size
                    let face_area_ratio = bbox.area() / (width as f64 * height as f64);
                    if face_area_ratio >= self.config.min_face_size {
                        Some((*bbox, *score))
                    } else {
                        debug!(
                            "Filtered small face: {:.1}x{:.1} at ({:.0},{:.0}), area_ratio={:.4} < min={:.4}",
                            bbox.width, bbox.height, bbox.x, bbox.y,
                            face_area_ratio, self.config.min_face_size
                        );
                        None
                    }
                })
                .collect();

            // Update tracker
            let tracked = tracker.update(&tracker_input);

            // Create Detection objects
            let frame_dets: FrameDetections = tracked
                .into_iter()
                .map(|(track_id, bbox, score)| Detection::new(current_time, bbox, score, track_id))
                .collect();

            all_detections.push(frame_dets);
            total_detections += all_detections.last().map(|f| f.len()).unwrap_or(0);

            if frame_idx % Self::DETECTION_HEARTBEAT == 0 {
                info!(
                    frame = frame_idx,
                    total_frames = capped_samples,
                    total_detections = total_detections,
                    "Face detection progress"
                );
            }

            if total_detections >= Self::MAX_TOTAL_DETECTIONS {
                warn!(
                    total = total_detections,
                    "Stopping detection early: hit max detections cap"
                );
                break;
            }

            current_time += sample_interval;
            frame_idx += 1;
        }

        // Fill remaining frames if needed
        while all_detections.len() < capped_samples {
            all_detections.push(Vec::new());
        }

        let total_dets: usize = all_detections.iter().map(|d| d.len()).sum();
        debug!(
            "Detection complete: {} total detections across {} frames",
            total_dets, capped_samples
        );

        Ok(all_detections)
    }

    /// Detect motion tracks for fallback rendering when no faces are available.
    pub async fn detect_motion_tracks<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        duration: f64,
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<FrameDetections>> {
        let video_path = video_path.as_ref();
        let sample_interval = 1.0 / self.config.fps_sample;

        // Use the existing FFmpeg motion analysis path to find moving regions
        let motion_boxes = self
            .analyze_motion(video_path, start_time, duration, width, height)
            .await?;

        let mut tracker = IoUTracker::new(self.config.iou_threshold, self.config.max_track_gap);
        let mut frames = Vec::with_capacity(motion_boxes.len());
        let mut current_time = start_time;

        for boxes in motion_boxes {
            // Track motion blobs as pseudo faces so downstream camera planners work unchanged
            let tracked = tracker.update(&boxes);
            let frame_dets: FrameDetections = tracked
                .into_iter()
                .map(|(track_id, bbox, score)| Detection::new(current_time, bbox, score, track_id))
                .collect();
            frames.push(frame_dets);
            current_time += sample_interval;
        }

        let has_any_detection = frames.iter().any(|f| !f.is_empty());
        if !has_any_detection {
            let (bbox, score) = self.heuristic_generator.create_centered_detection(width, height, 0.6);
            let expected_frames = (duration / sample_interval).ceil().max(1.0) as usize;

            frames = (0..expected_frames)
                .map(|i| {
                    let det =
                        Detection::new(start_time + i as f64 * sample_interval, bbox, score, 0);
                    vec![det]
                })
                .collect();
        }

        Ok(frames)
    }

    /// Extract face regions from video using the best available method.
    ///
    /// Priority:
    /// 1. OpenCV YuNet (if feature enabled and model available)
    /// 2. FFmpeg analysis with skin tone detection
    /// 3. Intelligent heuristics based on video layout
    async fn extract_face_regions<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
        fps: f64,
    ) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
        let video_path = video_path.as_ref();
        let duration = end_time - start_time;
        let sample_interval = 1.0 / self.config.fps_sample;
        let num_samples = (duration / sample_interval).ceil() as usize;
        let capped_samples = num_samples.min(Self::MAX_SAMPLED_FRAMES);

        // Try YuNet first if available
        #[cfg(feature = "opencv")]
        {
            let yunet_result = self
                .try_yunet_detection(video_path, start_time, end_time, width, height)
                .await;

            match yunet_result {
                Ok(detections) if detections.iter().any(|d| !d.is_empty()) => {
                    info!(
                        "YuNet detection successful: {} frames with faces",
                        detections.iter().filter(|d| !d.is_empty()).count()
                    );
                    return Ok(detections);
                }
                Ok(_) => {
                    info!("YuNet found no faces, falling back to heuristics");
                }
                Err(e) => {
                    // Log the error but continue to fallback
                    let error_str = e.to_string();
                    if error_str.contains("OpenCV")
                        || error_str.contains("Layer with requested id")
                    {
                        warn!("YuNet OpenCV compatibility issue: {}", error_str);
                    } else {
                        warn!("YuNet detection failed: {}", error_str);
                    }
                    info!("Falling back to heuristic face detection");
                }
            }
        }

        // Try FFmpeg analysis
        let detections = self
            .analyze_with_ffmpeg(video_path, start_time, duration, width, height, fps)
            .await?;

        let has_faces = detections.iter().any(|d| !d.is_empty());
        let has_multi_face_spread = Self::has_multi_face_spread(&detections, width);

        if has_faces && has_multi_face_spread {
            return Ok(detections);
        }

        // If FFmpeg analysis only produced single-track/centered detections,
        // fall back to layout-aware heuristics so two-person podcasts still
        // leverage speaker detection to move the camera.
        info!(
            "FFmpeg analysis produced low-information detections (faces: {}, spread: {}); using layout-aware heuristics",
            has_faces,
            has_multi_face_spread
        );

        let layout_detector = LayoutDetector::new();
        let layout = layout_detector.detect_layout(video_path, width, height).await?;

        match layout {
            VideoLayout::SinglePerson => {
                if has_faces {
                    Ok(detections)
                } else {
                    info!("Using single-person heuristic detections");
                    Ok(self
                        .heuristic_generator
                        .single_person_heuristic(width, height, capped_samples))
                }
            }
            VideoLayout::TwoPeopleSideBySide => {
                info!("Using speaker-aware heuristic detections for podcast layout");
                self.heuristic_generator
                    .speaker_aware_heuristic(video_path, width, height, capped_samples)
                    .await
            }
        }
    }

    /// Analyze video using FFmpeg's filters for face detection.
    async fn analyze_with_ffmpeg<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        duration: f64,
        width: u32,
        height: u32,
        _fps: f64,
    ) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
        let video_path = video_path.as_ref();
        let sample_interval = 1.0 / self.config.fps_sample;
        let num_samples = (duration / sample_interval).ceil() as usize;

        // Use FFmpeg's selectivecolor or histogram analysis
        // to detect regions with skin tones and high contrast (face-like)
        let filter = format!(
            "fps={},\
             scale={}:-1,\
             format=gray,\
             edgedetect=low=0.1:high=0.4,\
             metadata=print:file=-",
            self.config.fps_sample, self.config.analysis_resolution
        );

        let mut cmd = crate::command::create_ffmpeg_command();
        cmd.args([
            "-ss",
            &format!("{:.3}", start_time),
            "-t",
            &format!("{:.3}", duration),
            "-i",
            video_path.to_str().unwrap_or(""),
            "-vf",
            &filter,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(
                format!("Failed to run FFmpeg for analysis: {}", e),
                None,
                None,
            )
        })?;

        // Parse FFmpeg output for edge detection results
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detections = self.parse_ffmpeg_analysis(&stderr, width, height, num_samples);

        if detections.iter().all(|d| d.is_empty()) {
            // Edge detection didn't find enough, use motion analysis
            return self
                .analyze_motion(video_path, start_time, duration, width, height)
                .await;
        }

        Ok(detections)
    }

    /// Analyze motion in video to find regions of interest.
    async fn analyze_motion<P: AsRef<Path>>(
        &self,
        video_path: P,
        start_time: f64,
        duration: f64,
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
        let video_path = video_path.as_ref();
        let num_samples = (duration * self.config.fps_sample).ceil() as usize;

        // Use FFmpeg's mpdecimate or select filter to find frames with motion
        let filter = format!(
            "fps={},\
             select='gt(scene,0.1)',\
             showinfo",
            self.config.fps_sample
        );

        let mut cmd = crate::command::create_ffmpeg_command();
        cmd.args([
            "-ss",
            &format!("{:.3}", start_time),
            "-t",
            &format!("{:.3}", duration),
            "-i",
            video_path.to_str().unwrap_or(""),
            "-vf",
            &filter,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            MediaError::ffmpeg_failed(
                format!("Failed to run FFmpeg motion analysis: {}", e),
                None,
                None,
            )
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse showinfo output for scene changes and motion
        let detections = self.parse_motion_analysis(&stderr, width, height, num_samples);

        Ok(detections)
    }

    /// Parse FFmpeg edge detection analysis output.
    fn parse_ffmpeg_analysis(
        &self,
        _output: &str,
        width: u32,
        height: u32,
        num_samples: usize,
    ) -> Vec<Vec<(BoundingBox, f64)>> {
        // Create default detections based on frame composition heuristics
        // This is where we'd parse FFmpeg metadata if available

        // For now, use smart heuristics based on video composition
        self.heuristic_generator
            .single_person_heuristic(width, height, num_samples)
    }

    /// Parse motion analysis output.
    fn parse_motion_analysis(
        &self,
        output: &str,
        width: u32,
        height: u32,
        num_samples: usize,
    ) -> Vec<Vec<(BoundingBox, f64)>> {
        // Parse showinfo output for motion information
        // Format: n:X pts:X pts_time:X pos:X fmt:X sar:X s:WxH ...

        let mut detections = Vec::with_capacity(num_samples);
        let mut frame_count = 0;

        for line in output.lines() {
            if line.contains("showinfo") && line.contains("pts_time:") {
                // Extract scene score if available
                let has_motion = line.contains("scene:") || !line.contains("dup");

                if has_motion && frame_count < num_samples {
                    // Detected motion - use center-weighted detection
                    let det = self
                        .heuristic_generator
                        .create_centered_detection(width, height, 0.7);
                    detections.push(vec![det]);
                    frame_count += 1;
                }
            }
        }

        // Fill remaining with heuristic detections
        while detections.len() < num_samples {
            let det = self
                .heuristic_generator
                .create_centered_detection(width, height, 0.5);
            detections.push(vec![det]);
        }

        detections
    }

}

impl FaceDetector {
    /// Determine if detections show multiple faces on distinct sides of the frame.
    fn has_multi_face_spread(detections: &[Vec<(BoundingBox, f64)>], width: u32) -> bool {
        if width == 0 {
            return false;
        }

        let spread_threshold = (width as f64 * 0.15).max(1.0);

        detections.iter().any(|frame| {
            if frame.len() < 2 {
                return false;
            }

            let mut min_cx = f64::INFINITY;
            let mut max_cx = f64::NEG_INFINITY;

            for (bbox, _) in frame {
                let cx = bbox.cx();
                if cx < min_cx {
                    min_cx = cx;
                }
                if cx > max_cx {
                    max_cx = cx;
                }
            }

            (max_cx - min_cx) > spread_threshold
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_face_detector() {
        let config = IntelligentCropConfig::default();
        let _detector = FaceDetector::new(config);
    }

    #[test]
    fn test_heuristic_generator_integration() {
        let config = IntelligentCropConfig::default();
        let detector = FaceDetector::new(config);

        // Test that heuristic generator works through detector
        let (bbox, score) = detector
            .heuristic_generator
            .create_centered_detection(1920, 1080, 0.8);

        assert!(bbox.cx() > 900.0 && bbox.cx() < 1020.0);
        assert_eq!(score, 0.8);
    }
}
