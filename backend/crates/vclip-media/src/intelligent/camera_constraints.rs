//! Camera motion constraint enforcement.
//!
//! This module provides motion constraint enforcement for camera smoothing,
//! extracted from tier_aware_smoother.rs for better modularity.

use super::config::IntelligentCropConfig;
use super::models::CameraKeyframe;
use super::smoothing_utils::moving_average;

/// Camera constraint enforcer for motion limits.
pub struct CameraConstraintEnforcer {
    config: IntelligentCropConfig,
}

impl CameraConstraintEnforcer {
    /// Create a new constraint enforcer.
    pub fn new(config: IntelligentCropConfig) -> Self {
        Self { config }
    }

    /// Standard motion constraints on keyframes.
    pub fn enforce_constraints(
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

    /// Relaxed motion constraints for speaker-aware tiers.
    /// Allows faster camera movements to track speakers.
    pub fn enforce_constraints_relaxed(
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

    /// Motion constraints permitting instant snap transitions for visual tiers.
    pub fn enforce_constraints_with_snaps(
        &self,
        keyframes: &[CameraKeyframe],
        width: u32,
        height: u32,
    ) -> Vec<CameraKeyframe> {
        if keyframes.len() < 2 {
            return keyframes.to_vec();
        }

        let switch_threshold = compute_switch_threshold(keyframes);
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
            let is_switch = dx.abs() > switch_threshold || dy.abs() > switch_threshold;
            let speed = (dx * dx + dy * dy).sqrt() / dt;

            let (new_cx, new_cy) = if is_switch {
                // Allow instantaneous jump on switch
                (curr.cx, curr.cy)
            } else if speed > relaxed_max_pan_speed {
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

/// Compute a switch threshold scaled to the typical crop width.
pub fn compute_switch_threshold(keyframes: &[CameraKeyframe]) -> f64 {
    if keyframes.is_empty() {
        return 0.0;
    }
    let avg_width: f64 =
        keyframes.iter().map(|kf| kf.width).sum::<f64>() / keyframes.len() as f64;
    avg_width * 0.25
}

/// Light smoothing for individual segments (preserves quick movements).
pub fn smooth_segment_light(keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligent::config::IntelligentCropConfig;

    #[test]
    fn test_standard_constraints() {
        let config = IntelligentCropConfig::default();
        let enforcer = CameraConstraintEnforcer::new(config);

        let keyframes = vec![
            CameraKeyframe::new(0.0, 100.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(0.1, 150.0, 100.0, 200.0, 400.0),
        ];

        let result = enforcer.enforce_constraints(&keyframes, 1920, 1080);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_snap_transitions() {
        let config = IntelligentCropConfig::default();
        let enforcer = CameraConstraintEnforcer::new(config);

        let keyframes = vec![
            CameraKeyframe::new(0.0, 100.0, 100.0, 200.0, 400.0),
            CameraKeyframe::new(0.1, 500.0, 100.0, 200.0, 400.0), // Large jump
        ];

        let result = enforcer.enforce_constraints_with_snaps(&keyframes, 1920, 1080);
        assert_eq!(result.len(), 2);
        // The large jump should be preserved (instant snap)
        assert!((result[1].cx - 500.0).abs() < 50.0);
    }
}
