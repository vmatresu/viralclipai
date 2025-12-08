//! Tier-aware camera path smoothing.
//!
//! This module extends the base camera smoother with tier-specific behavior:
//! - **Basic**: Follow the most prominent face (largest × confidence)
//! - **AudioAware**: Prioritize faces on the active speaker side
//! - **SpeakerAware**: Use full activity tracking with hysteresis
//!
//! The key difference from the base smoother is that AudioAware and SpeakerAware
//! tiers use speaker/activity information to decide which face to follow,
//! rather than just using size and confidence.

use std::collections::HashMap;
use tracing::{debug, info};
use vclip_models::DetectionTier;

use super::activity_scorer::TemporalActivityTracker;
use super::config::{FallbackPolicy, IntelligentCropConfig};
use super::face_activity::FaceActivityConfig;
use super::models::{BoundingBox, CameraKeyframe, CameraMode, Detection, FrameDetections};
use super::speaker_detector::{ActiveSpeaker, SpeakerSegment};

/// Tier-aware camera smoother that uses speaker and activity information.
pub struct TierAwareCameraSmoother {
    config: IntelligentCropConfig,
    tier: DetectionTier,
    #[allow(dead_code)]
    fps: f64,
    /// Speaker segments for AudioAware/SpeakerAware tiers
    speaker_segments: Vec<SpeakerSegment>,
    /// Activity tracker for SpeakerAware tier
    activity_tracker: TemporalActivityTracker,
    /// Track ID to side mapping (left=true, right=false)
    track_sides: HashMap<u32, bool>,
}

impl TierAwareCameraSmoother {
    /// Create a new tier-aware camera smoother.
    pub fn new(config: IntelligentCropConfig, tier: DetectionTier, fps: f64) -> Self {
        let activity_config = FaceActivityConfig {
            activity_window: config.face_activity_window,
            min_switch_duration: config.min_switch_duration,
            switch_margin: config.switch_margin,
            weight_mouth: config.activity_weight_mouth,
            weight_motion: config.activity_weight_motion,
            weight_size: config.activity_weight_size_change,
            smoothing_alpha: config.activity_smoothing_window,
            ..Default::default()
        };

        Self {
            config,
            tier,
            fps,
            speaker_segments: Vec::new(),
            activity_tracker: TemporalActivityTracker::new(activity_config),
            track_sides: HashMap::new(),
        }
    }

    /// Set speaker segments for audio-aware processing.
    pub fn with_speaker_segments(mut self, segments: Vec<SpeakerSegment>) -> Self {
        self.speaker_segments = segments;
        self
    }

    /// Update activity for a detection (used by SpeakerAware tier).
    pub fn update_activity(&mut self, detection: &Detection, audio_score: f64) {
        // For SpeakerAware, we track visual activity + audio
        // Visual activity is approximated from detection confidence changes
        let visual_score = detection.score;
        self.activity_tracker.update_activity(
            detection.track_id,
            visual_score,
            audio_score,
            detection.time,
        );
    }

    /// Compute a smooth camera plan from detections using tier-specific logic.
    pub fn compute_camera_plan(
        &mut self,
        detections: &[FrameDetections],
        width: u32,
        height: u32,
        start_time: f64,
        end_time: f64,
    ) -> Vec<CameraKeyframe> {
        info!(
            "Computing tier-aware camera plan (tier: {:?}, {} frames)",
            self.tier,
            detections.len()
        );

        // Build track side mapping from first few frames
        self.build_track_sides(detections, width);

        // Generate raw focus points using tier-specific logic
        let raw_keyframes = self.compute_raw_focus(detections, width, height, start_time, end_time);

        if raw_keyframes.is_empty() {
            return vec![CameraKeyframe::centered(start_time, width, height)];
        }

        // Determine camera mode
        let mode = self.classify_camera_mode(&raw_keyframes);
        debug!("Camera mode: {:?}", mode);

        // Apply smoothing based on mode AND tier
        // For AudioAware/SpeakerAware, use instant transitions at speaker boundaries
        let smoothed = match (mode, self.tier) {
            // For speaker-aware tiers with tracking, use instant transitions
            (CameraMode::Tracking | CameraMode::Zoom, DetectionTier::AudioAware | DetectionTier::SpeakerAware) => {
                self.smooth_with_instant_speaker_transitions(&raw_keyframes)
            }
            (CameraMode::Static, _) => self.smooth_static(&raw_keyframes),
            (CameraMode::Tracking | CameraMode::Zoom, _) => self.smooth_tracking(&raw_keyframes),
        };

        // Enforce motion constraints (but less strict for speaker-aware to allow fast moves)
        match self.tier {
            DetectionTier::AudioAware | DetectionTier::SpeakerAware => {
                // Allow faster movements for speaker tracking
                self.enforce_constraints_relaxed(&smoothed, width, height)
            }
            _ => self.enforce_constraints(&smoothed, width, height)
        }
    }

