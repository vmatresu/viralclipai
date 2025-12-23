//! Inference Backend Selection and Management
//!
//! Implements OpenVINO-first backend policy with safe fallback to OpenCV DNN.
//! Automatically selects the optimal backend based on availability and CPU features.
//!
//! # Backend Priority
//! 1. **OpenVINO** (`DNN_BACKEND_INFERENCE_ENGINE`) - Best performance
//! 2. **OpenCV DNN** (`DNN_BACKEND_OPENCV`) - Universal fallback
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::backend::{BackendSelector, InferenceBackend};
//!
//! let (backend, metrics) = BackendSelector::select_optimal(960, 540)?;
//! println!("Selected backend: {:?}", backend);
//! println!("Initialization time: {}ms", metrics.initialization_time_ms);
//! ```

use super::cpu_features::{CpuFeatures, InferenceTier};
use crate::error::{MediaError, MediaResult};
use std::time::Instant;
use tracing::{debug, info, warn};

#[cfg(feature = "opencv")]
use opencv::dnn::{
    DNN_BACKEND_DEFAULT, DNN_BACKEND_INFERENCE_ENGINE, DNN_BACKEND_OPENCV, DNN_TARGET_CPU,
};

/// Available inference backends for face detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceBackend {
    /// OpenVINO Inference Engine (optimal performance)
    OpenVino,
    /// OpenCV DNN module (universal fallback)
    OpenCvDnn,
    /// Default backend (let OpenCV choose)
    Default,
}

impl std::fmt::Display for InferenceBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceBackend::OpenVino => write!(f, "openvino"),
            InferenceBackend::OpenCvDnn => write!(f, "opencv_dnn"),
            InferenceBackend::Default => write!(f, "default"),
        }
    }
}

impl InferenceBackend {
    /// Get OpenCV backend ID constant.
    #[cfg(feature = "opencv")]
    pub fn backend_id(&self) -> i32 {
        match self {
            InferenceBackend::OpenVino => DNN_BACKEND_INFERENCE_ENGINE,
            InferenceBackend::OpenCvDnn => DNN_BACKEND_OPENCV,
            InferenceBackend::Default => DNN_BACKEND_DEFAULT,
        }
    }

    /// Get OpenCV target ID (always CPU for our use case).
    #[cfg(feature = "opencv")]
    pub fn target_id(&self) -> i32 {
        DNN_TARGET_CPU
    }
}

/// Metrics collected during backend initialization.
#[derive(Debug, Clone)]
pub struct BackendMetrics {
    /// Selected backend type
    pub backend: InferenceBackend,
    /// Time taken to initialize the backend (ms)
    pub initialization_time_ms: u64,
    /// CPU features detected
    pub cpu_tier: InferenceTier,
    /// Whether VNNI is available for INT8 acceleration
    pub uses_vnni: bool,
    /// OpenCV build information (truncated)
    pub opencv_build_info: String,
}

impl BackendMetrics {
    /// Log metrics for diagnostics.
    pub fn log(&self) {
        info!(
            backend = %self.backend,
            init_time_ms = self.initialization_time_ms,
            cpu_tier = %self.cpu_tier,
            uses_vnni = self.uses_vnni,
            "Inference backend initialized"
        );
    }
}

/// Backend selector implementing OpenVINO-first policy.
pub struct BackendSelector;

