//! Enhanced camera path smoothing with Gaussian filtering and deadband.
//!
//! This module provides a Virtual Camera model with:
//! - **Gaussian Smoothing**: Weighted average with lookahead for jitter-free motion
//! - **Deadband**: Camera locks in place until subject moves significantly (tripod-like)
//! - **Velocity Limiting**: Max pan speed enforcement for cinematic feel

use super::config::IntelligentCropConfig;
use super::models::CameraKeyframe;
use tracing::debug;

/// Enhanced camera smoother with Gaussian kernel and deadband logic.
///
/// Implements a Virtual Camera model that:
/// 1. Collects raw focus points from face detection
/// 2. Applies Gaussian-weighted smoothing with lookahead
/// 3. Enforces deadband (no motion below threshold)
/// 4. Limits velocity to max_pan_speed
pub struct EnhancedCameraSmoother {
    config: IntelligentCropConfig,
    fps: f64,
    /// Deadband threshold as fraction of frame width (default: 0.05 = 5%)
    deadband_threshold: f64,
    /// Gaussian sigma in seconds (default: ~0.4s for 1-second effective window)
    gaussian_sigma: f64,
}

impl EnhancedCameraSmoother {
    /// Create a new enhanced camera smoother.
    pub fn new(config: IntelligentCropConfig, fps: f64) -> Self {
        Self {
            config,
            fps,
            deadband_threshold: 0.05, // 5% of frame width
            gaussian_sigma: 0.4,      // ~1 second effective window at ±2σ
        }
    }

    /// Create with custom deadband and smoothing parameters.
    pub fn with_params(
        config: IntelligentCropConfig,
        fps: f64,
        deadband_threshold: f64,
        gaussian_sigma: f64,
    ) -> Self {
        Self {
            config,
            fps,
            deadband_threshold,
            gaussian_sigma,
        }
    }

    /// Apply full smoothing pipeline to raw keyframes.
    ///
    /// Pipeline:
    /// 1. Gaussian smoothing (bidirectional lookahead)
    /// 2. Deadband application (lock camera when subject stationary)
    /// 3. Velocity constraint enforcement
    pub fn smooth(&self, raw_keyframes: &[CameraKeyframe], frame_width: u32) -> Vec<CameraKeyframe> {
        if raw_keyframes.len() < 2 {
            return raw_keyframes.to_vec();
        }

        debug!(
            "Enhanced smoothing: {} keyframes, sigma={:.2}s, deadband={:.1}%",
            raw_keyframes.len(),
            self.gaussian_sigma,
            self.deadband_threshold * 100.0
        );

        // Step 1: Gaussian smoothing with lookahead
        let gaussian_smoothed = self.apply_gaussian_smoothing(raw_keyframes);

        // Step 2: Apply deadband (lock camera when small movements)
        let deadband_applied = self.apply_deadband(&gaussian_smoothed, frame_width);

        // Step 3: Enforce velocity constraints
        self.enforce_velocity_constraints(&deadband_applied, frame_width)
    }

    /// Apply Gaussian-weighted smoothing with bidirectional lookahead.
    ///
    /// Formula: P_smooth[t] = Σ(P_raw[t+i] × K[i]) / Σ(K[i])
    /// where K[i] = exp(-i²/(2σ²)) is the Gaussian kernel
    fn apply_gaussian_smoothing(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        let n = keyframes.len();
        if n < 3 {
            return keyframes.to_vec();
        }

        // Compute window size in samples based on sigma and sample rate
        // Window extends to ±3σ for 99.7% coverage
        let sample_duration = if n > 1 {
            (keyframes[n - 1].time - keyframes[0].time) / (n - 1) as f64
        } else {
            1.0 / self.fps
        };

        let window_samples = (3.0 * self.gaussian_sigma / sample_duration).ceil() as usize;
        let window_samples = window_samples.max(1).min(n / 2); // Clamp to reasonable range

        // Pre-compute Gaussian kernel weights
        let kernel = self.compute_gaussian_kernel(window_samples, sample_duration);

        let mut smoothed = Vec::with_capacity(n);

        for i in 0..n {
            let mut sum_cx = 0.0;
            let mut sum_cy = 0.0;
            let mut sum_width = 0.0;
            let mut sum_height = 0.0;
            let mut sum_weights = 0.0;

            // Apply kernel centered at position i
            let start = i.saturating_sub(window_samples);
            let end = (i + window_samples + 1).min(n);

            for j in start..end {
                let offset = (j as i64 - i as i64).unsigned_abs() as usize;
                let weight = if offset < kernel.len() {
                    kernel[offset]
                } else {
                    0.0
                };

                sum_cx += keyframes[j].cx * weight;
                sum_cy += keyframes[j].cy * weight;
                sum_width += keyframes[j].width * weight;
                sum_height += keyframes[j].height * weight;
                sum_weights += weight;
            }

            if sum_weights > 0.0 {
                smoothed.push(CameraKeyframe::new(
                    keyframes[i].time,
                    sum_cx / sum_weights,
                    sum_cy / sum_weights,
                    sum_width / sum_weights,
                    sum_height / sum_weights,
                ));
            } else {
                smoothed.push(keyframes[i]);
            }
        }

        smoothed
    }