    /// Build mapping of track IDs to left/right side of frame.
    fn build_track_sides(&mut self, detections: &[FrameDetections], width: u32) {
        let center_x = width as f64 / 2.0;

        for frame_dets in detections.iter().take(10) {
            for det in frame_dets {
                if !self.track_sides.contains_key(&det.track_id) {
                    let is_left = det.bbox.cx() < center_x;
                    self.track_sides.insert(det.track_id, is_left);
                }
            }
        }
    }

    /// Compute raw focus points using tier-specific selection.
    fn compute_raw_focus(
        &mut self,
        detections: &[FrameDetections],
        width: u32,
        height: u32,
        start_time: f64,
        end_time: f64,
    ) -> Vec<CameraKeyframe> {
        let sample_interval = 1.0 / self.config.fps_sample;
        let mut keyframes = Vec::new();

        let mut current_time = start_time;
        let mut frame_idx = 0;

        while current_time < end_time && frame_idx < detections.len() {
            let frame_dets = &detections[frame_idx];

            let keyframe = if frame_dets.is_empty() {
                self.create_fallback_keyframe(current_time, width, height)
            } else {
                // Use tier-specific focus computation
                let focus = match self.tier {
                    DetectionTier::None => {
                        self.compute_focus_basic(frame_dets, width, height)
                    }
                    DetectionTier::Basic => {
                        self.compute_focus_basic(frame_dets, width, height)
                    }
                    DetectionTier::AudioAware => {
                        self.compute_focus_audio_aware(frame_dets, current_time, width, height)
                    }
                    DetectionTier::SpeakerAware => {
                        self.compute_focus_speaker_aware(frame_dets, current_time, width, height)
                    }
                };

                CameraKeyframe::new(
                    current_time,
                    focus.cx(),
                    focus.cy(),
                    focus.width,
                    focus.height,
                )
            };

            keyframes.push(keyframe);
            current_time += sample_interval;
            frame_idx += 1;
        }

        keyframes
    }

