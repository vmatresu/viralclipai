//! Smoothing and statistical helper functions for camera path planning.
//!
//! This module contains reusable mathematical utilities used by the camera
//! smoothing algorithms, including:
//! - Basic statistics (mean, median, standard deviation)
//! - Moving average filter
//! - Camera keyframe smoothing algorithms

use super::models::CameraKeyframe;

// === Statistical Functions ===

/// Calculate the arithmetic mean of a slice of values.
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate the standard deviation of a slice of values.
pub fn std_deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = mean(values);
    let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Calculate the median of a slice of values.
pub fn median(values: &[f64]) -> f64 {
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

// === Filtering Functions ===

/// Apply a moving average filter to a data series.
///
/// Uses symmetric padding at boundaries to maintain array length.
///
/// # Arguments
/// * `data` - The input data series
/// * `window` - Window size (should be odd for symmetric filtering)
pub fn moving_average(data: &[f64], window: usize) -> Vec<f64> {
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

// === Keyframe Smoothing Functions ===

/// Smooth keyframes for static camera mode.
///
/// Uses median values across all keyframes to produce a stable, locked-off camera.
pub fn smooth_static(keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
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
///
/// Uses moving average with configurable window size.
///
/// # Arguments
/// * `keyframes` - The input keyframes
/// * `smoothing_window` - Time window for smoothing in seconds
pub fn smooth_tracking(keyframes: &[CameraKeyframe], smoothing_window: f64) -> Vec<CameraKeyframe> {
    if keyframes.len() < 3 {
        return keyframes.to_vec();
    }

    let duration = keyframes.last().unwrap().time - keyframes.first().unwrap().time;
    let sample_rate = if duration > 0.0 {
        keyframes.len() as f64 / duration
    } else {
        1.0
    };

    let mut window_samples = (smoothing_window * sample_rate) as usize;
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

/// Light smoothing for individual segments (preserves quick movements).
///
/// Uses a small window size (3 samples) for minimal smoothing.
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

    #[test]
    fn test_mean() {
        assert_eq!(mean(&[]), 0.0);
        assert_eq!(mean(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(mean(&[10.0]), 10.0);
    }

    #[test]
    fn test_median() {
        assert_eq!(median(&[]), 0.0);
        assert_eq!(median(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
    }

    #[test]
    fn test_std_deviation() {
        assert_eq!(std_deviation(&[]), 0.0);
        assert_eq!(std_deviation(&[5.0]), 0.0);
        // [2, 4, 4, 4, 5, 5, 7, 9] -> mean=5, variance=4, std=2
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert!((std_deviation(&values) - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_moving_average() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let smoothed = moving_average(&data, 3);
        assert_eq!(smoothed.len(), 5);
        // Check middle value (i=2): avg of [2,3,4] = 3.0
        assert!((smoothed[2] - 3.0).abs() < 0.01);
    }
}
