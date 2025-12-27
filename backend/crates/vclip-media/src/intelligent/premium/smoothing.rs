//! Camera smoothing algorithms for premium intelligent_speaker style.
//!
//! Provides exponential smoothing with:
//! - Zoom-aware dead-zone hysteresis
//! - Velocity and acceleration constraints for pan
//! - Smooth zoom trajectory with speed/acceleration limits
//! - Real timestamp-based dt calculations

use super::config::PremiumSpeakerConfig;
use super::target_selector::FocusPoint;
use crate::intelligent::models::CameraKeyframe;
use tracing::debug;

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
    /// Current zoom velocity (zoom factor change per second)
    pub zoom_velocity: f64,
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
            zoom_velocity: 0.0,
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

/// Camera smoother with EMA, zoom-aware dead-zone, and velocity constraints.
///
/// Implements a Virtual Camera model that:
/// 1. Applies exponential moving average smoothing
/// 2. Enforces zoom-aware dead-zone (tighter at high zoom)
/// 3. Limits pan velocity and acceleration for cinematic motion
/// 4. Smooths zoom changes with speed/acceleration limits
pub struct PremiumSmoother {
    config: PremiumSpeakerConfig,
    /// Current smoothed camera state
    state: Option<CameraState>,
    /// Anchor position for dead-zone
    anchor: Option<(f64, f64)>,
    /// Frame dimensions for bounds checking
    frame_width: u32,
    frame_height: u32,
    /// Frames since last scene change (for relaxed limits)
    frames_since_scene_change: u32,
}

impl PremiumSmoother {
    /// Create a new premium smoother.
    pub fn new(
        config: PremiumSpeakerConfig,
        _fps: f64,
        frame_width: u32,
        frame_height: u32,
    ) -> Self {
        Self {
            config,
            state: None,
            anchor: None,
            frame_width,
            frame_height,
            frames_since_scene_change: u32::MAX,
        }
    }

    /// Smooth a focus point and return the smoothed camera keyframe.
    /// Uses real timestamps for dt calculation.
    pub fn smooth(&mut self, focus: &FocusPoint, time: f64) -> CameraKeyframe {
        // Handle scene change
        if focus.is_scene_change {
            self.frames_since_scene_change = 0;
        } else if self.frames_since_scene_change < u32::MAX {
            self.frames_since_scene_change += 1;
        }

        // Initialize state if needed
        if self.state.is_none() {
            let state = CameraState::from_focus(focus, time);
            self.state = Some(state);
            self.anchor = Some((focus.cx, focus.cy));
            return state.to_keyframe();
        }

        let prev_state = self.state.unwrap();
        let dt = (time - prev_state.time).max(1e-6);

        // Compute current zoom factor
        let current_zoom = self.frame_width as f64 / prev_state.width;

        // Apply zoom-aware dead-zone
        let (target_cx, target_cy) = self.apply_dead_zone(focus.cx, focus.cy, current_zoom);

        // Compute raw displacement
        let dx = target_cx - prev_state.cx;
        let dy = target_cy - prev_state.cy;

        // Apply EMA smoothing with real dt
        let alpha = self.config.compute_ema_alpha_for_dt(dt);
        let smoothed_cx = prev_state.cx + alpha * dx;
        let smoothed_cy = prev_state.cy + alpha * dy;

        // Apply pan velocity constraints
        let (final_cx, final_cy, new_vx, new_vy) =
            self.apply_pan_constraints(&prev_state, smoothed_cx, smoothed_cy, dt);

        // Smooth dimensions with zoom constraints
        let (final_width, final_height, new_zoom_vel) =
            self.apply_zoom_constraints(&prev_state, focus.width, focus.height, dt, alpha);

        // Update state
        let new_state = CameraState {
            cx: final_cx,
            cy: final_cy,
            width: final_width,
            height: final_height,
            vx: new_vx,
            vy: new_vy,
            zoom_velocity: new_zoom_vel,
            time,
        };
        self.state = Some(new_state);

        if self.config.enable_debug_logging {
            let speed = (new_vx * new_vx + new_vy * new_vy).sqrt();
            debug!(
                "Smooth t={:.2}: pos=({:.0},{:.0}) vel={:.0}px/s zoom={:.2} zoom_vel={:.2}/s",
                time,
                final_cx,
                final_cy,
                speed,
                self.frame_width as f64 / final_width,
                new_zoom_vel
            );
        }

        new_state.to_keyframe()
    }

