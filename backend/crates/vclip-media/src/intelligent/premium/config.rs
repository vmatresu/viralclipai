//! Configuration for the premium intelligent_speaker style.
//!
//! Centralizes all tunable parameters for the Active Speaker mode,
//! avoiding magic numbers scattered throughout the code.

use serde::{Deserialize, Serialize};

/// Configuration for the premium `intelligent_speaker` style.
///
/// All parameters have sensible defaults but can be tuned for specific use cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PremiumSpeakerConfig {
    // === Camera Motion Constraints ===
    /// Maximum camera pan speed in pixels per second.
    /// Higher values allow faster tracking, lower values feel more cinematic.
    /// Default: 400.0 (balanced for speaker tracking)
    pub max_pan_speed_px_per_sec: f64,

    /// Maximum camera acceleration in pixels per second squared.
    /// Prevents abrupt starts/stops. Set to 0.0 to disable.
    /// Default: 800.0
    pub max_acceleration_px_per_sec2: f64,

    // === Temporal Smoothing ===
    /// Smoothing time window in milliseconds.
    /// Longer windows = smoother but more latent camera.
    /// Default: 400 (0.4 seconds)
    pub smoothing_time_window_ms: u32,

    /// Exponential moving average alpha (0.0-1.0).
    /// Higher = more responsive, lower = smoother.
    /// Computed from smoothing_time_window_ms if not set.
    /// Default: 0.15
    pub ema_alpha: f64,

    // === Dead-zone (Hysteresis) ===
    /// Dead-zone as fraction of frame width (x-axis).
    /// Camera won't move if subject stays within this zone.
    /// Default: 0.05 (5% of frame width)
    pub dead_zone_fraction_x: f64,

    /// Dead-zone as fraction of frame height (y-axis).
    /// Default: 0.08 (8% of frame height - more tolerance vertically)
    pub dead_zone_fraction_y: f64,

    // === Vertical Bias (Framing) ===
    /// Vertical bias fraction for eye placement.
    /// 0.0 = center, positive = eyes higher in frame.
    /// Default: 0.15 (eyes in upper third)
    pub vertical_bias_fraction: f64,

    /// Headroom ratio as fraction of crop height.
    /// Default: 0.12
    pub headroom_ratio: f64,

    // === Subject Selection ===
    /// Minimum dwell time before switching primary subject (milliseconds).
    /// Prevents rapid ping-ponging between speakers.
    /// Default: 1200 (1.2 seconds)
    pub primary_subject_dwell_ms: u32,

    /// Activity margin required to switch subjects (0.0-1.0).
    /// New subject must be this much more active to trigger switch.
    /// Default: 0.25 (25% more active)
    pub switch_activity_margin: f64,

    /// Weight for face size in subject scoring.
    /// Larger faces are considered more prominent.
    /// Default: 0.4
    pub weight_face_size: f64,

    /// Weight for detection confidence in subject scoring.
    /// Default: 0.3
    pub weight_confidence: f64,

    /// Weight for mouth activity (speaking) in subject scoring.
    /// Default: 0.3
    pub weight_mouth_activity: f64,

    // === Scene Change Detection ===
    /// Enable scene change detection for faster adaptation.
    /// Default: true
    pub enable_scene_detection: bool,

    /// Threshold for scene change detection (0.0-1.0).
    /// Fraction of detections that must change for scene cut.
    /// Default: 0.6
    pub scene_change_threshold: f64,

    /// Smoothing reset factor on scene change (0.0-1.0).
    /// 1.0 = full reset, 0.0 = no reset.
    /// Default: 0.7
    pub scene_change_reset_factor: f64,

    // === Aspect Ratio Framing ===
    /// Minimum horizontal padding as fraction of crop width.
    /// Ensures context around speaker.
    /// Default: 0.08
    pub min_horizontal_padding: f64,

    /// Maximum zoom factor relative to source.
    /// Prevents over-tight crops.
    /// Default: 2.5
    pub max_zoom_factor: f64,

    /// Minimum zoom factor.
    /// Default: 1.0
    pub min_zoom_factor: f64,

    /// Safe margin from crop edge as fraction of crop size.
    /// Default: 0.05
    pub safe_margin: f64,
}

