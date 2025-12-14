//! Cacheable signal extraction for the Cinematic pipeline.
//!
//! This module separates "signal extraction" (expensive ML/histogram operations)
//! from "algorithm" (cheap CPU operations). Signals can be cached to R2 to avoid
//! redundant computation across reprocessing runs.
//!
//! # Architecture
//!
//! ```text
//! Video Input
//!     │
//!     ├──────────────────────────────────────┐
//!     ▼                                      ▼
//! [ShotSignals]                      [FaceSignals]
//! - Histogram extraction             - Saliency from detections
//! - Shot boundary detection          - Activity-weighted importance
//!     │                                      │
//!     └───────────┬──────────────────────────┘
//!                 ▼
//!         [CinematicSignals]
//!         - Combined cacheable struct
//!         - Stored in SceneNeuralAnalysis
//! ```
//!
//! # Caching Strategy
//!
//! `CinematicSignals` is stored as part of `SceneNeuralAnalysis` and persisted to R2.
//! On subsequent runs, if cached signals are present and valid (version check), the
//! expensive extraction is skipped.

mod face_signals;
mod shot_signals;

pub use face_signals::{FaceSignals, PerFrameSaliency};
pub use shot_signals::{ShotBoundary, ShotSignals};

use serde::{Deserialize, Serialize};

/// Combined signal cache for the Cinematic pipeline.
///
/// This struct contains all cacheable signals needed for cinematic processing:
/// - Shot boundaries from histogram analysis
/// - Per-frame saliency signals (computed at processing time, not cached)
///
/// Only shot boundaries are cached since they require expensive histogram extraction.
/// Face saliency is computed on-demand from cached neural analysis detections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CinematicSignals {
    /// Detected shot boundaries
    pub shots: Vec<ShotBoundary>,

    /// Version for cache invalidation
    pub version: u32,

    /// Configuration used to generate these signals (for validation)
    pub shot_threshold: f64,
    pub min_shot_duration: f64,
}

/// Current version of the cinematic signals format.
/// Increment when structure changes to invalidate old caches.
pub const CINEMATIC_SIGNALS_VERSION: u32 = 1;

impl CinematicSignals {
    /// Create a new empty signals container.
    pub fn new() -> Self {
        Self {
            shots: Vec::new(),
            version: CINEMATIC_SIGNALS_VERSION,
            shot_threshold: 0.5,
            min_shot_duration: 0.5,
        }
    }

    /// Create with shot boundaries.
    pub fn with_shots(shots: Vec<ShotBoundary>, threshold: f64, min_duration: f64) -> Self {
        Self {
            shots,
            version: CINEMATIC_SIGNALS_VERSION,
            shot_threshold: threshold,
            min_shot_duration: min_duration,
        }
    }

    /// Check if this cache is compatible with current version and config.
    pub fn is_valid(&self, threshold: f64, min_duration: f64) -> bool {
        self.version == CINEMATIC_SIGNALS_VERSION
            && (self.shot_threshold - threshold).abs() < 0.01
            && (self.min_shot_duration - min_duration).abs() < 0.01
    }

    /// Check if shot detection was disabled (single shot covering full video).
    pub fn is_single_shot(&self) -> bool {
        self.shots.len() <= 1
    }
}

impl Default for CinematicSignals {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signals_version() {
        let signals = CinematicSignals::new();
        assert_eq!(signals.version, CINEMATIC_SIGNALS_VERSION);
    }

    #[test]
    fn test_signals_validity() {
        let signals = CinematicSignals::with_shots(vec![], 0.5, 0.5);
        assert!(signals.is_valid(0.5, 0.5));
        assert!(!signals.is_valid(0.6, 0.5)); // Different threshold
        assert!(!signals.is_valid(0.5, 1.0)); // Different min duration
    }

    #[test]
    fn test_single_shot_detection() {
        let empty = CinematicSignals::new();
        assert!(empty.is_single_shot());

        let single = CinematicSignals::with_shots(
            vec![ShotBoundary::new(0.0, 10.0)],
            0.5,
            0.5,
        );
        assert!(single.is_single_shot());

        let multiple = CinematicSignals::with_shots(
            vec![
                ShotBoundary::new(0.0, 5.0),
                ShotBoundary::new(5.0, 10.0),
            ],
            0.5,
            0.5,
        );
        assert!(!multiple.is_single_shot());
    }
}
