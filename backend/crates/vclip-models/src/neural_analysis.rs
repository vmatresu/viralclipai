//! Neural analysis caching models.
//!
//! This module defines the data structures for caching per-scene neural analysis
//! results (YuNet face detection, FaceMesh landmarks, etc.) to avoid redundant
//! expensive ML inference across reprocessing runs.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Version of the neural analysis format.
/// Increment this when the structure changes to invalidate old caches.
pub const NEURAL_ANALYSIS_VERSION: u32 = 1;

/// Per-scene neural analysis results.
///
/// Contains frame-by-frame face detection and landmark data for a single scene.
/// This is cached to R2 to avoid re-running expensive ML inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SceneNeuralAnalysis {
    /// User who owns this analysis
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// Video ID this analysis belongs to
    pub video_id: String,

    /// Scene ID within the video
    pub scene_id: u32,

    /// Detection tier used to compute this analysis.
    ///
    /// Used to ensure we don't reuse a lower-tier cache entry when a higher tier is required.
    #[serde(default = "default_detection_tier")]
    pub detection_tier: crate::detection_tier::DetectionTier,

    /// Per-frame analysis results
    pub frames: Vec<FrameAnalysis>,

    /// Version of the analysis format for cache invalidation
    pub analysis_version: u32,

    /// When this analysis was created
    pub created_at: DateTime<Utc>,

    /// Optional cinematic signals (shot boundaries) for caching.
    /// Only populated when Cinematic tier processing has been run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cinematic_signals: Option<CinematicSignalsCache>,
}

fn default_detection_tier() -> crate::detection_tier::DetectionTier {
    // Backward compatibility: older cache entries were computed at the highest tier.
    crate::detection_tier::DetectionTier::SpeakerAware
}

impl SceneNeuralAnalysis {
    /// Create a new empty analysis for a scene.
    pub fn new(video_id: impl Into<String>, scene_id: u32) -> Self {
        Self {
            user_id: None,
            video_id: video_id.into(),
            scene_id,
            detection_tier: default_detection_tier(),
            frames: Vec::new(),
            analysis_version: NEURAL_ANALYSIS_VERSION,
            created_at: Utc::now(),
            cinematic_signals: None,
        }
    }

    /// Create with user ID.
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set the detection tier used for this analysis.
    pub fn with_detection_tier(mut self, tier: crate::detection_tier::DetectionTier) -> Self {
        self.detection_tier = tier;
        self
    }

    /// Add a frame analysis result.
    pub fn add_frame(&mut self, frame: FrameAnalysis) {
        self.frames.push(frame);
    }

    /// Check if this analysis is compatible with the current version.
    pub fn is_current_version(&self) -> bool {
        self.analysis_version == NEURAL_ANALYSIS_VERSION
    }

