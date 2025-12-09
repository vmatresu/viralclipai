//! Provider traits wrapping existing detection implementations.
//!
//! These traits provide a uniform interface for detection components,
//! making them composable in pipelines.

use async_trait::async_trait;
use std::path::Path;

use crate::error::MediaResult;
use crate::intelligent::{BoundingBox, FrameDetections};

/// Face detection provider.
///
/// Wraps face detection implementations (YuNet, heuristics) with a
/// uniform interface.
#[async_trait]
pub trait FaceProvider: Send + Sync {
    /// Detect faces in a video segment.
    ///
    /// # Returns
    /// Vector of frame detections, one per sampled frame.
    async fn detect_faces(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
        fps: f64,
    ) -> MediaResult<Vec<FrameDetections>>;

    /// Provider name for logging.
    fn name(&self) -> &'static str;

    /// Whether this provider uses AI/ML detection (vs pure heuristics).
    fn uses_ai(&self) -> bool;
}

/// Face activity analysis provider.
///
/// Wraps implementations that analyze per-face activity (mouth movement,
/// motion, size changes) to determine which face is most active.
pub trait FaceActivityProvider: Send + Sync {
    /// Compute activity score for a face region.
    ///
    /// # Arguments
    /// * `bbox` - Bounding box of the face
    /// * `track_id` - Face track identifier
    /// * `time` - Current timestamp
    ///
    /// # Returns
    /// Activity score from 0.0 (inactive) to 1.0 (very active).
    fn compute_activity(
        &mut self,
        bbox: &BoundingBox,
        track_id: u32,
        time: f64,
        confidence: f64,
    ) -> f64;

    /// Clean up resources for a track that's no longer visible.
    fn cleanup_track(&mut self, track_id: u32);

    /// Reset all state.
    fn reset(&mut self);

    /// Provider name for logging.
    fn name(&self) -> &'static str;
}

// ============================================================================
// Default Implementations
// ============================================================================

use crate::intelligent::{FaceDetector, IntelligentCropConfig};

/// YuNet-based face provider with heuristic fallback.
pub struct YuNetFaceProvider {
    detector: FaceDetector,
}

impl YuNetFaceProvider {
    pub fn new() -> Self {
        Self {
            detector: FaceDetector::new(IntelligentCropConfig::default()),
        }
    }
}

impl Default for YuNetFaceProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FaceProvider for YuNetFaceProvider {
    async fn detect_faces(
        &self,
        video_path: &Path,
        start_time: f64,
        end_time: f64,
        width: u32,
        height: u32,
        fps: f64,
    ) -> MediaResult<Vec<FrameDetections>> {
        self.detector
            .detect_in_video(video_path, start_time, end_time, width, height, fps)
            .await
    }

    fn name(&self) -> &'static str {
        "yunet"
    }

    fn uses_ai(&self) -> bool {
        true
    }
}

#[cfg(feature = "opencv")]
use crate::intelligent::{FaceActivityAnalyzer, FaceActivityConfig};

/// Face activity provider using visual analysis.
#[cfg(feature = "opencv")]
pub struct VisualFaceActivityProvider {
    analyzer: FaceActivityAnalyzer,
}

#[cfg(feature = "opencv")]
impl VisualFaceActivityProvider {
    pub fn new() -> Self {
        Self {
            analyzer: FaceActivityAnalyzer::new(FaceActivityConfig::default()),
        }
    }
}

#[cfg(feature = "opencv")]
impl Default for VisualFaceActivityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "opencv")]
impl FaceActivityProvider for VisualFaceActivityProvider {
    fn compute_activity(
        &mut self,
        bbox: &BoundingBox,
        track_id: u32,
        time: f64,
        confidence: f64,
    ) -> f64 {
        // Use size change tracking as a proxy for activity
        // Full implementation would use frame data
        self.analyzer
            .compute_size_change_score(bbox, confidence, track_id, time)
    }

    fn cleanup_track(&mut self, track_id: u32) {
        self.analyzer.cleanup_track(track_id);
    }

    fn reset(&mut self) {
        self.analyzer.reset();
    }

    fn name(&self) -> &'static str {
        "visual_face_activity"
    }
}

/// Stub face activity provider when OpenCV is not available.
#[cfg(not(feature = "opencv"))]
pub struct VisualFaceActivityProvider;

#[cfg(not(feature = "opencv"))]
impl VisualFaceActivityProvider {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(feature = "opencv"))]
impl Default for VisualFaceActivityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(feature = "opencv"))]
impl FaceActivityProvider for VisualFaceActivityProvider {
    fn compute_activity(
        &mut self,
        _bbox: &BoundingBox,
        _track_id: u32,
        _time: f64,
        _confidence: f64,
    ) -> f64 {
        0.0 // No activity detection without OpenCV
    }

    fn cleanup_track(&mut self, _track_id: u32) {}

    fn reset(&mut self) {}

    fn name(&self) -> &'static str {
        "stub_face_activity"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yunet_provider_creation() {
        let provider = YuNetFaceProvider::new();
        assert_eq!(provider.name(), "yunet");
        assert!(provider.uses_ai());
    }

    #[test]
    fn test_face_activity_provider_creation() {
        let provider = VisualFaceActivityProvider::new();
        assert_eq!(provider.name(), if cfg!(feature = "opencv") {
            "visual_face_activity"
        } else {
            "stub_face_activity"
        });
    }
}
