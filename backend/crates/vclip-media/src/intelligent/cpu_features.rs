//! CPU Feature Detection and Verification
//!
//! Runtime detection of CPU capabilities for optimal backend selection.
//! Supports both x86_64 (Intel/AMD) and aarch64 (ARM) architectures.
//!
//! # Usage
//! ```rust
//! use vclip_media::intelligent::cpu_features::CpuFeatures;
//!
//! let features = CpuFeatures::detect();
//! features.log_capabilities();
//!
//! // For tuned builds, verify requirements
//! #[cfg(feature = "tuned-build")]
//! CpuFeatures::verify_tuned_requirements()?;
//! ```

use std::fmt;
use tracing::{info, warn};

/// CPU feature detection results for x86_64 architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuFeatures {
    /// SSE4.2 support (baseline for most operations)
    pub sse42: bool,
    /// AVX2 support (256-bit SIMD, required for portable builds)
    pub avx2: bool,
    /// FMA support (fused multiply-add)
    pub fma: bool,
    /// AVX-512 Foundation support
    pub avx512f: bool,
    /// AVX-512 Byte and Word support
    pub avx512bw: bool,
    /// AVX-512 Vector Length Extensions
    pub avx512vl: bool,
    /// AVX-512 Vector Neural Network Instructions (INT8 acceleration)
    pub avx512_vnni: bool,
    /// Architecture type
    pub arch: CpuArch,
}

/// CPU architecture type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuArch {
    /// x86_64 (Intel/AMD)
    X86_64,
    /// ARM64 (Apple Silicon, AWS Graviton)
    Aarch64,
    /// Unknown architecture
    Unknown,
}

impl fmt::Display for CpuArch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpuArch::X86_64 => write!(f, "x86_64"),
            CpuArch::Aarch64 => write!(f, "aarch64"),
            CpuArch::Unknown => write!(f, "unknown"),
        }
    }
}

/// Error type for CPU feature verification
#[derive(Debug, Clone)]
pub struct CpuMismatchError {
    /// Name of the missing feature
    pub missing_feature: &'static str,
    /// Human-readable message
    pub message: String,
}

impl std::fmt::Display for CpuMismatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CpuMismatchError {}

impl CpuFeatures {
    /// Detect CPU features at runtime.
    ///
    /// This function is safe to call from any thread and caches results
    /// via the compiler's feature detection intrinsics.
    #[cfg(target_arch = "x86_64")]
    pub fn detect() -> Self {
        Self {
            sse42: std::arch::is_x86_feature_detected!("sse4.2"),
            avx2: std::arch::is_x86_feature_detected!("avx2"),
            fma: std::arch::is_x86_feature_detected!("fma"),
            avx512f: std::arch::is_x86_feature_detected!("avx512f"),
            avx512bw: std::arch::is_x86_feature_detected!("avx512bw"),
            avx512vl: std::arch::is_x86_feature_detected!("avx512vl"),
            avx512_vnni: std::arch::is_x86_feature_detected!("avx512vnni"),
            arch: CpuArch::X86_64,
        }
    }

    /// Detect CPU features on ARM64.
    #[cfg(target_arch = "aarch64")]
    pub fn detect() -> Self {
        // ARM64 has NEON by default, no AVX equivalent
        Self {
            sse42: false,
            avx2: false,
            fma: false,
            avx512f: false,
            avx512bw: false,
            avx512vl: false,
            avx512_vnni: false,
            arch: CpuArch::Aarch64,
        }
    }

    /// Fallback for other architectures.
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub fn detect() -> Self {
        Self {
            sse42: false,
            avx2: false,
            fma: false,
            avx512f: false,
            avx512bw: false,
            avx512vl: false,
            avx512_vnni: false,
            arch: CpuArch::Unknown,
        }
    }

    /// Check if CPU has full AVX-512 support (F + BW + VL).
    #[inline]
    pub fn has_avx512(&self) -> bool {
        self.avx512f && self.avx512bw && self.avx512vl
    }

