//! Trajectory optimization for cinematic camera motion.
//!
//! This module provides a unified trajectory optimizer that can use either:
//! - **L1 Optimal paths**: Promotes sparsity for cinematographic segments (default)
//! - **L2 Polynomial fitting**: Fast baseline with regularized least-squares
//!
//! The L1 method produces professional-quality camera motion with distinct
//! static and panning segments, while L2 provides faster processing with
//! good (but not cinematic) smoothness.

use super::camera_mode::{median, CameraMode};
use super::config::{CinematicConfig, TrajectoryMethod};
use super::l1_optimizer::L1TrajectoryOptimizer;
use crate::intelligent::CameraKeyframe;
use tracing::warn;

/// Trajectory optimizer for smooth camera paths.
///
/// Supports both L1 (optimal cinematographic) and L2 (polynomial) methods.
/// L1 is the default for best visual quality.
pub struct TrajectoryOptimizer {
    /// Trajectory optimization method.
    pub method: TrajectoryMethod,

    /// Degree of polynomial for L2 fitting (3 = cubic, good balance).
    pub polynomial_degree: usize,

    /// Regularization weight for L2 smoothness (0-1).
    pub smoothness_weight: f64,

    /// Output sample rate in fps.
    pub sample_rate: f64,
}

impl TrajectoryOptimizer {
    /// Create optimizer from config.
    pub fn new(config: &CinematicConfig) -> Self {
        Self {
            method: config.trajectory_method,
            polynomial_degree: config.polynomial_degree,
            smoothness_weight: config.smoothness_weight,
            sample_rate: config.output_sample_rate,
        }
    }

    /// Create optimizer with explicit parameters (uses L1 by default).
    pub fn with_params(degree: usize, smoothness: f64, sample_rate: f64) -> Self {
        Self {
            method: TrajectoryMethod::L1Optimal,
            polynomial_degree: degree,
            smoothness_weight: smoothness,
            sample_rate,
        }
    }

    /// Create optimizer with specific method.
    pub fn with_method(method: TrajectoryMethod, degree: usize, smoothness: f64, sample_rate: f64) -> Self {
        Self {
            method,
            polynomial_degree: degree,
            smoothness_weight: smoothness,
            sample_rate,
        }
    }

    /// Optimize trajectory based on camera mode.
    ///
    /// # Arguments
    /// * `keyframes` - Input keyframes from detection
    /// * `mode` - Camera mode (determines optimization strategy)
    ///
    /// # Returns
    /// Smoothed keyframes sampled at the output frame rate.
    pub fn optimize(&self, keyframes: &[CameraKeyframe], mode: CameraMode) -> Vec<CameraKeyframe> {
        if keyframes.is_empty() {
            return Vec::new();
        }

        if keyframes.len() == 1 {
            return keyframes.to_vec();
        }

        match self.method {
            TrajectoryMethod::L1Optimal => self.optimize_l1(keyframes, mode),
            TrajectoryMethod::L2Polynomial => self.optimize_l2(keyframes, mode),
        }
    }

    /// Optimize using L1 optimal paths (with L2 fallback on failure).
    fn optimize_l1(&self, keyframes: &[CameraKeyframe], mode: CameraMode) -> Vec<CameraKeyframe> {
        // For stationary mode, L1 isn't needed - just use median
        if matches!(mode, CameraMode::Stationary) {
            return self.apply_stationary(keyframes);
        }

        let l1_optimizer = L1TrajectoryOptimizer::new(self.sample_rate);
        
        match l1_optimizer.optimize(keyframes) {
            Ok(result) => result,
            Err(e) => {
                warn!("L1 trajectory optimization failed: {}, falling back to L2", e);
                self.optimize_l2(keyframes, mode)
            }
        }
    }

    /// Optimize using L2 polynomial fitting.
    fn optimize_l2(&self, keyframes: &[CameraKeyframe], mode: CameraMode) -> Vec<CameraKeyframe> {
        match mode {
            CameraMode::Stationary => self.apply_stationary(keyframes),
            CameraMode::Panning => self.apply_panning(keyframes),
            CameraMode::Tracking => self.apply_tracking(keyframes),
        }
    }

