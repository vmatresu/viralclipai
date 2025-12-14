//! Scene composition analysis for intelligent camera decisions.
//!
//! Analyzes scene layouts to make smarter camera decisions:
//! - Subject arrangement (single, side-by-side, scattered)
//! - Optimal framing zones
//! - Camera behavior hints
//!
//! This module helps the pipeline understand the scene context before
//! making camera mode and framing decisions.

use std::collections::HashMap;

use crate::detection::ObjectDetection;
use crate::intelligent::models::{BoundingBox, Detection};

/// Scene composition analysis result.
#[derive(Debug, Clone)]
pub struct SceneComposition {
    /// Detected arrangement of subjects
    pub arrangement: SubjectArrangement,
    /// Primary focus zone (pixel coordinates)
    pub primary_focus: FocusZone,
    /// Secondary focus zone (if any)
    pub secondary_focus: Option<FocusZone>,
    /// Recommended camera behavior
    pub camera_hint: CameraHint,
    /// Number of detected subjects
    pub subject_count: usize,
}

/// Subject arrangement patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectArrangement {
    /// Single subject prominent in frame
    Single,
    /// Two subjects side by side (podcast layout)
    SideBySide,
    /// Subjects in interview positions (one larger, one smaller)
    Interview,
    /// Multiple subjects scattered
    Group,
    /// No clear subjects (screen recording, etc)
    NoSubjects,
}

impl SubjectArrangement {
    /// Get recommended zoom level for this arrangement.
    pub fn recommended_zoom(&self) -> f64 {
        match self {
            Self::Single => 0.15,      // Tight on single subject
            Self::SideBySide => 0.5,   // Wide to fit both
            Self::Interview => 0.35,   // Medium
            Self::Group => 0.6,        // Wide
            Self::NoSubjects => 0.5,   // Default
        }
    }
}

/// Focus zone for camera targeting.
#[derive(Debug, Clone)]
pub struct FocusZone {
    /// Center X coordinate (pixels)
    pub cx: f64,
    /// Center Y coordinate (pixels)
    pub cy: f64,
    /// Recommended crop width (pixels)
    pub width: f64,
    /// Recommended crop height (pixels)
    pub height: f64,
    /// Confidence in this zone (0-1)
    pub confidence: f64,
}

impl FocusZone {
    /// Create a centered focus zone.
    pub fn centered(frame_width: u32, frame_height: u32) -> Self {
        Self {
            cx: frame_width as f64 / 2.0,
            cy: frame_height as f64 / 2.0,
            width: frame_width as f64,
            height: frame_height as f64,
            confidence: 0.5,
        }
    }

    /// Create from a bounding box.
    pub fn from_bbox(bbox: &BoundingBox, confidence: f64) -> Self {
        Self {
            cx: bbox.cx(),
            cy: bbox.cy(),
            width: bbox.width,
            height: bbox.height,
            confidence,
        }
    }
}

/// Camera behavior hints based on scene composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraHint {
    /// Lock on primary subject (minimal movement)
    LockOn,
    /// Frame both subjects equally (static wide shot)
    FrameBoth,
    /// Follow the active subject (dynamic tracking)
    FollowActive,
    /// Wide shot for group (minimal zoom)
    WideShot,
    /// Center frame (no subjects detected)
    CenterDefault,
}

impl CameraHint {
    /// Whether this hint suggests static camera behavior.
    pub fn is_static(&self) -> bool {
        matches!(
            self,
            Self::LockOn | Self::FrameBoth | Self::WideShot | Self::CenterDefault
        )
    }
}

/// Scene composition analyzer.
///
/// Analyzes a window of detections to determine the optimal
/// camera behavior and framing strategy.
pub struct SceneCompositionAnalyzer {
    frame_width: u32,
    frame_height: u32,
}

impl SceneCompositionAnalyzer {
    /// Create a new analyzer for the given frame dimensions.
    pub fn new(frame_width: u32, frame_height: u32) -> Self {
        Self {
            frame_width,
            frame_height,
        }
    }