impl BackendSelector {
    /// Select the optimal backend for face inference.
    ///
    /// Tries backends in order of preference:
    /// 1. OpenVINO (if available and working)
    /// 2. OpenCV DNN (fallback)
    ///
    /// # Arguments
    /// * `input_width` - Inference input width
    /// * `input_height` - Inference input height
    ///
    /// # Returns
    /// Tuple of (selected backend, initialization metrics)
    #[cfg(feature = "opencv")]
    pub fn select_optimal(
        input_width: i32,
        input_height: i32,
    ) -> MediaResult<(InferenceBackend, BackendMetrics)> {
        let cpu_features = CpuFeatures::detect();
        cpu_features.log_capabilities();

        let start = Instant::now();

        // Try OpenVINO first
        match Self::try_openvino(input_width, input_height) {
            Ok(()) => {
                let metrics = BackendMetrics {
                    backend: InferenceBackend::OpenVino,
                    initialization_time_ms: start.elapsed().as_millis() as u64,
                    cpu_tier: cpu_features.inference_tier(),
                    uses_vnni: cpu_features.has_vnni(),
                    opencv_build_info: Self::get_opencv_build_info(),
                };
                metrics.log();
                return Ok((InferenceBackend::OpenVino, metrics));
            }
            Err(e) => {
                debug!("OpenVINO backend not available: {}", e);
            }
        }

        // Fallback to OpenCV DNN
        match Self::try_opencv_dnn(input_width, input_height) {
            Ok(()) => {
                let metrics = BackendMetrics {
                    backend: InferenceBackend::OpenCvDnn,
                    initialization_time_ms: start.elapsed().as_millis() as u64,
                    cpu_tier: cpu_features.inference_tier(),
                    uses_vnni: cpu_features.has_vnni(),
                    opencv_build_info: Self::get_opencv_build_info(),
                };
                warn!(
                    "Using OpenCV DNN backend (OpenVINO not available). \
                     Performance may be reduced."
                );
                metrics.log();
                return Ok((InferenceBackend::OpenCvDnn, metrics));
            }
            Err(e) => {
                debug!("OpenCV DNN backend failed: {}", e);
            }
        }

        // Last resort: default backend
        let metrics = BackendMetrics {
            backend: InferenceBackend::Default,
            initialization_time_ms: start.elapsed().as_millis() as u64,
            cpu_tier: cpu_features.inference_tier(),
            uses_vnni: cpu_features.has_vnni(),
            opencv_build_info: Self::get_opencv_build_info(),
        };
        warn!(
            "Using default backend. Both OpenVINO and OpenCV DNN failed. \
             Performance may be significantly reduced."
        );
        metrics.log();
        Ok((InferenceBackend::Default, metrics))
    }

    /// Try to initialize OpenVINO backend.
    #[cfg(feature = "opencv")]
    fn try_openvino(input_width: i32, input_height: i32) -> MediaResult<()> {
        use opencv::objdetect::FaceDetectorYN;
        use opencv::prelude::FaceDetectorYNTrait;

        // Find a model to test with
        let (_, model_path) = super::model_config::get_resolved_model().map_err(|e| {
            MediaError::detection_failed(format!("No YuNet model: {}", e))
        })?;
        let model_path_str = model_path.to_string_lossy().into_owned();

        debug!(
            "Testing OpenVINO backend with model: {}, size: {}x{}",
            model_path_str, input_width, input_height
        );

        // Try to create detector with OpenVINO backend
        let mut detector = FaceDetectorYN::create(
            &model_path_str,
            "",
            opencv::core::Size::new(input_width, input_height),
            0.3,
            0.3,
            10,
            DNN_BACKEND_INFERENCE_ENGINE,
            DNN_TARGET_CPU,
        )
        .map_err(|e| MediaError::detection_failed(format!("OpenVINO init failed: {}", e)))?;

        // Verify detector is valid
        let size = detector.get_input_size().map_err(|e| {
            MediaError::detection_failed(format!("OpenVINO detector invalid: {}", e))
        })?;

        if size.width != input_width || size.height != input_height {
            return Err(MediaError::detection_failed(format!(
                "OpenVINO detector size mismatch: expected {}x{}, got {}x{}",
                input_width, input_height, size.width, size.height
            )));
        }

        info!("OpenVINO backend verified successfully");
        Ok(())
    }

    /// Try to initialize OpenCV DNN backend.
    #[cfg(feature = "opencv")]
    fn try_opencv_dnn(input_width: i32, input_height: i32) -> MediaResult<()> {
        use opencv::objdetect::FaceDetectorYN;
        use opencv::prelude::FaceDetectorYNTrait;

        let (_, model_path) = super::model_config::get_resolved_model().map_err(|e| {
            MediaError::detection_failed(format!("No YuNet model: {}", e))
        })?;
        let model_path_str = model_path.to_string_lossy().into_owned();

        debug!(
            "Testing OpenCV DNN backend with model: {}, size: {}x{}",
            model_path_str, input_width, input_height
        );

        let mut detector = FaceDetectorYN::create(
            &model_path_str,
            "",
            opencv::core::Size::new(input_width, input_height),
            0.3,
            0.3,
            10,
            DNN_BACKEND_OPENCV,
            DNN_TARGET_CPU,
        )
        .map_err(|e| MediaError::detection_failed(format!("OpenCV DNN init failed: {}", e)))?;

        let size = detector.get_input_size().map_err(|e| {
            MediaError::detection_failed(format!("OpenCV DNN detector invalid: {}", e))
        })?;

        if size.width != input_width || size.height != input_height {
            return Err(MediaError::detection_failed(format!(
                "OpenCV DNN detector size mismatch: expected {}x{}, got {}x{}",
                input_width, input_height, size.width, size.height
            )));
        }

        info!("OpenCV DNN backend verified successfully");
        Ok(())
    }