    /// Check if CPU has AVX-512 VNNI for optimal INT8 inference.
    #[inline]
    pub fn has_vnni(&self) -> bool {
        self.avx512_vnni
    }

    /// Get the optimal inference tier based on CPU capabilities.
    pub fn inference_tier(&self) -> InferenceTier {
        if self.avx512_vnni {
            InferenceTier::Vnni
        } else if self.has_avx512() {
            InferenceTier::Avx512
        } else if self.avx2 {
            InferenceTier::Avx2
        } else if self.sse42 {
            InferenceTier::Sse42
        } else {
            InferenceTier::Baseline
        }
    }

    /// Get a human-readable CPU capability string for logging.
    pub fn capability_string(&self) -> &'static str {
        match self.inference_tier() {
            InferenceTier::Vnni => "AVX-512 VNNI",
            InferenceTier::Avx512 => "AVX-512",
            InferenceTier::Avx2 => "AVX2",
            InferenceTier::Sse42 => "SSE4.2",
            InferenceTier::Baseline => "Baseline",
        }
    }

    /// Log CPU capabilities for diagnostics.
    ///
    /// Should be called once at startup to record the deployment environment.
    pub fn log_capabilities(&self) {
        info!(
            arch = %self.arch,
            sse42 = self.sse42,
            avx2 = self.avx2,
            fma = self.fma,
            avx512f = self.avx512f,
            avx512bw = self.avx512bw,
            avx512vl = self.avx512vl,
            avx512_vnni = self.avx512_vnni,
            tier = %self.inference_tier(),
            "CPU feature detection complete"
        );

        if self.avx512_vnni {
            info!("AVX-512 VNNI available: INT8 inference will be optimal (<2ms target)");
        } else if self.has_avx512() {
            info!("AVX-512 available: INT8 inference will be accelerated");
        } else if self.avx2 {
            info!("AVX2 available: using portable SIMD optimizations");
        } else {
            warn!("No advanced SIMD detected: inference may be slow");
        }
    }

    /// Verify CPU meets requirements for tuned builds (AVX-512).
    ///
    /// Call this at startup when using the `tuned-build` feature to prevent
    /// SIGILL crashes from AVX-512 instructions on unsupported CPUs.
    ///
    /// # Returns
    /// - `Ok(())` if CPU meets requirements
    /// - `Err(CpuMismatchError)` if a required feature is missing
    pub fn verify_tuned_requirements() -> Result<(), CpuMismatchError> {
        let features = Self::detect();

        // Log what we found
        features.log_capabilities();

        #[cfg(target_arch = "x86_64")]
        {
            if !features.avx512f {
                return Err(CpuMismatchError {
                    missing_feature: "avx512f",
                    message: format!(
                        "This binary requires AVX-512F which is not available on this CPU. \
                         Detected: {}. Use the 'portable' image for this CPU.",
                        features.capability_string()
                    ),
                });
            }
            if !features.avx512bw {
                return Err(CpuMismatchError {
                    missing_feature: "avx512bw",
                    message: "This binary requires AVX-512BW which is not available. \
                              Use the 'portable' image for this CPU."
                        .to_string(),
                });
            }

            // Warn (but don't fail) if VNNI is not available
            if !features.avx512_vnni {
                warn!(
                    "AVX-512 VNNI not available. INT8 inference will be suboptimal. \
                     Consider using FP32 model or upgrading to VNNI-capable CPU \
                     (Intel Ice Lake+, AMD Zen 4+)."
                );
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            // ARM64 doesn't have AVX-512, tuned build is not applicable
            warn!(
                "Running tuned build on ARM64. This is not optimal. \
                 Use the portable build for ARM64 deployment."
            );
        }

        Ok(())
    }

    /// Verify CPU meets requirements for portable builds (AVX2).
    pub fn verify_portable_requirements() -> Result<(), CpuMismatchError> {
        let features = Self::detect();

        #[cfg(target_arch = "x86_64")]
        {
            if !features.avx2 {
                return Err(CpuMismatchError {
                    missing_feature: "avx2",
                    message: format!(
                        "This binary requires AVX2 which is not available on this CPU. \
                         Detected capabilities: {}. \
                         AVX2 is available on Intel Haswell (2013+) and AMD Zen (2017+).",
                        features.capability_string()
                    ),
                });
            }
        }

        Ok(())
    }
}

