//! FFmpeg progress parsing.

use serde::{Deserialize, Serialize};

/// Progress information from FFmpeg.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FfmpegProgress {
    /// Current frame number
    pub frame: u64,
    /// Current FPS
    pub fps: f64,
    /// Output time in milliseconds
    pub out_time_ms: i64,
    /// Output time as string (HH:MM:SS.microseconds)
    pub out_time: String,
    /// Encoding speed (e.g., 1.5 = 1.5x realtime)
    pub speed: f64,
    /// Whether encoding is complete
    pub is_complete: bool,
}

impl FfmpegProgress {
    /// Calculate progress percentage given total duration in milliseconds.
    pub fn percentage(&self, total_duration_ms: i64) -> f64 {
        if total_duration_ms <= 0 {
            return 0.0;
        }
        ((self.out_time_ms as f64 / total_duration_ms as f64) * 100.0).min(100.0)
    }

    /// Estimate time remaining in seconds.
    pub fn eta_seconds(&self, total_duration_ms: i64) -> Option<f64> {
        if self.speed <= 0.0 || self.out_time_ms <= 0 {
            return None;
        }

        let remaining_ms = total_duration_ms - self.out_time_ms;
        if remaining_ms <= 0 {
            return Some(0.0);
        }

        // Time remaining = remaining duration / speed
        Some((remaining_ms as f64 / 1000.0) / self.speed)
    }
}

/// Callback type for progress updates.
pub type ProgressCallback = Box<dyn Fn(FfmpegProgress) + Send + 'static>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_percentage() {
        let progress = FfmpegProgress {
            out_time_ms: 5000,
            ..Default::default()
        };

        assert!((progress.percentage(10000) - 50.0).abs() < 0.01);
        assert!((progress.percentage(5000) - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_eta_calculation() {
        let progress = FfmpegProgress {
            out_time_ms: 5000,
            speed: 2.0, // 2x realtime
            ..Default::default()
        };

        // 5 seconds remaining at 2x speed = 2.5 seconds ETA
        let eta = progress.eta_seconds(10000).unwrap();
        assert!((eta - 2.5).abs() < 0.01);
    }
}
