//! YuNet Model Configuration
//!
//! Provides explicit model variant selection with performance characteristics
//! and configurable model discovery paths.
//!
//! # Architecture
//!
//! Model selection follows explicit priority:
//! 1. Environment variable `YUNET_MODEL_VARIANT` (int8bq, int8, fp32)
//! 2. Configured variant in `ModelConfig`
//! 3. Auto-detection preferring fastest available (int8bq > int8 > fp32)
//!
//! # Performance Characteristics
//!
//! | Variant | Speed | Accuracy | Size |
//! |---------|-------|----------|------|
//! | INT8-BQ | ~6ms  | 0.8845 AP | 120KB |
//! | INT8    | ~8ms  | 0.8810 AP | 98KB |
//! | FP32    | ~25ms | 0.8844 AP | 227KB |

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// Model variant with explicit performance characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelVariant {
    /// Block-quantized INT8 - fastest, recommended for production
    #[default]
    Int8BlockQuantized,
    /// Standard INT8 quantization
    Int8,
    /// Full precision FP32 - slowest but baseline accuracy
    Fp32,
    /// 2022 model for OpenCV <4.8 compatibility
    Legacy2022,
}

impl ModelVariant {
    /// Expected inference time per frame.
    pub const fn expected_ms_per_frame(&self) -> u32 {
        match self {
            Self::Int8BlockQuantized => 6,
            Self::Int8 => 8,
            Self::Fp32 => 25,
            Self::Legacy2022 => 30,
        }
    }

    /// Model accuracy (AP on WIDER FACE easy set).
    pub const fn accuracy_ap(&self) -> f32 {
        match self {
            Self::Int8BlockQuantized => 0.8845,
            Self::Int8 => 0.8810,
            Self::Fp32 => 0.8844,
            Self::Legacy2022 => 0.834,
        }
    }

    /// Human-readable name for logging.
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Int8BlockQuantized => "2023mar INT8-BQ",
            Self::Int8 => "2023mar INT8",
            Self::Fp32 => "2023mar FP32",
            Self::Legacy2022 => "2022mar Legacy",
        }
    }

    /// Model filename suffix pattern.
    pub const fn filename_pattern(&self) -> &'static str {
        match self {
            Self::Int8BlockQuantized => "face_detection_yunet_2023mar_int8bq.onnx",
            Self::Int8 => "face_detection_yunet_2023mar_int8.onnx",
            Self::Fp32 => "face_detection_yunet_2023mar.onnx",
            Self::Legacy2022 => "face_detection_yunet_2022mar.onnx",
        }
    }

    /// Parse from environment variable or string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "int8bq" | "int8-bq" | "block-quantized" => Some(Self::Int8BlockQuantized),
            "int8" | "quantized" => Some(Self::Int8),
            "fp32" | "float32" | "full" => Some(Self::Fp32),
            "legacy" | "2022" | "2022mar" => Some(Self::Legacy2022),
            _ => None,
        }
    }

    /// All variants in preference order (fastest first).
    pub const ALL_BY_SPEED: &'static [Self] = &[
        Self::Int8BlockQuantized,
        Self::Int8,
        Self::Fp32,
        Self::Legacy2022,
    ];
}

impl std::fmt::Display for ModelVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (~{}ms, {:.3} AP)",
            self.display_name(),
            self.expected_ms_per_frame(),
            self.accuracy_ap()
        )
    }
}

/// Model search paths in priority order.
#[derive(Debug, Clone)]
pub struct ModelSearchPaths {
    /// Base directories to search for models.
    pub directories: Vec<PathBuf>,
}

impl Default for ModelSearchPaths {
    fn default() -> Self {
        Self {
            directories: vec![
                // Production: Docker container paths
                PathBuf::from("/app/backend/models/face_detection/yunet"),
                PathBuf::from("/app/models/face_detection/yunet"),
                // Legacy flat paths
                PathBuf::from("/app/models"),
                // Development: relative paths
                PathBuf::from("./backend/models/face_detection/yunet"),
                // System paths
                PathBuf::from("/usr/share/opencv/models"),
            ],
        }
    }
}

impl ModelSearchPaths {
    /// Find the path to a specific model variant.
    pub fn find_variant(&self, variant: ModelVariant) -> Option<PathBuf> {
        let filename = variant.filename_pattern();

        for dir in &self.directories {
            let path = dir.join(filename);
            if path.exists() {
                debug!("Found {} at {}", variant.display_name(), path.display());
                return Some(path);
            }
        }

        None
    }

    /// Find the best available model (fastest variant that exists).
    pub fn find_best_available(&self) -> Option<(ModelVariant, PathBuf)> {
        for variant in ModelVariant::ALL_BY_SPEED {
            if let Some(path) = self.find_variant(*variant) {
                return Some((*variant, path));
            }
        }
        None
    }

    /// List all available models.
    pub fn list_available(&self) -> Vec<(ModelVariant, PathBuf)> {
        ModelVariant::ALL_BY_SPEED
            .iter()
            .filter_map(|v| self.find_variant(*v).map(|p| (*v, p)))
            .collect()
    }
}

/// Configuration for YuNet model loading.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Preferred model variant (None = auto-select fastest).
    pub preferred_variant: Option<ModelVariant>,
    /// Search paths for model files.
    pub search_paths: ModelSearchPaths,
    /// Minimum model file size to consider valid (corruption check).
    pub min_file_size: u64,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            preferred_variant: None, // Auto-select fastest
            search_paths: ModelSearchPaths::default(),
            min_file_size: 50_000, // 50KB minimum
        }
    }
}