    /// Apply stationary mode: lock to median position.
    fn apply_stationary(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        let cx_values: Vec<f64> = keyframes.iter().map(|k| k.cx).collect();
        let cy_values: Vec<f64> = keyframes.iter().map(|k| k.cy).collect();
        let width_values: Vec<f64> = keyframes.iter().map(|k| k.width).collect();
        let height_values: Vec<f64> = keyframes.iter().map(|k| k.height).collect();

        let median_cx = median(&cx_values);
        let median_cy = median(&cy_values);
        let median_width = median(&width_values);
        let median_height = median(&height_values);

        let start_time = keyframes.first().map(|k| k.time).unwrap_or(0.0);
        let end_time = keyframes.last().map(|k| k.time).unwrap_or(0.0);
        let duration = end_time - start_time;

        if duration <= 0.0 {
            return vec![CameraKeyframe {
                time: start_time,
                cx: median_cx,
                cy: median_cy,
                width: median_width,
                height: median_height,
            }];
        }

        // Sample at output rate
        let num_samples = ((duration * self.sample_rate).ceil() as usize).max(2);
        let dt = duration / (num_samples - 1) as f64;

        (0..num_samples)
            .map(|i| CameraKeyframe {
                time: start_time + i as f64 * dt,
                cx: median_cx,
                cy: median_cy,
                width: median_width,
                height: median_height,
            })
            .collect()
    }

    /// Apply panning mode: linear interpolation from start to end.
    fn apply_panning(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        let first = &keyframes[0];
        let last = &keyframes[keyframes.len() - 1];

        let start_time = first.time;
        let end_time = last.time;
        let duration = end_time - start_time;

        if duration <= 0.0 {
            return keyframes.to_vec();
        }

        let num_samples = ((duration * self.sample_rate).ceil() as usize).max(2);
        let dt = duration / (num_samples - 1) as f64;

        (0..num_samples)
            .map(|i| {
                let t = i as f64 / (num_samples - 1) as f64;
                CameraKeyframe {
                    time: start_time + i as f64 * dt,
                    cx: lerp(first.cx, last.cx, t),
                    cy: lerp(first.cy, last.cy, t),
                    width: lerp(first.width, last.width, t),
                    height: lerp(first.height, last.height, t),
                }
            })
            .collect()
    }

    /// Apply tracking mode: polynomial trajectory optimization.
    fn apply_tracking(&self, keyframes: &[CameraKeyframe]) -> Vec<CameraKeyframe> {
        let start_time = keyframes.first().map(|k| k.time).unwrap_or(0.0);
        let end_time = keyframes.last().map(|k| k.time).unwrap_or(0.0);
        let duration = end_time - start_time;

        if duration <= 0.0 {
            return keyframes.to_vec();
        }

        // Normalize time to [0, 1]
        let times: Vec<f64> = keyframes
            .iter()
            .map(|k| (k.time - start_time) / duration)
            .collect();
        let cx_values: Vec<f64> = keyframes.iter().map(|k| k.cx).collect();
        let cy_values: Vec<f64> = keyframes.iter().map(|k| k.cy).collect();
        let width_values: Vec<f64> = keyframes.iter().map(|k| k.width).collect();
        let height_values: Vec<f64> = keyframes.iter().map(|k| k.height).collect();

        // Fit polynomials for each dimension
        let cx_coeffs = self.fit_polynomial(&times, &cx_values);
        let cy_coeffs = self.fit_polynomial(&times, &cy_values);
        let width_coeffs = self.fit_polynomial(&times, &width_values);
        let height_coeffs = self.fit_polynomial(&times, &height_values);

        // Sample at output rate
        let num_samples = ((duration * self.sample_rate).ceil() as usize).max(2);

        (0..num_samples)
            .map(|i| {
                let t = i as f64 / (num_samples - 1) as f64;
                CameraKeyframe {
                    time: start_time + t * duration,
                    cx: eval_polynomial(&cx_coeffs, t),
                    cy: eval_polynomial(&cy_coeffs, t),
                    width: eval_polynomial(&width_coeffs, t).max(1.0),
                    height: eval_polynomial(&height_coeffs, t).max(1.0),
                }
            })
            .collect()
    }