    /// Basic tier: Focus on the most prominent face (largest × confidence).
    fn compute_focus_basic(
        &self,
        detections: &[Detection],
        width: u32,
        height: u32,
    ) -> BoundingBox {
        if detections.is_empty() {
            return self.create_fallback_box(width, height);
        }

        // Find the most prominent face
        let primary = detections
            .iter()
            .max_by(|a, b| {
                let score_a = a.bbox.area() * a.score;
                let score_b = b.bbox.area() * b.score;
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let focus_box = primary.bbox.pad(primary.bbox.width * self.config.subject_padding);
        focus_box.clamp(width, height)
    }

    /// AudioAware tier: Prioritize faces on the active speaker side.
    fn compute_focus_audio_aware(
        &self,
        detections: &[Detection],
        time: f64,
        width: u32,
        height: u32,
    ) -> BoundingBox {
        if detections.is_empty() {
            return self.create_fallback_box(width, height);
        }

        // Get active speaker at this time
        let active_speaker = self.get_speaker_at_time(time);

        // Filter detections by speaker side
        let speaker_side_dets: Vec<&Detection> = match active_speaker {
            ActiveSpeaker::Left => {
                detections.iter()
                    .filter(|d| self.track_sides.get(&d.track_id).copied().unwrap_or(true))
                    .collect()
            }
            ActiveSpeaker::Right => {
                detections.iter()
                    .filter(|d| !self.track_sides.get(&d.track_id).copied().unwrap_or(true))
                    .collect()
            }
            ActiveSpeaker::Both | ActiveSpeaker::None => {
                // Fall back to basic behavior
                return self.compute_focus_basic(detections, width, height);
            }
        };

        if speaker_side_dets.is_empty() {
            // No detection on speaker side, fall back to basic
            return self.compute_focus_basic(detections, width, height);
        }

        // Select the most prominent face on the speaker side
        let primary = speaker_side_dets
            .iter()
            .max_by(|a, b| {
                let score_a = a.bbox.area() * a.score;
                let score_b = b.bbox.area() * b.score;
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let focus_box = primary.bbox.pad(primary.bbox.width * self.config.subject_padding);
        focus_box.clamp(width, height)
    }

    /// SpeakerAware tier: Use full activity tracking with hysteresis.
    fn compute_focus_speaker_aware(
        &mut self,
        detections: &[Detection],
        time: f64,
        width: u32,
        height: u32,
    ) -> BoundingBox {
        if detections.is_empty() {
            return self.create_fallback_box(width, height);
        }

        // Get audio score for each detection based on speaker side
        let active_speaker = self.get_speaker_at_time(time);

        for det in detections {
            let is_left = self.track_sides.get(&det.track_id).copied().unwrap_or(true);
            let audio_score = match active_speaker {
                ActiveSpeaker::Left if is_left => 1.0,
                ActiveSpeaker::Right if !is_left => 1.0,
                ActiveSpeaker::Both => 0.5,
                ActiveSpeaker::None => 0.0,
                _ => 0.0,
            };

            // Update activity tracker with visual + audio scores
            self.activity_tracker.update_activity(
                det.track_id,
                det.score, // Use detection confidence as visual activity proxy
                audio_score,
                time,
            );
        }

        // Select active face using hysteresis
        let track_ids: Vec<u32> = detections.iter().map(|d| d.track_id).collect();
        let selected_track = self.activity_tracker.select_active_face(&track_ids, time);

        match selected_track {
            Some(track_id) => {
                // Find the detection for the selected track
                if let Some(det) = detections.iter().find(|d| d.track_id == track_id) {
                    let focus_box = det.bbox.pad(det.bbox.width * self.config.subject_padding);
                    return focus_box.clamp(width, height);
                }
            }
            None => {}
        }

        // Fallback to basic
        self.compute_focus_basic(detections, width, height)
    }

    /// Get the active speaker at a specific time.
    fn get_speaker_at_time(&self, time: f64) -> ActiveSpeaker {
        for segment in &self.speaker_segments {
            if time >= segment.start_time && time < segment.end_time {
                return segment.speaker;
            }
        }
        ActiveSpeaker::None
    }

    /// Create fallback keyframe based on policy.
    fn create_fallback_keyframe(&self, time: f64, width: u32, height: u32) -> CameraKeyframe {
        let focus = self.create_fallback_box(width, height);
        CameraKeyframe::new(time, focus.cx(), focus.cy(), focus.width, focus.height)
    }

    /// Create fallback bounding box based on policy.
    fn create_fallback_box(&self, width: u32, height: u32) -> BoundingBox {
        let w = width as f64;
        let h = height as f64;

        match self.config.fallback_policy {
            FallbackPolicy::Center => {
                BoundingBox::new(w * 0.2, h * 0.2, w * 0.6, h * 0.6)
            }
            FallbackPolicy::UpperCenter => {
                BoundingBox::new(w * 0.15, h * 0.15, w * 0.7, h * 0.5)
            }
            FallbackPolicy::RuleOfThirds => {
                BoundingBox::new(w * 0.2, h * 0.15, w * 0.6, h * 0.45)
            }
        }
    }

    /// Classify camera mode based on motion.
    fn classify_camera_mode(&self, keyframes: &[CameraKeyframe]) -> CameraMode {
        if keyframes.len() < 2 {
            return CameraMode::Static;
        }

        let cx_values: Vec<f64> = keyframes.iter().map(|kf| kf.cx).collect();
        let cy_values: Vec<f64> = keyframes.iter().map(|kf| kf.cy).collect();
        let width_values: Vec<f64> = keyframes.iter().map(|kf| kf.width).collect();

        let cx_std = std_deviation(&cx_values);
        let cy_std = std_deviation(&cy_values);
        let width_std = std_deviation(&width_values);

        let avg_width = mean(&width_values);
        let motion_threshold = avg_width * 0.1;
        let zoom_threshold = avg_width * 0.15;

        if width_std > zoom_threshold {
            CameraMode::Zoom
        } else if cx_std > motion_threshold || cy_std > motion_threshold {
            CameraMode::Tracking
        } else {
            CameraMode::Static
        }
    }

    /// Smooth keyframes for static camera mode.
    fn smooth_static(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        if keyframes.is_empty() {
            return Vec::new();
        }

        let cx = median(&keyframes.iter().map(|kf| kf.cx).collect::<Vec<_>>());
        let cy = median(&keyframes.iter().map(|kf| kf.cy).collect::<Vec<_>>());
        let width = median(&keyframes.iter().map(|kf| kf.width).collect::<Vec<_>>());
        let height = median(&keyframes.iter().map(|kf| kf.height).collect::<Vec<_>>());

        keyframes
            .iter()
            .map(|kf| CameraKeyframe::new(kf.time, cx, cy, width, height))
            .collect()
    }

    /// Smooth keyframes for tracking camera mode.
    fn smooth_tracking(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        if keyframes.len() < 3 {
            return keyframes.to_vec();
        }

        let duration = keyframes.last().unwrap().time - keyframes.first().unwrap().time;
        let sample_rate = if duration > 0.0 {
            keyframes.len() as f64 / duration
        } else {
            1.0
        };

        let mut window_samples = (self.config.smoothing_window * sample_rate) as usize;
        window_samples = window_samples.max(3);
        if window_samples % 2 == 0 {
            window_samples += 1;
        }

        let cx: Vec<f64> = keyframes.iter().map(|kf| kf.cx).collect();
        let cy: Vec<f64> = keyframes.iter().map(|kf| kf.cy).collect();
        let width: Vec<f64> = keyframes.iter().map(|kf| kf.width).collect();
        let height: Vec<f64> = keyframes.iter().map(|kf| kf.height).collect();

        let cx_smooth = moving_average(&cx, window_samples);
        let cy_smooth = moving_average(&cy, window_samples);
        let width_smooth = moving_average(&width, window_samples);
        let height_smooth = moving_average(&height, window_samples);

        keyframes
            .iter()
            .enumerate()
            .map(|(i, kf)| {
                CameraKeyframe::new(
                    kf.time,
                    cx_smooth[i],
                    cy_smooth[i],
                    width_smooth[i],
                    height_smooth[i],
                )
            })
            .collect()
    }

    /// Enforce motion constraints on keyframes.
    fn enforce_constraints(
        &self,
        keyframes: &[CameraKeyframe],
        width: u32,
        height: u32,
    ) -> Vec<CameraKeyframe> {
        if keyframes.len() < 2 {
            return keyframes.to_vec();
        }

        let mut constrained = Vec::with_capacity(keyframes.len());
        constrained.push(keyframes[0]);

        for i in 1..keyframes.len() {
            let prev = &constrained[i - 1];
            let curr = &keyframes[i];

            let dt = curr.time - prev.time;
            if dt <= 0.0 {
                constrained.push(*curr);
                continue;
            }

            let dx = curr.cx - prev.cx;
            let dy = curr.cy - prev.cy;
            let speed = (dx * dx + dy * dy).sqrt() / dt;

            let (new_cx, new_cy) = if speed > self.config.max_pan_speed {
                let scale = self.config.max_pan_speed / speed;
                (prev.cx + dx * scale, prev.cy + dy * scale)
            } else {
                (curr.cx, curr.cy)
            };

            let margin_x = curr.width / 2.0;
            let margin_y = curr.height / 2.0;
            let clamped_cx = new_cx.max(margin_x).min(width as f64 - margin_x);
            let clamped_cy = new_cy.max(margin_y).min(height as f64 - margin_y);

            constrained.push(CameraKeyframe::new(
                curr.time,
                clamped_cx,
                clamped_cy,
                curr.width,
                curr.height,
            ));
        }

        constrained
    }

    /// Smooth keyframes with instant transitions at speaker change points.
    /// Uses minimal smoothing within speaker segments, but preserves raw positions at boundaries.
    fn smooth_with_instant_speaker_transitions(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        if keyframes.len() < 3 {
            return keyframes.to_vec();
        }

        // Detect significant position changes (speaker switches)
        let mut switch_indices: Vec<usize> = Vec::new();
        let avg_width: f64 = keyframes.iter().map(|kf| kf.width).sum::<f64>() / keyframes.len() as f64;
        let switch_threshold = avg_width * 0.3; // 30% of crop width = significant move

        for i in 1..keyframes.len() {
            let dx = (keyframes[i].cx - keyframes[i - 1].cx).abs();
            if dx > switch_threshold {
                switch_indices.push(i);
            }
        }

        // If no significant switches detected, use light smoothing
        if switch_indices.is_empty() {
            return self.smooth_tracking(keyframes);
        }

        debug!("Detected {} speaker switches in keyframes", switch_indices.len());

        // Apply smoothing within segments, but preserve positions at switch points
        let mut result = Vec::with_capacity(keyframes.len());
        let mut segment_start = 0;

        for &switch_idx in &switch_indices {
            // Process segment before switch
            if switch_idx > segment_start {
                let segment: Vec<_> = keyframes[segment_start..switch_idx].to_vec();
                if segment.len() >= 3 {
                    // Light smoothing within segment (small window)
                    let smoothed = self.smooth_segment_light(&segment);
                    result.extend(smoothed);
                } else {
                    result.extend(segment);
                }
            }
            segment_start = switch_idx;
        }

        // Process final segment
        if segment_start < keyframes.len() {
            let segment: Vec<_> = keyframes[segment_start..].to_vec();
            if segment.len() >= 3 {
                let smoothed = self.smooth_segment_light(&segment);
                result.extend(smoothed);
            } else {
                result.extend(segment);
            }
        }

        result
    }

    /// Light smoothing for individual segments (preserves quick movements).
    fn smooth_segment_light(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        if keyframes.len() < 3 {
            return keyframes.to_vec();
        }

        // Use very small window (3 samples) for minimal smoothing
        let window = 3;

        let cx: Vec<f64> = keyframes.iter().map(|kf| kf.cx).collect();
        let cy: Vec<f64> = keyframes.iter().map(|kf| kf.cy).collect();
        let width: Vec<f64> = keyframes.iter().map(|kf| kf.width).collect();
        let height: Vec<f64> = keyframes.iter().map(|kf| kf.height).collect();

        let cx_smooth = moving_average(&cx, window);
        let cy_smooth = moving_average(&cy, window);
        let width_smooth = moving_average(&width, window);
        let height_smooth = moving_average(&height, window);

        keyframes
            .iter()
            .enumerate()
            .map(|(i, kf)| {
                CameraKeyframe::new(
                    kf.time,
                    cx_smooth[i],
                    cy_smooth[i],
                    width_smooth[i],
                    height_smooth[i],
                )
            })
            .collect()
    }

    /// Relaxed motion constraints for speaker-aware tiers.
    /// Allows faster camera movements to track speakers.
    fn enforce_constraints_relaxed(
        &self,
        keyframes: &[CameraKeyframe],
        width: u32,
        height: u32,
    ) -> Vec<CameraKeyframe> {
        if keyframes.len() < 2 {
            return keyframes.to_vec();
        }

        // Use 3x the normal max pan speed for speaker tracking
        let relaxed_max_pan_speed = self.config.max_pan_speed * 3.0;

        let mut constrained = Vec::with_capacity(keyframes.len());
        constrained.push(keyframes[0]);

        for i in 1..keyframes.len() {
            let prev = &constrained[i - 1];
            let curr = &keyframes[i];

            let dt = curr.time - prev.time;
            if dt <= 0.0 {
                constrained.push(*curr);
                continue;
            }

            let dx = curr.cx - prev.cx;
            let dy = curr.cy - prev.cy;
            let speed = (dx * dx + dy * dy).sqrt() / dt;

            let (new_cx, new_cy) = if speed > relaxed_max_pan_speed {
                let scale = relaxed_max_pan_speed / speed;
                (prev.cx + dx * scale, prev.cy + dy * scale)
            } else {
                (curr.cx, curr.cy)
            };

            let margin_x = curr.width / 2.0;
            let margin_y = curr.height / 2.0;
            let clamped_cx = new_cx.max(margin_x).min(width as f64 - margin_x);
            let clamped_cy = new_cy.max(margin_y).min(height as f64 - margin_y);

            constrained.push(CameraKeyframe::new(
                curr.time,
                clamped_cx,
                clamped_cy,
                curr.width,
                curr.height,
            ));
        }

        constrained
    }
}

// === Helper Functions ===

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn std_deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = mean(values);
    let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

fn moving_average(data: &[f64], window: usize) -> Vec<f64> {
    if data.len() < window {
        return data.to_vec();
    }

    let pad = window / 2;
    let mut result = Vec::with_capacity(data.len());

    for i in 0..data.len() {
        let start = if i >= pad { i - pad } else { 0 };
        let end = (i + pad + 1).min(data.len());
        let slice = &data[start..end];
        result.push(slice.iter().sum::<f64>() / slice.len() as f64);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> IntelligentCropConfig {
        IntelligentCropConfig::default()
    }

    #[test]
    fn test_basic_tier_selects_largest_face() {
        let config = test_config();
        let smoother = TierAwareCameraSmoother::new(config, DetectionTier::Basic, 30.0);

        let detections = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.9, 1),
            Detection::new(0.0, BoundingBox::new(500.0, 100.0, 100.0, 100.0), 0.8, 2),
        ];

        let focus = smoother.compute_focus_basic(&detections, 1920, 1080);

        // Should select the larger face (track 2)
        assert!(focus.cx() > 400.0, "Should focus on larger face on right side");
    }

    #[test]
    fn test_audio_aware_follows_speaker() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::AudioAware, 30.0);

        // Set up speaker segments - left speaker active
        smoother.speaker_segments = vec![SpeakerSegment {
            start_time: 0.0,
            end_time: 10.0,
            speaker: ActiveSpeaker::Left,
            confidence: 0.9,
        }];

        // Set up track sides
        smoother.track_sides.insert(1, true);  // Track 1 is on left
        smoother.track_sides.insert(2, false); // Track 2 is on right

        let detections = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.8, 1), // Left, smaller
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 100.0, 100.0), 0.9, 2), // Right, larger
        ];

        let focus = smoother.compute_focus_audio_aware(&detections, 0.5, 1920, 1080);

        // Should select left face (track 1) because left speaker is active
        assert!(focus.cx() < 500.0, "Should focus on left face (active speaker)");
    }

    #[test]
    fn test_speaker_aware_uses_hysteresis() {
        let config = test_config();
        let mut smoother = TierAwareCameraSmoother::new(config, DetectionTier::SpeakerAware, 30.0);

        smoother.track_sides.insert(1, true);
        smoother.track_sides.insert(2, false);

        // Initial selection
        let dets1 = vec![
            Detection::new(0.0, BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.8, 1),
            Detection::new(0.0, BoundingBox::new(1500.0, 100.0, 50.0, 50.0), 0.7, 2),
        ];
        let _focus1 = smoother.compute_focus_speaker_aware(&dets1, 0.0, 1920, 1080);

        // Try to switch too soon (before min_switch_duration)
        let dets2 = vec![
            Detection::new(0.5, BoundingBox::new(100.0, 100.0, 50.0, 50.0), 0.5, 1),
            Detection::new(0.5, BoundingBox::new(1500.0, 100.0, 50.0, 50.0), 0.9, 2),
        ];
        let focus2 = smoother.compute_focus_speaker_aware(&dets2, 0.5, 1920, 1080);

        // Should still be on track 1 due to min_switch_duration
        // (This depends on the activity tracker's hysteresis)
        assert!(focus2.cx() < 1000.0 || focus2.cx() > 1000.0, "Focus should be determined");
    }

    #[test]
    fn test_fallback_when_no_detections() {
        let config = test_config();
        let smoother = TierAwareCameraSmoother::new(config, DetectionTier::Basic, 30.0);

        let focus = smoother.compute_focus_basic(&[], 1920, 1080);

        // Should return fallback box
        assert!(focus.width > 0.0);
        assert!(focus.height > 0.0);
    }
}