    /// Analyze a window of frames to determine scene composition.
    ///
    /// # Arguments
    /// * `face_detections` - Per-frame face detections
    /// * `object_detections` - Per-frame object detections (optional)
    ///
    /// # Returns
    /// Scene composition analysis with arrangement, focus zones, and camera hints.
    pub fn analyze(
        &self,
        face_detections: &[Vec<Detection>],
        object_detections: &[Vec<ObjectDetection>],
    ) -> SceneComposition {
        // 1. Count unique tracks across window
        let track_count = self.count_unique_tracks(face_detections);

        // 2. Analyze spatial distribution
        let arrangement = self.determine_arrangement(face_detections, track_count);

        // 3. Compute focus zones based on arrangement
        let (primary, secondary) =
            self.compute_focus_zones(face_detections, object_detections, arrangement);

        // 4. Determine camera hint
        let camera_hint = self.determine_camera_hint(arrangement, track_count);

        SceneComposition {
            arrangement,
            primary_focus: primary,
            secondary_focus: secondary,
            camera_hint,
            subject_count: track_count,
        }
    }

    /// Count unique face tracks in the detection window.
    fn count_unique_tracks(&self, detections: &[Vec<Detection>]) -> usize {
        let mut seen_tracks: HashMap<u32, usize> = HashMap::new();

        for frame in detections {
            for det in frame {
                *seen_tracks.entry(det.track_id).or_insert(0) += 1;
            }
        }

        // Only count tracks that appear in at least 10% of frames
        let min_appearances = (detections.len() / 10).max(1);
        seen_tracks
            .values()
            .filter(|&&count| count >= min_appearances)
            .count()
    }

    /// Determine subject arrangement from detection patterns.
    fn determine_arrangement(
        &self,
        detections: &[Vec<Detection>],
        track_count: usize,
    ) -> SubjectArrangement {
        if track_count == 0 {
            return SubjectArrangement::NoSubjects;
        }

        if track_count == 1 {
            return SubjectArrangement::Single;
        }

        // Get average positions per track
        let avg_positions = self.average_track_positions(detections);

        if track_count == 2 {
            let positions: Vec<_> = avg_positions.values().collect();
            if positions.len() == 2 {
                let (cx1, cy1, w1, h1) = positions[0];
                let (cx2, cy2, w2, h2) = positions[1];

                let frame_w = self.frame_width as f64;
                let frame_h = self.frame_height as f64;

                // FIRST: Check for Interview (one face significantly larger than other > 2x area)
                // This takes priority because size difference is a stronger indicator
                let area1 = w1 * h1;
                let area2 = w2 * h2;
                let area_ratio = area1.max(area2) / area1.min(area2).max(1.0);

                if area_ratio > 2.0 {
                    return SubjectArrangement::Interview;
                }

                // SECOND: Check Side by side (similar sizes, significant X separation)
                // Normalize to 0-1 range
                let x1 = cx1 / frame_w;
                let x2 = cx2 / frame_w;
                let y1 = cy1 / frame_h;
                let y2 = cy2 / frame_h;

                // Side by side: significant X separation (> 30%), minimal Y difference (< 15%)
                let x_diff = (x1 - x2).abs();
                let y_diff = (y1 - y2).abs();

                if x_diff > 0.3 && y_diff < 0.15 {
                    return SubjectArrangement::SideBySide;
                }
            }
        }

        SubjectArrangement::Group
    }

    /// Compute average position and size for each track.
    fn average_track_positions(
        &self,
        detections: &[Vec<Detection>],
    ) -> HashMap<u32, (f64, f64, f64, f64)> {
        let mut track_sums: HashMap<u32, (f64, f64, f64, f64, usize)> = HashMap::new();

        for frame in detections {
            for det in frame {
                let entry = track_sums.entry(det.track_id).or_insert((0.0, 0.0, 0.0, 0.0, 0));
                entry.0 += det.bbox.cx();
                entry.1 += det.bbox.cy();
                entry.2 += det.bbox.width;
                entry.3 += det.bbox.height;
                entry.4 += 1;
            }
        }

        track_sums
            .into_iter()
            .map(|(id, (cx, cy, w, h, count))| {
                let c = count as f64;
                (id, (cx / c, cy / c, w / c, h / c))
            })
            .collect()
    }