    /// Compute Gaussian kernel weights.
    ///
    /// Returns weights for offsets [0, 1, 2, ..., window_samples]
    /// where weight[0] is the center weight.
    fn compute_gaussian_kernel(&self, window_samples: usize, sample_duration: f64) -> Vec<f64> {
        let mut kernel = Vec::with_capacity(window_samples + 1);
        let sigma_samples = self.gaussian_sigma / sample_duration;
        let two_sigma_sq = 2.0 * sigma_samples * sigma_samples;

        for i in 0..=window_samples {
            let weight = (-(i as f64 * i as f64) / two_sigma_sq).exp();
            kernel.push(weight);
        }

        kernel
    }

    /// Apply deadband logic to lock camera when subject movements are small.
    ///
    /// If the face moves less than `deadband_threshold` × frame_width,
    /// the camera stays locked at the last significant position.
    fn apply_deadband(&self, keyframes: &[CameraKeyframe], frame_width: u32) -> Vec<CameraKeyframe> {
        if keyframes.is_empty() {
            return Vec::new();
        }

        let threshold = self.deadband_threshold * frame_width as f64;
        let mut result = Vec::with_capacity(keyframes.len());

        // Start with first keyframe as anchor
        let mut anchor = keyframes[0];
        result.push(anchor);

        for kf in keyframes.iter().skip(1) {
            let dx = (kf.cx - anchor.cx).abs();
            let dy = (kf.cy - anchor.cy).abs();
            let distance = (dx * dx + dy * dy).sqrt();

            if distance > threshold {
                // Movement exceeds deadband - update anchor
                anchor = *kf;
                result.push(*kf);
            } else {
                // Movement within deadband - keep camera locked
                result.push(CameraKeyframe::new(
                    kf.time,
                    anchor.cx,
                    anchor.cy,
                    kf.width,  // Allow zoom changes
                    kf.height,
                ));
            }
        }

        result
    }

    /// Enforce maximum velocity constraints.
    ///
    /// Limits camera pan speed to config.max_pan_speed pixels/second.
    fn enforce_velocity_constraints(
        &self,
        keyframes: &[CameraKeyframe],
        frame_width: u32,
    ) -> Vec<CameraKeyframe> {
        if keyframes.len() < 2 {
            return keyframes.to_vec();
        }

        let mut result = Vec::with_capacity(keyframes.len());
        result.push(keyframes[0]);

        for i in 1..keyframes.len() {
            let prev = &result[i - 1];
            let curr = &keyframes[i];

            let dt = curr.time - prev.time;
            if dt <= 0.0 {
                result.push(*curr);
                continue;
            }

            let dx = curr.cx - prev.cx;
            let dy = curr.cy - prev.cy;
            let distance = (dx * dx + dy * dy).sqrt();
            let speed = distance / dt;

            if speed > self.config.max_pan_speed {
                // Limit speed by scaling the displacement
                let scale = self.config.max_pan_speed / speed;
                let limited_cx = prev.cx + dx * scale;
                let limited_cy = prev.cy + dy * scale;

                // Clamp to frame bounds
                let margin_x = curr.width / 2.0;
                let margin_y = curr.height / 2.0;
                let clamped_cx = limited_cx.max(margin_x).min(frame_width as f64 - margin_x);
                let clamped_cy = limited_cy.max(margin_y).min(frame_width as f64 - margin_y);

                result.push(CameraKeyframe::new(
                    curr.time,
                    clamped_cx,
                    clamped_cy,
                    curr.width,
                    curr.height,
                ));
            } else {
                result.push(*curr);
            }
        }

        result
    }
}

/// Configuration preset for enhanced smoothing.
#[derive(Debug, Clone, Copy)]
pub enum SmoothingPreset {
    /// Cinematic: Heavy smoothing, large deadband (documentary style)
    Cinematic,
    /// Responsive: Light smoothing, small deadband (action content)
    Responsive,
    /// Podcast: Medium smoothing, medium deadband (talking heads)
    Podcast,
}

impl SmoothingPreset {
    /// Get deadband threshold for this preset.
    pub fn deadband_threshold(&self) -> f64 {
        match self {
            SmoothingPreset::Cinematic => 0.08,  // 8% - very stable
            SmoothingPreset::Responsive => 0.03, // 3% - quick response
            SmoothingPreset::Podcast => 0.05,    // 5% - balanced
        }
    }