impl Default for PremiumSpeakerConfig {
    fn default() -> Self {
        Self {
            // Camera motion
            max_pan_speed_px_per_sec: 400.0,
            max_acceleration_px_per_sec2: 800.0,

            // Temporal smoothing
            smoothing_time_window_ms: 400,
            ema_alpha: 0.15,

            // Dead-zone
            dead_zone_fraction_x: 0.05,
            dead_zone_fraction_y: 0.08,

            // Vertical bias
            vertical_bias_fraction: 0.15,
            headroom_ratio: 0.12,

            // Subject selection
            primary_subject_dwell_ms: 1200,
            switch_activity_margin: 0.25,
            weight_face_size: 0.4,
            weight_confidence: 0.3,
            weight_mouth_activity: 0.3,

            // Scene change
            enable_scene_detection: true,
            scene_change_threshold: 0.6,
            scene_change_reset_factor: 0.7,

            // Aspect ratio framing
            min_horizontal_padding: 0.08,
            max_zoom_factor: 2.5,
            min_zoom_factor: 1.0,
            safe_margin: 0.05,
        }
    }
}

impl PremiumSpeakerConfig {
    /// Configuration optimized for podcast/interview content.
    /// Slower, more stable camera with longer dwell times.
    pub fn podcast() -> Self {
        Self {
            max_pan_speed_px_per_sec: 300.0,
            smoothing_time_window_ms: 500,
            ema_alpha: 0.12,
            dead_zone_fraction_x: 0.07,
            dead_zone_fraction_y: 0.10,
            primary_subject_dwell_ms: 1500,
            switch_activity_margin: 0.30,
            ..Default::default()
        }
    }

    /// Configuration optimized for dynamic content (presentations, vlogs).
    /// More responsive camera with shorter dwell times.
    pub fn dynamic() -> Self {
        Self {
            max_pan_speed_px_per_sec: 500.0,
            smoothing_time_window_ms: 300,
            ema_alpha: 0.20,
            dead_zone_fraction_x: 0.04,
            dead_zone_fraction_y: 0.06,
            primary_subject_dwell_ms: 800,
            switch_activity_margin: 0.20,
            ..Default::default()
        }
    }

    /// Configuration for single-speaker content.
    /// Very stable camera, minimal switching.
    pub fn single_speaker() -> Self {
        Self {
            max_pan_speed_px_per_sec: 250.0,
            smoothing_time_window_ms: 600,
            ema_alpha: 0.10,
            dead_zone_fraction_x: 0.08,
            dead_zone_fraction_y: 0.12,
            primary_subject_dwell_ms: 2000,
            switch_activity_margin: 0.40,
            ..Default::default()
        }
    }

    /// Compute EMA alpha from smoothing time window and frame rate.
    pub fn compute_ema_alpha(&self, fps: f64) -> f64 {
        // EMA alpha = 1 - exp(-dt / tau)
        // where tau = smoothing_time_window_ms / 1000
        let dt = 1.0 / fps;
        let tau = self.smoothing_time_window_ms as f64 / 1000.0;
        1.0 - (-dt / tau).exp()
    }

    /// Get dead-zone in pixels for given frame dimensions.
    pub fn dead_zone_pixels(&self, frame_width: u32, frame_height: u32) -> (f64, f64) {
        (
            frame_width as f64 * self.dead_zone_fraction_x,
            frame_height as f64 * self.dead_zone_fraction_y,
        )
    }

    /// Get primary subject dwell time in seconds.
    pub fn dwell_time_seconds(&self) -> f64 {
        self.primary_subject_dwell_ms as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PremiumSpeakerConfig::default();
        assert!(config.max_pan_speed_px_per_sec > 0.0);
        assert!(config.ema_alpha > 0.0 && config.ema_alpha < 1.0);
        assert!(config.dead_zone_fraction_x > 0.0);
        assert!(config.primary_subject_dwell_ms > 0);
    }

    #[test]
    fn test_ema_alpha_computation() {
        let config = PremiumSpeakerConfig::default();
        let alpha_30fps = config.compute_ema_alpha(30.0);
        let alpha_60fps = config.compute_ema_alpha(60.0);

        // Higher FPS should have lower alpha per frame
        assert!(alpha_60fps < alpha_30fps);
        assert!(alpha_30fps > 0.0 && alpha_30fps < 1.0);
    }

    #[test]
    fn test_dead_zone_pixels() {
        let config = PremiumSpeakerConfig::default();
        let (dz_x, dz_y) = config.dead_zone_pixels(1920, 1080);

        assert!(dz_x > 0.0);
        assert!(dz_y > 0.0);
        assert_eq!(dz_x, 1920.0 * 0.05);
        assert_eq!(dz_y, 1080.0 * 0.08);
    }

    #[test]
    fn test_presets() {
        let podcast = PremiumSpeakerConfig::podcast();
        let dynamic = PremiumSpeakerConfig::dynamic();

        // Podcast should be slower/more stable
        assert!(podcast.max_pan_speed_px_per_sec < dynamic.max_pan_speed_px_per_sec);
        assert!(podcast.primary_subject_dwell_ms > dynamic.primary_subject_dwell_ms);
    }
}