    /// Compute focus zones based on arrangement.
    fn compute_focus_zones(
        &self,
        face_detections: &[Vec<Detection>],
        _object_detections: &[Vec<ObjectDetection>],
        arrangement: SubjectArrangement,
    ) -> (FocusZone, Option<FocusZone>) {
        if face_detections.is_empty() || face_detections.iter().all(|f| f.is_empty()) {
            return (
                FocusZone::centered(self.frame_width, self.frame_height),
                None,
            );
        }

        // Get all face bounding boxes
        let all_faces: Vec<&Detection> = face_detections.iter().flat_map(|f| f.iter()).collect();

        if all_faces.is_empty() {
            return (
                FocusZone::centered(self.frame_width, self.frame_height),
                None,
            );
        }

        match arrangement {
            SubjectArrangement::Single => {
                // Focus on the single track (use median position)
                let bbox = self.compute_median_bbox(&all_faces);
                (FocusZone::from_bbox(&bbox, 0.9), None)
            }
            SubjectArrangement::SideBySide => {
                // Compute union bbox of all faces
                let union = self.compute_union_bbox(&all_faces);
                (FocusZone::from_bbox(&union, 0.85), None)
            }
            SubjectArrangement::Interview => {
                // Primary: larger face, secondary: smaller face
                let avg_positions = self.average_track_positions(face_detections);
                let mut sorted: Vec<_> = avg_positions.iter().collect();
                sorted.sort_by(|a, b| {
                    let area_a = a.1 .2 * a.1 .3;
                    let area_b = b.1 .2 * b.1 .3;
                    area_b.partial_cmp(&area_a).unwrap_or(std::cmp::Ordering::Equal)
                });

                if let Some((_, (cx, cy, w, h))) = sorted.first() {
                    let primary = FocusZone {
                        cx: *cx,
                        cy: *cy,
                        width: *w,
                        height: *h,
                        confidence: 0.85,
                    };
                    let secondary = sorted.get(1).map(|(_, (cx, cy, w, h))| FocusZone {
                        cx: *cx,
                        cy: *cy,
                        width: *w,
                        height: *h,
                        confidence: 0.6,
                    });
                    (primary, secondary)
                } else {
                    (
                        FocusZone::centered(self.frame_width, self.frame_height),
                        None,
                    )
                }
            }
            SubjectArrangement::Group | SubjectArrangement::NoSubjects => {
                let union = self.compute_union_bbox(&all_faces);
                (FocusZone::from_bbox(&union, 0.7), None)
            }
        }
    }

    /// Compute median bounding box from detections.
    fn compute_median_bbox(&self, faces: &[&Detection]) -> BoundingBox {
        if faces.is_empty() {
            return BoundingBox::new(
                self.frame_width as f64 / 4.0,
                self.frame_height as f64 / 4.0,
                self.frame_width as f64 / 2.0,
                self.frame_height as f64 / 2.0,
            );
        }

        let mut cxs: Vec<f64> = faces.iter().map(|d| d.bbox.cx()).collect();
        let mut cys: Vec<f64> = faces.iter().map(|d| d.bbox.cy()).collect();
        let mut widths: Vec<f64> = faces.iter().map(|d| d.bbox.width).collect();
        let mut heights: Vec<f64> = faces.iter().map(|d| d.bbox.height).collect();

        cxs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        cys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mid = faces.len() / 2;
        let cx = cxs[mid];
        let cy = cys[mid];
        let w = widths[mid];
        let h = heights[mid];

        BoundingBox::new(cx - w / 2.0, cy - h / 2.0, w, h)
    }

