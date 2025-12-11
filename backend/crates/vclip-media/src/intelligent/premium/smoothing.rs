//! Camera smoothing algorithms for premium intelligent_speaker style.
//!
//! Provides exponential smoothing with dead-zone hysteresis and velocity constraints.

use super::config::PremiumSpeakerConfig;
use super::target_selector::FocusPoint;
use crate::intelligent::models::CameraKeyframe;

/// Camera state for temporal smoothing.
#[derive(Debug, Clone, Copy)]
pub struct CameraState {
    /// Smoothed center X
    pub cx: f64,
    /// Smoothed center Y
    pub cy: f64,
    /// Smoothed width
    pub width: f64,
    /// Smoothed height
    pub height: f64,
    /// Current velocity X (pixels/second)
    pub vx: f64,
    /// Current velocity Y (pixels/second)
    pub vy: f64,
    /// Last update time
    pub time: f64,
}

impl CameraState {
    /// Create a new camera state.
    pub fn new(cx: f64, cy: f64, width: f64, height: f64, time: f64) -> Self {
        Self {
            cx,
            cy,
            width,
            height,
            vx: 0.0,
            vy: 0.0,
            time,
        }
    }

    /// Create from a focus point.
    pub fn from_focus(focus: &FocusPoint, time: f64) -> Self {
        Self::new(focus.cx, focus.cy, focus.width, focus.height, time)
    }

    /// Convert to camera keyframe.
    pub fn to_keyframe(&self) -> CameraKeyframe {
        CameraKeyframe::new(self.time, self.cx, self.cy, self.width, self.height)
    }
}

/// Camera smoother with EMA, dead-zone, and velocity constraints.
///
/// Implements a Virtual Camera model that:
/// 1. Applies exponential moving average smoothing
/// 2. Enforces dead-zone (camera locks until subject moves significantly)
/// 3. Limits velocity and acceleration for cinematic motion
pub struct PremiumSmoother {
    config: PremiumSpeakerConfig,
    fps: f64,
    /// Current smoothed camera state
    state: Option<CameraState>,
    /// Anchor position for dead-zone
    anchor: Option<(f64, f64)>,
    /// Frame dimensions for bounds checking
    frame_width: u32,
    frame_height: u32,
}

impl PremiumSmoother {
    /// Create a new premium smoother.
    pub fn new(config: PremiumSpeakerConfig, fps: f64, frame_width: u32, frame_height: u32) -> Self {
        Self {
            config,
            fps,
            state: None,
            anchor: None,
            frame_width,
            frame_height,
        }
    }

    /// Smooth a focus point and return the smoothed camera keyframe.
    pub fn smooth(&mut self, focus: &FocusPoint, time: f64) -> CameraKeyframe {
        // Initialize state if needed
        if self.state.is_none() {
            let state = CameraState::from_focus(focus, time);
            self.state = Some(state);
            self.anchor = Some((focus.cx, focus.cy));
            return state.to_keyframe();
        }

        let prev_state = self.state.unwrap();
        let dt = (time - prev_state.time).max(1e-6);

        // Apply dead-zone
        let (target_cx, target_cy) = self.apply_dead_zone(focus.cx, focus.cy);

        // Compute raw displacement
        let dx = target_cx - prev_state.cx;
        let dy = target_cy - prev_state.cy;

        // Apply EMA smoothing
        let alpha = self.config.compute_ema_alpha(self.fps);
        let smoothed_cx = prev_state.cx + alpha * dx;
        let smoothed_cy = prev_state.cy + alpha * dy;

        // Apply velocity constraints
        let (final_cx, final_cy, new_vx, new_vy) =
            self.apply_velocity_constraints(&prev_state, smoothed_cx, smoothed_cy, dt);

        // Smooth dimensions
        let smoothed_width = prev_state.width + alpha * (focus.width - prev_state.width);
        let smoothed_height = prev_state.height + alpha * (focus.height - prev_state.height);

        // Update state
        let new_state = CameraState {
            cx: final_cx,
            cy: final_cy,
            width: smoothed_width,
            height: smoothed_height,
            vx: new_vx,
            vy: new_vy,
            time,
        };
        self.state = Some(new_state);

        new_state.to_keyframe()
    }

    /// Apply dead-zone hysteresis.
    fn apply_dead_zone(&mut self, target_cx: f64, target_cy: f64) -> (f64, f64) {
        let (dz_x, dz_y) = self.config.dead_zone_pixels(self.frame_width, self.frame_height);

        match self.anchor {
            Some((anchor_x, anchor_y)) => {
                let dx = (target_cx - anchor_x).abs();
                let dy = (target_cy - anchor_y).abs();

                if dx > dz_x || dy > dz_y {
                    // Outside dead-zone - update anchor and follow target
                    self.anchor = Some((target_cx, target_cy));
                    (target_cx, target_cy)
                } else {
                    // Inside dead-zone - stay at anchor
                    (anchor_x, anchor_y)
                }
            }
            None => {
                self.anchor = Some((target_cx, target_cy));
                (target_cx, target_cy)
            }
        }
    }