    /// Get truncated OpenCV build information for logging.
    #[cfg(feature = "opencv")]
    fn get_opencv_build_info() -> String {
        use opencv::core::get_build_information;

        match get_build_information() {
            Ok(info) => {
                // Extract key lines (OpenVINO, CPU baseline, TBB)
                let lines: Vec<&str> = info
                    .lines()
                    .filter(|l| {
                        l.contains("OpenVINO")
                            || l.contains("CPU_BASELINE")
                            || l.contains("TBB")
                            || l.contains("OpenCV version")
                    })
                    .take(5)
                    .collect();
                lines.join(" | ")
            }
            Err(_) => "Unknown".to_string(),
        }
    }

    /// Placeholder for non-opencv builds.
    #[cfg(not(feature = "opencv"))]
    pub fn select_optimal(
        _input_width: i32,
        _input_height: i32,
    ) -> MediaResult<(InferenceBackend, BackendMetrics)> {
        let cpu_features = CpuFeatures::detect();
        Ok((
            InferenceBackend::Default,
            BackendMetrics {
                backend: InferenceBackend::Default,
                initialization_time_ms: 0,
                cpu_tier: cpu_features.inference_tier(),
                uses_vnni: cpu_features.has_vnni(),
                opencv_build_info: "OpenCV not enabled".to_string(),
            },
        ))
    }

    #[cfg(not(feature = "opencv"))]
    fn get_opencv_build_info() -> String {
        "OpenCV not enabled".to_string()
    }
}

/// Configuration for backend selection behavior.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// Preferred backend (None = auto-select)
    pub preferred_backend: Option<InferenceBackend>,
    /// Whether to log detailed backend info at startup
    pub log_startup_info: bool,
    /// Whether to fail if preferred backend is unavailable
    pub require_preferred: bool,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            preferred_backend: None, // Auto-select (OpenVINO-first)
            log_startup_info: true,
            require_preferred: false,
        }
    }
}

impl BackendConfig {
    /// Create config that requires OpenVINO backend.
    pub fn require_openvino() -> Self {
        Self {
            preferred_backend: Some(InferenceBackend::OpenVino),
            log_startup_info: true,
            require_preferred: true,
        }
    }

    /// Create config for testing with OpenCV DNN.
    pub fn opencv_dnn_only() -> Self {
        Self {
            preferred_backend: Some(InferenceBackend::OpenCvDnn),
            log_startup_info: true,
            require_preferred: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_display() {
        assert_eq!(format!("{}", InferenceBackend::OpenVino), "openvino");
        assert_eq!(format!("{}", InferenceBackend::OpenCvDnn), "opencv_dnn");
        assert_eq!(format!("{}", InferenceBackend::Default), "default");
    }

    #[test]
    fn test_backend_config_default() {
        let config = BackendConfig::default();
        assert!(config.preferred_backend.is_none());
        assert!(config.log_startup_info);
        assert!(!config.require_preferred);
    }

    #[test]
    fn test_backend_config_require_openvino() {
        let config = BackendConfig::require_openvino();
        assert_eq!(config.preferred_backend, Some(InferenceBackend::OpenVino));
        assert!(config.require_preferred);
    }

    #[cfg(feature = "opencv")]
    #[test]
    fn test_backend_selection() {
        // This test may fail if no YuNet model is present
        // That's expected in CI without models
        let result = BackendSelector::select_optimal(640, 480);

        match result {
            Ok((backend, metrics)) => {
                println!("Selected backend: {:?}", backend);
                println!("CPU tier: {:?}", metrics.cpu_tier);
                println!("Init time: {}ms", metrics.initialization_time_ms);
                assert!(metrics.initialization_time_ms < 10000); // Sanity check
            }
            Err(e) => {
                println!("Backend selection failed (expected without models): {}", e);
            }
        }
    }
}