/// Inference performance tier based on CPU capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferenceTier {
    /// No SIMD acceleration
    Baseline = 0,
    /// SSE4.2 (128-bit SIMD)
    Sse42 = 1,
    /// AVX2 (256-bit SIMD)
    Avx2 = 2,
    /// AVX-512 (512-bit SIMD)
    Avx512 = 3,
    /// AVX-512 with VNNI (optimal INT8)
    Vnni = 4,
}

impl fmt::Display for InferenceTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InferenceTier::Baseline => write!(f, "baseline"),
            InferenceTier::Sse42 => write!(f, "sse42"),
            InferenceTier::Avx2 => write!(f, "avx2"),
            InferenceTier::Avx512 => write!(f, "avx512"),
            InferenceTier::Vnni => write!(f, "vnni"),
        }
    }
}

impl InferenceTier {
    /// Get expected inference latency range for 1080p YuNet detection.
    pub fn expected_latency_ms(&self) -> (f64, f64) {
        match self {
            InferenceTier::Baseline => (15.0, 25.0),
            InferenceTier::Sse42 => (10.0, 18.0),
            InferenceTier::Avx2 => (4.0, 8.0),
            InferenceTier::Avx512 => (2.5, 5.0),
            InferenceTier::Vnni => (1.0, 2.0),
        }
    }

    /// Get recommended model type for this tier.
    pub fn recommended_model(&self) -> &'static str {
        match self {
            InferenceTier::Vnni => "face_detection_yunet_2023mar_int8bq.onnx",
            InferenceTier::Avx512 => "face_detection_yunet_2023mar_int8.onnx",
            _ => "face_detection_yunet_2023mar.onnx",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_detection() {
        let features = CpuFeatures::detect();

        // Should always succeed
        println!("Detected CPU features: {:?}", features);
        println!("Architecture: {}", features.arch);
        println!("Inference tier: {}", features.inference_tier());

        // On x86_64, at least SSE4.2 should be available on any modern CPU
        #[cfg(target_arch = "x86_64")]
        {
            // Most CI runners have at least SSE4.2
            // AVX2 is common but not guaranteed
            assert!(features.arch == CpuArch::X86_64);
        }

        #[cfg(target_arch = "aarch64")]
        {
            assert!(features.arch == CpuArch::Aarch64);
        }
    }

    #[test]
    fn test_inference_tier_ordering() {
        assert!(InferenceTier::Vnni > InferenceTier::Avx512);
        assert!(InferenceTier::Avx512 > InferenceTier::Avx2);
        assert!(InferenceTier::Avx2 > InferenceTier::Sse42);
        assert!(InferenceTier::Sse42 > InferenceTier::Baseline);
    }

    #[test]
    fn test_capability_string() {
        let features = CpuFeatures::detect();
        let cap = features.capability_string();

        // Should be one of the known strings
        assert!(
            ["AVX-512 VNNI", "AVX-512", "AVX2", "SSE4.2", "Baseline"].contains(&cap),
            "Unknown capability string: {}",
            cap
        );
    }

    #[test]
    fn test_expected_latency() {
        let tier = InferenceTier::Avx2;
        let (min, max) = tier.expected_latency_ms();

        assert!(min > 0.0);
        assert!(max > min);
        assert!(max < 100.0); // Sanity check
    }

    #[test]
    fn test_recommended_model() {
        assert!(InferenceTier::Vnni
            .recommended_model()
            .contains("int8bq"));
        assert!(InferenceTier::Avx512
            .recommended_model()
            .contains("int8"));
        assert!(!InferenceTier::Avx2
            .recommended_model()
            .contains("int8"));
    }
}