impl ModelConfig {
    /// Create config with explicit variant selection.
    pub fn with_variant(variant: ModelVariant) -> Self {
        Self {
            preferred_variant: Some(variant),
            ..Default::default()
        }
    }

    /// Create config from environment variables.
    ///
    /// Reads `YUNET_MODEL_VARIANT` for variant selection.
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(variant_str) = std::env::var("YUNET_MODEL_VARIANT") {
            if let Some(variant) = ModelVariant::from_str(&variant_str) {
                info!(
                    "Using YuNet model variant from YUNET_MODEL_VARIANT: {}",
                    variant
                );
                config.preferred_variant = Some(variant);
            } else {
                warn!(
                    "Invalid YUNET_MODEL_VARIANT '{}', using auto-selection. \
                     Valid values: int8bq, int8, fp32, legacy",
                    variant_str
                );
            }
        }

        config
    }

    /// Resolve the model to use.
    ///
    /// Returns (variant, path) or error if no model found.
    pub fn resolve(&self) -> Result<(ModelVariant, PathBuf), ModelResolutionError> {
        // If specific variant requested, try to find it
        if let Some(variant) = self.preferred_variant {
            if let Some(path) = self.search_paths.find_variant(variant) {
                self.validate_model(&path)?;
                return Ok((variant, path));
            }
            return Err(ModelResolutionError::VariantNotFound(variant));
        }

        // Auto-select fastest available
        if let Some((variant, path)) = self.search_paths.find_best_available() {
            self.validate_model(&path)?;
            return Ok((variant, path));
        }

        Err(ModelResolutionError::NoModelsFound)
    }

    /// Validate model file exists and has reasonable size.
    fn validate_model(&self, path: &Path) -> Result<(), ModelResolutionError> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| ModelResolutionError::IoError(path.to_path_buf(), e.to_string()))?;

        if metadata.len() < self.min_file_size {
            return Err(ModelResolutionError::FileTooSmall {
                path: path.to_path_buf(),
                size: metadata.len(),
                min_size: self.min_file_size,
            });
        }

        Ok(())
    }
}

/// Errors during model resolution.
#[derive(Debug)]
pub enum ModelResolutionError {
    /// Specific variant was requested but not found.
    VariantNotFound(ModelVariant),
    /// No models found in any search path.
    NoModelsFound,
    /// Model file exists but is too small (likely corrupted).
    FileTooSmall {
        path: PathBuf,
        size: u64,
        min_size: u64,
    },
    /// IO error reading model file.
    IoError(PathBuf, String),
}

impl std::fmt::Display for ModelResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VariantNotFound(v) => write!(
                f,
                "YuNet model variant {} not found. Download from: \
                 https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet",
                v.display_name()
            ),
            Self::NoModelsFound => write!(
                f,
                "No YuNet models found. Run: curl -L -o models/face_detection_yunet_2023mar_int8bq.onnx \
                 https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx"
            ),
            Self::FileTooSmall { path, size, min_size } => write!(
                f,
                "YuNet model at {} appears corrupted ({} bytes, expected >= {} bytes)",
                path.display(),
                size,
                min_size
            ),
            Self::IoError(path, err) => write!(
                f,
                "Failed to read YuNet model at {}: {}",
                path.display(),
                err
            ),
        }
    }
}

impl std::error::Error for ModelResolutionError {}

/// Global cached model resolution.
static RESOLVED_MODEL: OnceLock<Result<(ModelVariant, PathBuf), String>> = OnceLock::new();

/// Get the resolved model (cached).
///
/// Uses `ModelConfig::from_env()` for configuration.
pub fn get_resolved_model() -> Result<(ModelVariant, PathBuf), String> {
    RESOLVED_MODEL
        .get_or_init(|| {
            let config = ModelConfig::from_env();
            match config.resolve() {
                Ok((variant, path)) => {
                    info!("YuNet model resolved: {} at {}", variant, path.display());
                    Ok((variant, path))
                }
                Err(e) => {
                    warn!("YuNet model resolution failed: {}", e);
                    Err(e.to_string())
                }
            }
        })
        .clone()
}

/// Check if any YuNet model is available (for feature detection).
pub fn is_model_available() -> bool {
    get_resolved_model().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_from_str() {
        assert_eq!(
            ModelVariant::from_str("int8bq"),
            Some(ModelVariant::Int8BlockQuantized)
        );
        assert_eq!(ModelVariant::from_str("fp32"), Some(ModelVariant::Fp32));
        assert_eq!(ModelVariant::from_str("invalid"), None);
    }

    #[test]
    fn test_variant_display() {
        let v = ModelVariant::Int8BlockQuantized;
        let s = v.to_string();
        assert!(s.contains("INT8-BQ"));
        assert!(s.contains("6ms"));
    }

    #[test]
    fn test_speed_ordering() {
        let variants = ModelVariant::ALL_BY_SPEED;
        for i in 1..variants.len() {
            assert!(
                variants[i - 1].expected_ms_per_frame() <= variants[i].expected_ms_per_frame(),
                "Variants should be ordered by speed"
            );
        }
    }

    #[test]
    fn test_default_config() {
        let config = ModelConfig::default();
        assert!(config.preferred_variant.is_none());
        assert!(!config.search_paths.directories.is_empty());
    }
}