    /// Fit a polynomial to data points using regularized least squares.
    ///
    /// Minimizes: ||Ax - y||² + λ||Dx||²
    /// where A is the Vandermonde matrix, D is the second derivative matrix,
    /// and λ is the smoothness weight.
    fn fit_polynomial(&self, times: &[f64], values: &[f64]) -> Vec<f64> {
        let n = times.len();
        let degree = self.polynomial_degree.min(n - 1); // Can't fit higher degree than points

        if n <= 1 {
            return vec![values.first().copied().unwrap_or(0.0)];
        }

        if degree == 0 {
            // Constant polynomial = mean
            let mean = values.iter().sum::<f64>() / n as f64;
            return vec![mean];
        }

        // Build Vandermonde matrix A where A[i,j] = t[i]^j
        let mut a = vec![vec![0.0; degree + 1]; n];
        for i in 0..n {
            let t = times[i];
            let mut t_power = 1.0;
            for j in 0..=degree {
                a[i][j] = t_power;
                t_power *= t;
            }
        }

        // Compute A^T * A
        let mut ata = vec![vec![0.0; degree + 1]; degree + 1];
        for i in 0..=degree {
            for j in 0..=degree {
                let mut sum = 0.0;
                for k in 0..n {
                    sum += a[k][i] * a[k][j];
                }
                ata[i][j] = sum;
            }
        }

        // Add regularization for smoothness (penalize second derivative)
        // D is the second derivative operator: d²f/dt² = sum(j*(j-1)*c_j*t^(j-2))
        // We penalize ||D*coeffs||² which adds to diagonal elements
        let lambda = self.smoothness_weight * n as f64;
        for j in 2..=degree {
            // Penalize coefficient of t^j based on j*(j-1) (second derivative factor)
            let penalty = (j * (j - 1)) as f64;
            ata[j][j] += lambda * penalty * penalty;
        }

        // Compute A^T * y
        let mut aty = vec![0.0; degree + 1];
        for j in 0..=degree {
            let mut sum = 0.0;
            for i in 0..n {
                sum += a[i][j] * values[i];
            }
            aty[j] = sum;
        }

        // Solve (A^T*A + λ*D^T*D) * coeffs = A^T * y
        // Using simple Gaussian elimination (for small matrices)
        solve_linear_system(&ata, &aty).unwrap_or_else(|| {
            // Fallback: return linear interpolation coefficients
            let first = values.first().copied().unwrap_or(0.0);
            let last = values.last().copied().unwrap_or(0.0);
            vec![first, last - first] // y = first + (last-first)*t
        })
    }
}

/// Linear interpolation.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Evaluate polynomial at point t.
/// coeffs[i] is the coefficient of t^i.
fn eval_polynomial(coeffs: &[f64], t: f64) -> f64 {
    let mut result = 0.0;
    let mut t_power = 1.0;
    for &c in coeffs {
        result += c * t_power;
        t_power *= t;
    }
    result
}