    /// Compute union bounding box of all detections.
    fn compute_union_bbox(&self, faces: &[&Detection]) -> BoundingBox {
        if faces.is_empty() {
            return BoundingBox::new(
                self.frame_width as f64 / 4.0,
                self.frame_height as f64 / 4.0,
                self.frame_width as f64 / 2.0,
                self.frame_height as f64 / 2.0,
            );
        }

        let min_x = faces.iter().map(|d| d.bbox.x).fold(f64::INFINITY, f64::min);
        let min_y = faces.iter().map(|d| d.bbox.y).fold(f64::INFINITY, f64::min);
        let max_x = faces
            .iter()
            .map(|d| d.bbox.x + d.bbox.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = faces
            .iter()
            .map(|d| d.bbox.y + d.bbox.height)
            .fold(f64::NEG_INFINITY, f64::max);

        BoundingBox::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Determine camera hint based on arrangement.
    fn determine_camera_hint(
        &self,
        arrangement: SubjectArrangement,
        track_count: usize,
    ) -> CameraHint {
        match arrangement {
            SubjectArrangement::Single => CameraHint::LockOn,
            SubjectArrangement::SideBySide => CameraHint::FrameBoth,
            SubjectArrangement::Interview => CameraHint::FollowActive,
            SubjectArrangement::Group if track_count > 3 => CameraHint::WideShot,
            SubjectArrangement::Group => CameraHint::FollowActive,
            SubjectArrangement::NoSubjects => CameraHint::CenterDefault,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detection(cx: f64, cy: f64, size: f64, track_id: u32) -> Detection {
        Detection::new(
            0.0,
            BoundingBox::new(cx - size / 2.0, cy - size / 2.0, size, size),
            0.9,
            track_id,
        )
    }

    #[test]
    fn test_single_subject_arrangement() {
        let analyzer = SceneCompositionAnalyzer::new(1920, 1080);
        let faces = vec![
            vec![make_detection(960.0, 400.0, 200.0, 1)],
            vec![make_detection(965.0, 405.0, 200.0, 1)],
            vec![make_detection(970.0, 400.0, 200.0, 1)],
        ];
        let objects = vec![vec![], vec![], vec![]];

        let composition = analyzer.analyze(&faces, &objects);
        assert_eq!(composition.arrangement, SubjectArrangement::Single);
        assert_eq!(composition.camera_hint, CameraHint::LockOn);
        assert_eq!(composition.subject_count, 1);
    }

    #[test]
    fn test_side_by_side_arrangement() {
        let analyzer = SceneCompositionAnalyzer::new(1920, 1080);
        // Two faces on left and right (30% separation)
        let faces = vec![
            vec![
                make_detection(480.0, 400.0, 150.0, 1),  // Left
                make_detection(1440.0, 400.0, 150.0, 2), // Right
            ],
            vec![
                make_detection(480.0, 400.0, 150.0, 1),
                make_detection(1440.0, 400.0, 150.0, 2),
            ],
        ];
        let objects = vec![vec![], vec![]];

        let composition = analyzer.analyze(&faces, &objects);
        assert_eq!(composition.arrangement, SubjectArrangement::SideBySide);
        assert_eq!(composition.camera_hint, CameraHint::FrameBoth);
    }

    #[test]
    fn test_interview_arrangement() {
        let analyzer = SceneCompositionAnalyzer::new(1920, 1080);
        // One large face, one small face (> 2x area difference)
        let faces = vec![
            vec![
                make_detection(700.0, 400.0, 300.0, 1), // Large
                make_detection(1500.0, 400.0, 100.0, 2), // Small
            ],
            vec![
                make_detection(700.0, 400.0, 300.0, 1),
                make_detection(1500.0, 400.0, 100.0, 2),
            ],
        ];
        let objects = vec![vec![], vec![]];

        let composition = analyzer.analyze(&faces, &objects);
        assert_eq!(composition.arrangement, SubjectArrangement::Interview);
        assert_eq!(composition.camera_hint, CameraHint::FollowActive);
    }

    #[test]
    fn test_no_subjects() {
        let analyzer = SceneCompositionAnalyzer::new(1920, 1080);
        let faces: Vec<Vec<Detection>> = vec![vec![], vec![], vec![]];
        let objects = vec![vec![], vec![], vec![]];

        let composition = analyzer.analyze(&faces, &objects);
        assert_eq!(composition.arrangement, SubjectArrangement::NoSubjects);
        assert_eq!(composition.camera_hint, CameraHint::CenterDefault);
    }

    #[test]
    fn test_group_arrangement() {
        let analyzer = SceneCompositionAnalyzer::new(1920, 1080);
        // Four faces scattered
        let faces = vec![vec![
            make_detection(400.0, 300.0, 100.0, 1),
            make_detection(800.0, 400.0, 100.0, 2),
            make_detection(1200.0, 350.0, 100.0, 3),
            make_detection(1600.0, 300.0, 100.0, 4),
        ]];
        let objects = vec![vec![]];

        let composition = analyzer.analyze(&faces, &objects);
        assert_eq!(composition.arrangement, SubjectArrangement::Group);
        assert_eq!(composition.camera_hint, CameraHint::WideShot);
    }
}