    /// Convert cached neural analysis to detection format for intelligent cropper.
    ///
    /// This converts normalized coordinates back to pixel coordinates for the
    /// cropper pipeline. The cropper expects `Vec<Vec<Detection>>` where each
    /// inner Vec contains detections for a single frame.
    ///
    /// # Arguments
    /// * `frame_width` - Video frame width in pixels
    /// * `frame_height` - Video frame height in pixels
    ///
    /// # Returns
    /// Vector of frame detections in pixel coordinates, suitable for the cropper.
    pub fn to_cropper_detections(&self, frame_width: u32, frame_height: u32) -> Vec<Vec<CropperDetection>> {
        let fw = frame_width as f32;
        let fh = frame_height as f32;

        self.frames
            .iter()
            .map(|frame| {
                frame
                    .faces
                    .iter()
                    .map(|face| {
                        let (x, y, w, h) = face.bbox.to_pixels(fw, fh);
                        CropperDetection {
                            time: frame.time,
                            x: x as f64,
                            y: y as f64,
                            width: w as f64,
                            height: h as f64,
                            score: face.score as f64,
                            track_id: face.track_id.unwrap_or(0),
                            mouth_openness: face.mouth_openness.map(|m| m as f64),
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// Detection in pixel coordinates for the intelligent cropper.
///
/// This is a simplified detection format that can be converted to the
/// cropper's internal Detection type.
#[derive(Debug, Clone)]
pub struct CropperDetection {
    /// Timestamp in seconds
    pub time: f64,
    /// Left edge x-coordinate in pixels
    pub x: f64,
    /// Top edge y-coordinate in pixels
    pub y: f64,
    /// Width in pixels
    pub width: f64,
    /// Height in pixels
    pub height: f64,
    /// Detection confidence score (0.0-1.0)
    pub score: f64,
    /// Track ID for identity persistence
    pub track_id: u32,
    /// Optional mouth openness score
    pub mouth_openness: Option<f64>,
}

/// Cacheable cinematic signals (shot boundaries).
///
/// Stored as part of `SceneNeuralAnalysis` to avoid re-running expensive
/// histogram extraction for shot detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CinematicSignalsCache {
    /// Detected shot boundaries with timing information
    pub shots: Vec<ShotBoundaryCache>,
    /// Version for cache invalidation
    pub version: u32,
    /// Shot detection threshold used
    pub shot_threshold: f64,
    /// Minimum shot duration used
    pub min_shot_duration: f64,
}

/// Version of the cinematic signals format.
pub const CINEMATIC_SIGNALS_VERSION: u32 = 1;

impl CinematicSignalsCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            shots: Vec::new(),
            version: CINEMATIC_SIGNALS_VERSION,
            shot_threshold: 0.5,
            min_shot_duration: 0.5,
        }
    }

    /// Create with shot boundaries.
    pub fn with_shots(shots: Vec<ShotBoundaryCache>, threshold: f64, min_duration: f64) -> Self {
        Self {
            shots,
            version: CINEMATIC_SIGNALS_VERSION,
            shot_threshold: threshold,
            min_shot_duration: min_duration,
        }
    }

    /// Check if cache is valid for given config.
    pub fn is_valid(&self, threshold: f64, min_duration: f64) -> bool {
        self.version == CINEMATIC_SIGNALS_VERSION
            && (self.shot_threshold - threshold).abs() < 0.01
            && (self.min_shot_duration - min_duration).abs() < 0.01
    }
}

impl Default for CinematicSignalsCache {
    fn default() -> Self {
        Self::new()
    }
}

/// A cached shot boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ShotBoundaryCache {
    /// Start time in seconds
    pub start_time: f64,
    /// End time in seconds
    pub end_time: f64,
}

/// Analysis results for a single frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct FrameAnalysis {
    /// Timestamp in seconds from scene start
    pub time: f64,

    /// Detected faces in this frame
    pub faces: Vec<FaceDetection>,
}

impl FrameAnalysis {
    /// Create a new frame analysis.
    pub fn new(time: f64) -> Self {
        Self {
            time,
            faces: Vec::new(),
        }
    }

    /// Add a face detection.
    pub fn add_face(&mut self, face: FaceDetection) {
        self.faces.push(face);
    }