/// Solve linear system Ax = b using Gaussian elimination with partial pivoting.
fn solve_linear_system(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    if n == 0 || a.len() != n || a.iter().any(|row| row.len() != n) {
        return None;
    }

    // Create augmented matrix
    let mut aug: Vec<Vec<f64>> = a
        .iter()
        .zip(b.iter())
        .map(|(row, &bi)| {
            let mut new_row = row.clone();
            new_row.push(bi);
            new_row
        })
        .collect();

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_row = col;
        let mut max_val = aug[col][col].abs();
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }

        if max_val < 1e-12 {
            return None; // Singular matrix
        }

        // Swap rows
        aug.swap(col, max_row);

        // Eliminate
        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            for j in col..=n {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }

    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_keyframes(points: &[(f64, f64, f64, f64)]) -> Vec<CameraKeyframe> {
        points
            .iter()
            .map(|(t, cx, cy, w)| CameraKeyframe {
                time: *t,
                cx: *cx,
                cy: *cy,
                width: *w,
                height: *w * 0.5625,
            })
            .collect()
    }

    #[test]
    fn test_stationary_optimization() {
        let optimizer = TrajectoryOptimizer::with_params(3, 0.3, 10.0);
        let keyframes = make_keyframes(&[
            (0.0, 500.0, 300.0, 200.0),
            (0.5, 510.0, 305.0, 195.0),
            (1.0, 490.0, 295.0, 205.0),
        ]);

        let result = optimizer.optimize(&keyframes, CameraMode::Stationary);

        // All keyframes should have same position (median)
        assert!(!result.is_empty());
        let first = &result[0];
        for kf in &result {
            assert!((kf.cx - first.cx).abs() < 0.001);
            assert!((kf.cy - first.cy).abs() < 0.001);
        }
    }

    #[test]
    fn test_panning_optimization() {
        let optimizer = TrajectoryOptimizer::with_params(3, 0.3, 10.0);
        let keyframes = make_keyframes(&[(0.0, 200.0, 300.0, 200.0), (1.0, 800.0, 400.0, 300.0)]);

        let result = optimizer.optimize(&keyframes, CameraMode::Panning);

        // Check linear interpolation
        assert!(!result.is_empty());
        let first = &result[0];
        let last = &result[result.len() - 1];

        assert!((first.cx - 200.0).abs() < 1.0);
        assert!((last.cx - 800.0).abs() < 1.0);
    }

    #[test]
    fn test_tracking_optimization() {
        // Use L2 explicitly since this test validates L2-specific resampling
        let optimizer = TrajectoryOptimizer::with_method(
            TrajectoryMethod::L2Polynomial,
            3, 0.3, 10.0
        );
        let keyframes = make_keyframes(&[
            (0.0, 200.0, 300.0, 200.0),
            (0.5, 500.0, 350.0, 250.0),
            (1.0, 800.0, 400.0, 300.0),
        ]);

        let result = optimizer.optimize(&keyframes, CameraMode::Tracking);

        // Should produce smooth trajectory
        assert!(!result.is_empty());
        assert!(result.len() >= 10); // At least 10 samples for 1 second at 10fps

        // Values should be reasonable (bounded by input range)
        for kf in &result {
            assert!(kf.cx >= 100.0 && kf.cx <= 900.0);
            assert!(kf.cy >= 200.0 && kf.cy <= 500.0);
        }
    }

    #[test]
    fn test_polynomial_fit() {
        let optimizer = TrajectoryOptimizer::with_params(2, 0.0, 30.0);

        // Fit quadratic: y = 1 + 2t + 3t²
        let times = vec![0.0, 0.5, 1.0];
        let values = vec![1.0, 2.75, 6.0]; // 1+0+0, 1+1+0.75, 1+2+3

        let coeffs = optimizer.fit_polynomial(&times, &values);

        // Check coefficients are close to [1, 2, 3]
        assert!((coeffs[0] - 1.0).abs() < 0.1);
        assert!((coeffs[1] - 2.0).abs() < 0.1);
        assert!((coeffs[2] - 3.0).abs() < 0.1);
    }

    #[test]
    fn test_empty_keyframes() {
        let optimizer = TrajectoryOptimizer::with_params(3, 0.3, 30.0);
        let result = optimizer.optimize(&[], CameraMode::Tracking);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_keyframe() {
        let optimizer = TrajectoryOptimizer::with_params(3, 0.3, 30.0);
        let keyframes = make_keyframes(&[(0.0, 500.0, 300.0, 200.0)]);
        let result = optimizer.optimize(&keyframes, CameraMode::Tracking);
        assert_eq!(result.len(), 1);
        assert!((result[0].cx - 500.0).abs() < 0.001);
    }

    #[test]
    fn test_eval_polynomial() {
        // y = 1 + 2t + 3t²
        let coeffs = vec![1.0, 2.0, 3.0];
        assert!((eval_polynomial(&coeffs, 0.0) - 1.0).abs() < 0.001);
        assert!((eval_polynomial(&coeffs, 1.0) - 6.0).abs() < 0.001);
        assert!((eval_polynomial(&coeffs, 0.5) - 2.75).abs() < 0.001);
    }
}