    /// Apply velocity and acceleration constraints.
    fn apply_velocity_constraints(
        &self,
        prev: &CameraState,
        target_cx: f64,
        target_cy: f64,
        dt: f64,
    ) -> (f64, f64, f64, f64) {
        let max_speed = self.config.max_pan_speed_px_per_sec;
        let max_accel = self.config.max_acceleration_px_per_sec2;

        // Compute desired velocity
        let desired_vx = (target_cx - prev.cx) / dt;
        let desired_vy = (target_cy - prev.cy) / dt;

        // Apply acceleration limit
        let (limited_vx, limited_vy) = if max_accel > 0.0 {
            self.limit_acceleration(prev.vx, prev.vy, desired_vx, desired_vy, dt, max_accel)
        } else {
            (desired_vx, desired_vy)
        };

        // Apply speed limit
        let (final_vx, final_vy) = self.limit_speed(limited_vx, limited_vy, max_speed);

        // Compute final position
        let final_cx = prev.cx + final_vx * dt;
        let final_cy = prev.cy + final_vy * dt;

        // Clamp to frame bounds
        let (clamped_cx, clamped_cy) = self.clamp_to_bounds(final_cx, final_cy, prev.width, prev.height);

        (clamped_cx, clamped_cy, final_vx, final_vy)
    }

    /// Limit acceleration.
    fn limit_acceleration(
        &self,
        prev_vx: f64,
        prev_vy: f64,
        desired_vx: f64,
        desired_vy: f64,
        dt: f64,
        max_accel: f64,
    ) -> (f64, f64) {
        let dvx = desired_vx - prev_vx;
        let dvy = desired_vy - prev_vy;
        let accel_magnitude = (dvx * dvx + dvy * dvy).sqrt() / dt;

        if accel_magnitude > max_accel {
            let scale = max_accel * dt / (dvx * dvx + dvy * dvy).sqrt();
            (prev_vx + dvx * scale, prev_vy + dvy * scale)
        } else {
            (desired_vx, desired_vy)
        }
    }

    /// Limit speed.
    fn limit_speed(&self, vx: f64, vy: f64, max_speed: f64) -> (f64, f64) {
        let speed = (vx * vx + vy * vy).sqrt();
        if speed > max_speed {
            let scale = max_speed / speed;
            (vx * scale, vy * scale)
        } else {
            (vx, vy)
        }
    }

    /// Clamp position to frame bounds.
    fn clamp_to_bounds(&self, cx: f64, cy: f64, width: f64, height: f64) -> (f64, f64) {
        let margin_x = width / 2.0;
        let margin_y = height / 2.0;
        let clamped_cx = cx.max(margin_x).min(self.frame_width as f64 - margin_x);
        let clamped_cy = cy.max(margin_y).min(self.frame_height as f64 - margin_y);
        (clamped_cx, clamped_cy)
    }

    /// Reset smoother state.
    pub fn reset(&mut self) {
        self.state = None;
        self.anchor = None;
    }

    /// Soft reset for scene change adaptation.
    pub fn soft_reset(&mut self, new_focus: &FocusPoint, time: f64) {
        let reset_factor = self.config.scene_change_reset_factor;

        if let Some(ref mut state) = self.state {
            state.cx = state.cx * (1.0 - reset_factor) + new_focus.cx * reset_factor;
            state.cy = state.cy * (1.0 - reset_factor) + new_focus.cy * reset_factor;
            state.vx *= 1.0 - reset_factor;
            state.vy *= 1.0 - reset_factor;
            state.time = time;
            self.anchor = Some((state.cx, state.cy));
        }
    }

    /// Get current camera state.
    pub fn current_state(&self) -> Option<&CameraState> {
        self.state.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_focus(cx: f64, cy: f64) -> FocusPoint {
        FocusPoint {
            cx,
            cy,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
        }
    }

    #[test]
    fn test_initial_state() {
        let config = PremiumSpeakerConfig::default();
        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        let focus = make_focus(500.0, 400.0);
        let kf = smoother.smooth(&focus, 0.0);

        assert!((kf.cx - 500.0).abs() < 1.0);
        assert!((kf.cy - 400.0).abs() < 1.0);
    }

    #[test]
    fn test_dead_zone_stability() {
        let mut config = PremiumSpeakerConfig::default();
        config.dead_zone_fraction_x = 0.1; // 192px for 1920 width

        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        // Initial position
        let focus1 = make_focus(500.0, 400.0);
        let kf1 = smoother.smooth(&focus1, 0.0);

        // Small movement within dead-zone
        let focus2 = make_focus(550.0, 420.0); // +50px, within 192px dead-zone
        let kf2 = smoother.smooth(&focus2, 0.1);

        // Camera should stay relatively stable
        let dx = (kf2.cx - kf1.cx).abs();
        assert!(dx < 100.0, "Camera moved too much: {}", dx);
    }

    #[test]
    fn test_velocity_limiting() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_pan_speed_px_per_sec = 100.0;

        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        // Initial position
        let focus1 = make_focus(200.0, 400.0);
        smoother.smooth(&focus1, 0.0);

        // Large jump
        let focus2 = make_focus(1500.0, 400.0);
        let kf2 = smoother.smooth(&focus2, 0.1);

        // Movement should be limited
        let dx = (kf2.cx - 200.0).abs();
        assert!(dx < 200.0, "Velocity not limited: {} px", dx);
    }

    #[test]
    fn test_soft_reset() {
        let config = PremiumSpeakerConfig::default();
        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        // Establish state
        let focus1 = make_focus(200.0, 400.0);
        smoother.smooth(&focus1, 0.0);

        // Soft reset toward new position
        let new_focus = make_focus(1500.0, 400.0);
        smoother.soft_reset(&new_focus, 0.5);

        // State should be partially moved toward new focus
        let state = smoother.current_state().unwrap();
        assert!(state.cx > 200.0, "Should have moved toward new focus");
        assert!(state.cx < 1500.0, "Should not have fully jumped");
    }
}