    /// Get the primary (most confident or largest) face.
    pub fn primary_face(&self) -> Option<&FaceDetection> {
        self.faces.iter().max_by(|a, b| {
            a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

/// Face detection result from YuNet or similar detector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct FaceDetection {
    /// Bounding box: [x, y, width, height] in normalized coordinates (0.0-1.0)
    pub bbox: BoundingBox,

    /// Detection confidence score (0.0-1.0)
    pub score: f32,

    /// Optional tracking ID for face tracking across frames
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_id: Option<u32>,

    /// Mouth openness ratio (0.0 = closed, 1.0 = fully open)
    /// Derived from FaceMesh landmarks if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouth_openness: Option<f32>,

    /// Center X position in normalized coordinates (convenience accessor)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_x: Option<f32>,

    /// Center Y position in normalized coordinates (convenience accessor)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_y: Option<f32>,
}

impl FaceDetection {
    /// Create a new face detection.
    pub fn new(bbox: BoundingBox, score: f32) -> Self {
        let center_x = bbox.x + bbox.width / 2.0;
        let center_y = bbox.y + bbox.height / 2.0;
        Self {
            bbox,
            score,
            track_id: None,
            mouth_openness: None,
            center_x: Some(center_x),
            center_y: Some(center_y),
        }
    }

    /// Set the tracking ID.
    pub fn with_track_id(mut self, id: u32) -> Self {
        self.track_id = Some(id);
        self
    }

    /// Set the mouth openness.
    pub fn with_mouth_openness(mut self, openness: f32) -> Self {
        self.mouth_openness = Some(openness);
        self
    }

    /// Get the face center X coordinate.
    pub fn get_center_x(&self) -> f32 {
        self.center_x.unwrap_or(self.bbox.x + self.bbox.width / 2.0)
    }

    /// Get the face center Y coordinate.
    pub fn get_center_y(&self) -> f32 {
        self.center_y.unwrap_or(self.bbox.y + self.bbox.height / 2.0)
    }
}

/// Normalized bounding box.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct BoundingBox {
    /// X coordinate (normalized 0.0-1.0)
    pub x: f32,
    /// Y coordinate (normalized 0.0-1.0)
    pub y: f32,
    /// Width (normalized 0.0-1.0)
    pub width: f32,
    /// Height (normalized 0.0-1.0)
    pub height: f32,
}

impl BoundingBox {
    /// Create a new bounding box.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Create from pixel coordinates given frame dimensions.
    pub fn from_pixels(x: f32, y: f32, w: f32, h: f32, frame_w: f32, frame_h: f32) -> Self {
        Self {
            x: x / frame_w,
            y: y / frame_h,
            width: w / frame_w,
            height: h / frame_h,
        }
    }

    /// Convert to pixel coordinates given frame dimensions.
    pub fn to_pixels(&self, frame_w: f32, frame_h: f32) -> (f32, f32, f32, f32) {
        (
            self.x * frame_w,
            self.y * frame_h,
            self.width * frame_w,
            self.height * frame_h,
        )
    }

    /// Get center X coordinate.
    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    /// Get center Y coordinate.
    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    /// Get area (normalized).
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_neural_analysis_serde_roundtrip() {
        let mut analysis = SceneNeuralAnalysis::new("video_123", 1)
            .with_user("user_abc");

        let mut frame = FrameAnalysis::new(0.5);
        let face = FaceDetection::new(
            BoundingBox::new(0.2, 0.1, 0.3, 0.4),
            0.95,
        )
        .with_track_id(1)
        .with_mouth_openness(0.3);
        frame.add_face(face);
        analysis.add_frame(frame);

        let json = serde_json::to_string(&analysis).expect("serialize");
        let decoded: SceneNeuralAnalysis = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(analysis, decoded);
        assert_eq!(decoded.video_id, "video_123");
        assert_eq!(decoded.scene_id, 1);
        assert_eq!(decoded.frames.len(), 1);
        assert_eq!(decoded.frames[0].faces.len(), 1);
        assert_eq!(decoded.frames[0].faces[0].track_id, Some(1));
        assert!((decoded.frames[0].faces[0].mouth_openness.unwrap() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_from_pixels() {
        let bbox = BoundingBox::from_pixels(100.0, 50.0, 200.0, 300.0, 1920.0, 1080.0);
        assert!((bbox.x - 100.0 / 1920.0).abs() < 0.0001);
        assert!((bbox.y - 50.0 / 1080.0).abs() < 0.0001);
        assert!((bbox.width - 200.0 / 1920.0).abs() < 0.0001);
        assert!((bbox.height - 300.0 / 1080.0).abs() < 0.0001);
    }

    #[test]
    fn test_frame_primary_face() {
        let mut frame = FrameAnalysis::new(1.0);
        frame.add_face(FaceDetection::new(BoundingBox::new(0.1, 0.1, 0.2, 0.2), 0.7));
        frame.add_face(FaceDetection::new(BoundingBox::new(0.5, 0.5, 0.2, 0.2), 0.95));
        frame.add_face(FaceDetection::new(BoundingBox::new(0.3, 0.3, 0.2, 0.2), 0.8));

        let primary = frame.primary_face().expect("should have primary face");
        assert!((primary.score - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_version_check() {
        let analysis = SceneNeuralAnalysis::new("video", 1);
        assert!(analysis.is_current_version());

        let mut old_analysis = SceneNeuralAnalysis::new("video", 1);
        old_analysis.analysis_version = 0;
        assert!(!old_analysis.is_current_version());
    }
}