    /// Get Gaussian sigma for this preset.
    pub fn gaussian_sigma(&self) -> f64 {
        match self {
            SmoothingPreset::Cinematic => 0.6,  // Heavy smoothing
            SmoothingPreset::Responsive => 0.2, // Light smoothing
            SmoothingPreset::Podcast => 0.4,    // Medium smoothing
        }
    }

    /// Create an enhanced smoother with this preset.
    pub fn create_smoother(&self, config: IntelligentCropConfig, fps: f64) -> EnhancedCameraSmoother {
        EnhancedCameraSmoother::with_params(
            config,
            fps,
            self.deadband_threshold(),
            self.gaussian_sigma(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_keyframes(points: &[(f64, f64, f64)]) -> Vec<CameraKeyframe> {
        points
            .iter()
            .map(|(time, cx, cy)| CameraKeyframe::new(*time, *cx, *cy, 100.0, 100.0))
            .collect()
    }

    #[test]
    fn test_gaussian_smoothing_reduces_jitter() {
        let config = IntelligentCropConfig::default();
        let smoother = EnhancedCameraSmoother::new(config, 30.0);

        // Jittery input with high-frequency noise
        let raw = make_keyframes(&[
            (0.0, 500.0, 300.0),
            (0.033, 510.0, 305.0), // noise
            (0.066, 495.0, 295.0), // noise
            (0.1, 505.0, 302.0),   // noise
            (0.133, 500.0, 300.0),
            (0.166, 508.0, 298.0), // noise
            (0.2, 500.0, 300.0),
        ]);

        let smoothed = smoother.apply_gaussian_smoothing(&raw);

        // Smoothed values should be closer to 500, 300
        for kf in &smoothed {
            assert!((kf.cx - 500.0).abs() < 20.0, "cx={} too far from mean", kf.cx);
            assert!((kf.cy - 300.0).abs() < 15.0, "cy={} too far from mean", kf.cy);
        }
    }

    #[test]
    fn test_deadband_locks_camera() {
        let config = IntelligentCropConfig::default();
        let smoother = EnhancedCameraSmoother::with_params(config, 30.0, 0.05, 0.4);

        // Small movements should be ignored (frame width = 1920, 5% = 96px)
        let raw = make_keyframes(&[
            (0.0, 500.0, 300.0),
            (0.1, 510.0, 305.0),  // +10, +5 - within deadband
            (0.2, 520.0, 310.0),  // +20, +10 - within deadband
            (0.3, 530.0, 315.0),  // +30, +15 - still within
        ]);

        let result = smoother.apply_deadband(&raw, 1920);

        // All frames should stay at anchor (500, 300)
        for kf in &result {
            assert_eq!(kf.cx, 500.0, "Camera should stay locked");
            assert_eq!(kf.cy, 300.0, "Camera should stay locked");
        }
    }

    #[test]
    fn test_deadband_allows_large_movement() {
        let config = IntelligentCropConfig::default();
        let smoother = EnhancedCameraSmoother::with_params(config, 30.0, 0.05, 0.4);

        // Large movement should trigger camera update
        let raw = make_keyframes(&[
            (0.0, 500.0, 300.0),
            (0.1, 700.0, 400.0), // +200, +100 - exceeds deadband (96px)
        ]);

        let result = smoother.apply_deadband(&raw, 1920);

        assert_eq!(result[0].cx, 500.0);
        assert_eq!(result[1].cx, 700.0, "Camera should follow large movement");
    }

    #[test]
    fn test_velocity_limiting() {
        let mut config = IntelligentCropConfig::default();
        config.max_pan_speed = 100.0; // 100 px/s max

        let smoother = EnhancedCameraSmoother::new(config, 30.0);

        // Movement of 500px in 1 second = 500 px/s (should be limited)
        let raw = make_keyframes(&[
            (0.0, 500.0, 300.0),
            (1.0, 1000.0, 300.0), // 500px in 1s
        ]);

        let result = smoother.enforce_velocity_constraints(&raw, 1920);

        // Should only move 100px (max speed × dt)
        let dx = (result[1].cx - result[0].cx).abs();
        assert!(dx <= 101.0, "Movement {} exceeds max speed", dx);
    }

    #[test]
    fn test_full_pipeline() {
        let config = IntelligentCropConfig::default();
        let smoother = SmoothingPreset::Podcast.create_smoother(config, 30.0);

        let raw = make_keyframes(&[
            (0.0, 500.0, 300.0),
            (0.1, 520.0, 310.0),
            (0.2, 510.0, 305.0),
            (0.3, 505.0, 302.0),
        ]);

        let smoothed = smoother.smooth(&raw, 1920);

        // Should have same number of keyframes
        assert_eq!(smoothed.len(), raw.len());

        // All should be valid
        for kf in &smoothed {
            assert!(kf.cx > 0.0 && kf.cx < 1920.0);
        }
    }
}