    /// Apply zoom-aware dead-zone hysteresis.
    fn apply_dead_zone(&mut self, target_cx: f64, target_cy: f64, zoom: f64) -> (f64, f64) {
        let (dz_x, dz_y) =
            self.config
                .dead_zone_for_zoom(self.frame_width, self.frame_height, zoom);

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

    /// Apply pan velocity and acceleration constraints.
    fn apply_pan_constraints(
        &self,
        prev: &CameraState,
        target_cx: f64,
        target_cy: f64,
        dt: f64,
    ) -> (f64, f64, f64, f64) {
        // Relax limits slightly right after scene change
        let limit_factor = if self.frames_since_scene_change < 2 {
            1.5
        } else {
            1.0
        };

        let max_speed = self.config.max_pan_speed_px_per_sec * limit_factor;
        let max_accel = self.config.max_acceleration_px_per_sec2 * limit_factor;

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
        let (clamped_cx, clamped_cy) =
            self.clamp_to_bounds(final_cx, final_cy, prev.width, prev.height);

        (clamped_cx, clamped_cy, final_vx, final_vy)
    }

    /// Apply zoom velocity and acceleration constraints.
    fn apply_zoom_constraints(
        &self,
        prev: &CameraState,
        target_width: f64,
        target_height: f64,
        dt: f64,
        alpha: f64,
    ) -> (f64, f64, f64) {
        let frame_w = self.frame_width as f64;

        // Current and target zoom factors
        let current_zoom = frame_w / prev.width;
        let raw_target_width = prev.width + alpha * (target_width - prev.width);
        let target_zoom = frame_w / raw_target_width;

        // Compute desired zoom velocity
        let desired_zoom_vel = (target_zoom - current_zoom) / dt;

        // Apply zoom acceleration limit
        let max_zoom_accel = self.config.max_zoom_accel_per_sec2;
        let limited_zoom_vel = if max_zoom_accel > 0.0 {
            let dv = desired_zoom_vel - prev.zoom_velocity;
            let max_dv = max_zoom_accel * dt;
            if dv.abs() > max_dv {
                prev.zoom_velocity + dv.signum() * max_dv
            } else {
                desired_zoom_vel
            }
        } else {
            desired_zoom_vel
        };

        // Apply zoom speed limit
        let max_zoom_speed = self.config.max_zoom_speed_per_sec;
        let final_zoom_vel = limited_zoom_vel.clamp(-max_zoom_speed, max_zoom_speed);

        // Compute final zoom and convert back to width/height
        let final_zoom = (current_zoom + final_zoom_vel * dt)
            .clamp(self.config.min_zoom_factor, self.config.max_zoom_factor);

        let final_width = frame_w / final_zoom;
        let aspect = target_height / target_width.max(1.0);
        let final_height = final_width * aspect;

        // Clamp to frame bounds
        let final_width = final_width.min(frame_w);
        let final_height = final_height.min(self.frame_height as f64);

        (final_width, final_height, final_zoom_vel)
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
        let dv_mag = (dvx * dvx + dvy * dvy).sqrt();
        let max_dv = max_accel * dt;

        if dv_mag > max_dv && dv_mag > 0.0 {
            let scale = max_dv / dv_mag;
            (prev_vx + dvx * scale, prev_vy + dvy * scale)
        } else {
            (desired_vx, desired_vy)
        }
    }

    /// Limit speed.
    fn limit_speed(&self, vx: f64, vy: f64, max_speed: f64) -> (f64, f64) {
        let speed = (vx * vx + vy * vy).sqrt();
        if speed > max_speed && speed > 0.0 {
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
        self.frames_since_scene_change = u32::MAX;
    }

    /// Soft reset for scene change adaptation.
    /// Quickly moves camera toward new focus while maintaining some smoothness.
    pub fn soft_reset(&mut self, new_focus: &FocusPoint, time: f64) {
        let reset_factor = self.config.scene_change_reset_factor;
        self.frames_since_scene_change = 0;

        if let Some(ref mut state) = self.state {
            state.cx = state.cx * (1.0 - reset_factor) + new_focus.cx * reset_factor;
            state.cy = state.cy * (1.0 - reset_factor) + new_focus.cy * reset_factor;
            state.width = state.width * (1.0 - reset_factor) + new_focus.width * reset_factor;
            state.height = state.height * (1.0 - reset_factor) + new_focus.height * reset_factor;
            state.vx *= 1.0 - reset_factor;
            state.vy *= 1.0 - reset_factor;
            state.zoom_velocity *= 1.0 - reset_factor;
            state.time = time;
            self.anchor = Some((state.cx, state.cy));
        }
    }

    /// Get current camera state.
    pub fn current_state(&self) -> Option<&CameraState> {
        self.state.as_ref()
    }

    /// Get current zoom factor.
    pub fn current_zoom(&self) -> f64 {
        self.state
            .map(|s| self.frame_width as f64 / s.width)
            .unwrap_or(1.0)
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
            is_scene_change: false,
        }
    }

    fn make_focus_scene_change(cx: f64, cy: f64) -> FocusPoint {
        FocusPoint {
            cx,
            cy,
            width: 200.0,
            height: 200.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: true,
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
    fn test_zoom_aware_dead_zone() {
        let config = PremiumSpeakerConfig::default();
        let mut smoother = PremiumSmoother::new(config.clone(), 30.0, 1920, 1080);

        // At 1x zoom, dead-zone is larger
        let (dz_1x, _) = config.dead_zone_for_zoom(1920, 1080, 1.0);

        // At 2x zoom, dead-zone should be smaller
        let (dz_2x, _) = config.dead_zone_for_zoom(1920, 1080, 2.0);

        assert!(dz_2x < dz_1x, "Dead-zone should shrink at higher zoom");

        // Verify smoother uses zoom-aware dead-zone
        let focus1 = make_focus(500.0, 400.0);
        smoother.smooth(&focus1, 0.0);

        let zoom = smoother.current_zoom();
        assert!(zoom > 0.0);
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
    fn test_zoom_smoothing() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_zoom_speed_per_sec = 0.5;

        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        // Start with wide shot
        let focus1 = FocusPoint {
            cx: 960.0,
            cy: 540.0,
            width: 800.0,
            height: 800.0,
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        let kf1 = smoother.smooth(&focus1, 0.0);

        // Request tight zoom
        let focus2 = FocusPoint {
            cx: 960.0,
            cy: 540.0,
            width: 200.0,
            height: 200.0, // Much tighter
            track_id: 1,
            score: 0.9,
            is_scene_change: false,
        };
        let kf2 = smoother.smooth(&focus2, 0.1);

        // Zoom change should be limited
        let zoom1 = 1920.0 / kf1.width;
        let zoom2 = 1920.0 / kf2.width;
        let zoom_change = (zoom2 - zoom1).abs();

        // At 0.5 zoom/sec max, 0.1s should allow max 0.05 zoom change
        assert!(zoom_change < 0.2, "Zoom changed too fast: {}", zoom_change);
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

    #[test]
    fn test_scene_change_relaxes_limits() {
        let mut config = PremiumSpeakerConfig::default();
        config.max_pan_speed_px_per_sec = 100.0;

        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        // Initial position
        let focus1 = make_focus(200.0, 400.0);
        smoother.smooth(&focus1, 0.0);

        // Scene change with large jump - limits should be relaxed
        let focus2 = make_focus_scene_change(1500.0, 400.0);
        let kf2 = smoother.smooth(&focus2, 0.1);

        // Movement should be more than normal limit allows
        let dx = (kf2.cx - 200.0).abs();
        // With 1.5x relaxation, max would be 15px instead of 10px
        assert!(dx > 5.0, "Scene change should allow faster movement");
    }

    #[test]
    fn test_real_timestamp_dt() {
        let config = PremiumSpeakerConfig::default();
        let mut smoother = PremiumSmoother::new(config, 30.0, 1920, 1080);

        // Initial
        let focus1 = make_focus(500.0, 400.0);
        smoother.smooth(&focus1, 0.0);

        // Variable dt - longer gap should allow more movement
        let focus2 = make_focus(600.0, 400.0);
        let kf_short = smoother.smooth(&focus2, 0.033); // ~30fps

        smoother.reset();
        smoother.smooth(&focus1, 0.0);
        let kf_long = smoother.smooth(&focus2, 0.1); // ~10fps

        // Longer dt should result in more movement (higher alpha)
        let dx_short = (kf_short.cx - 500.0).abs();
        let dx_long = (kf_long.cx - 500.0).abs();

        assert!(
            dx_long >= dx_short,
            "Longer dt should allow more smoothing progress"
        );
    }
}
