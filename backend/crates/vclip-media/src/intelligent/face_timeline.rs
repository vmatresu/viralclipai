//! Face Timeline Schema and Export
//!
//! Provides structured output for face detection results compatible with
//! various downstream consumers (crop planners, analytics, debugging).
//!
//! # Schema
//! ```json
//! {
//!   "version": "1.0",
//!   "video_info": {
//!     "width": 1920,
//!     "height": 1080,
//!     "fps": 30.0,
//!     "duration_ms": 60000
//!   },
//!   "engine_config": {
//!     "mode": "optimized",
//!     "inference_size": [960, 540],
//!     "detect_every_n": 5
//!   },
//!   "entries": [
//!     {
//!       "frame_idx": 0,
//!       "timestamp_ms": 0,
//!       "is_keyframe": true,
//!       "scene_hash": 12345678,
//!       "faces": [
//!         {
//!           "track_id": 0,
//!           "bbox": {"x": 0.25, "y": 0.1, "w": 0.15, "h": 0.2},
//!           "confidence": 0.95
//!         }
//!       ]
//!     }
//!   ],
//!   "stats": {
//!     "frames_processed": 1800,
//!     "keyframe_count": 360,
//!     "gap_frame_count": 1440,
//!     "faces_detected": 1800,
//!     "throughput_multiplier": 5.0
//!   }
//! }
//! ```

use super::face_engine::{EngineMode, EngineStats, FaceFrameResult, TrackedFace};
use super::mapping::NormalizedBBox;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use tracing::info;

/// Schema version for compatibility checking.
pub const TIMELINE_VERSION: &str = "1.0";

/// Video information for the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Video framerate
    pub fps: f64,
    /// Total duration in milliseconds
    pub duration_ms: u64,
}

/// Engine configuration captured at processing time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfigSnapshot {
    /// Engine mode used
    pub mode: String,
    /// Inference dimensions [width, height]
    pub inference_size: [u32; 2],
    /// Temporal decimation interval
    pub detect_every_n: u32,
    /// Confidence threshold
    pub confidence_threshold: f64,
}

/// Face bounding box in normalized coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceBBox {
    /// X coordinate (0 = left, 1 = right)
    pub x: f64,
    /// Y coordinate (0 = top, 1 = bottom)
    pub y: f64,
    /// Width as fraction of frame width
    pub w: f64,
    /// Height as fraction of frame height
    pub h: f64,
}

impl From<NormalizedBBox> for FaceBBox {
    fn from(bbox: NormalizedBBox) -> Self {
        Self {
            x: bbox.x,
            y: bbox.y,
            w: bbox.w,
            h: bbox.h,
        }
    }
}

/// Face entry in a timeline frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceEntry {
    /// Persistent track ID
    pub track_id: u32,
    /// Normalized bounding box
    pub bbox: FaceBBox,
    /// Detection/tracking confidence
    pub confidence: f64,
}

impl From<&TrackedFace> for FaceEntry {
    fn from(face: &TrackedFace) -> Self {
        Self {
            track_id: face.track_id,
            bbox: face.bbox_normalized.into(),
            confidence: face.confidence,
        }
    }
}

/// Single frame entry in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceTimelineEntry {
    /// Frame index (0-based)
    pub frame_idx: u64,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Whether this was a keyframe (full inference)
    pub is_keyframe: bool,
    /// Scene hash for discontinuity detection
    pub scene_hash: u64,
    /// Faces detected/tracked in this frame
    pub faces: Vec<FaceEntry>,
}

impl From<&FaceFrameResult> for FaceTimelineEntry {
    fn from(detections: &FaceFrameResult) -> Self {
        Self {
            frame_idx: detections.frame_idx,
            timestamp_ms: detections.timestamp_ms,
            is_keyframe: detections.is_keyframe,
            scene_hash: detections.scene_hash,
            faces: detections.faces.iter().map(FaceEntry::from).collect(),
        }
    }
}

/// Statistics summary for the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineStats {
    /// Total frames processed
    pub frames_processed: u64,
    /// Keyframe detection count
    pub keyframe_count: u64,
    /// Gap frame prediction count
    pub gap_frame_count: u64,
    /// Total faces detected
    pub faces_detected: u64,
    /// Scene cuts detected
    pub scene_cut_count: u64,
    /// Throughput multiplier from decimation
    pub throughput_multiplier: f64,
    /// Average inference time per keyframe (ms)
    pub avg_inference_time_ms: f64,
    /// Peak inference time (ms)
    pub peak_inference_time_ms: u64,
    /// Backend used
    pub backend: String,
    /// CPU tier detected
    pub cpu_tier: String,
}

impl From<&EngineStats> for TimelineStats {
    fn from(stats: &EngineStats) -> Self {
        Self {
            frames_processed: stats.frames_processed,
            keyframe_count: stats.keyframe_count,
            gap_frame_count: stats.gap_frame_count,
            faces_detected: stats.faces_detected,
            scene_cut_count: stats.scene_cut_count,
            throughput_multiplier: stats.throughput_multiplier(),
            avg_inference_time_ms: stats.avg_inference_time_ms,
            peak_inference_time_ms: stats.peak_inference_time_ms,
            backend: stats.backend.clone(),
            cpu_tier: stats.cpu_tier.clone(),
        }
    }
}

/// Complete face timeline for a video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceTimeline {
    /// Schema version
    pub version: String,
    /// Video information
    pub video_info: VideoInfo,
    /// Engine configuration snapshot
    pub engine_config: EngineConfigSnapshot,
    /// Timeline entries (one per frame)
    pub entries: Vec<FaceTimelineEntry>,
    /// Processing statistics
    pub stats: TimelineStats,
}

impl FaceTimeline {
    /// Create a new empty timeline.
    pub fn new(video_info: VideoInfo, engine_config: EngineConfigSnapshot) -> Self {
        Self {
            version: TIMELINE_VERSION.to_string(),
            video_info,
            engine_config,
            entries: Vec::new(),
            stats: TimelineStats {
                frames_processed: 0,
                keyframe_count: 0,
                gap_frame_count: 0,
                faces_detected: 0,
                scene_cut_count: 0,
                throughput_multiplier: 1.0,
                avg_inference_time_ms: 0.0,
                peak_inference_time_ms: 0,
                backend: String::new(),
                cpu_tier: String::new(),
            },
        }
    }

    /// Add a frame detection result to the timeline.
    pub fn add_frame(&mut self, detections: &FaceFrameResult) {
        self.entries.push(FaceTimelineEntry::from(detections));
    }

    /// Finalize timeline with statistics.
    pub fn finalize(&mut self, stats: &EngineStats) {
        self.stats = TimelineStats::from(stats);
    }

    /// Get unique track IDs in the timeline.
    pub fn track_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .entries
            .iter()
            .flat_map(|e| e.faces.iter().map(|f| f.track_id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Get frames where a specific track appears.
    pub fn track_frames(&self, track_id: u32) -> Vec<(u64, &FaceEntry)> {
        self.entries
            .iter()
            .filter_map(|e| {
                e.faces
                    .iter()
                    .find(|f| f.track_id == track_id)
                    .map(|f| (e.frame_idx, f))
            })
            .collect()
    }

    /// Get face count per frame.
    pub fn face_counts(&self) -> Vec<(u64, usize)> {
        self.entries
            .iter()
            .map(|e| (e.frame_idx, e.faces.len()))
            .collect()
    }

    /// Calculate track lifespan (first frame to last frame).
    pub fn track_lifespans(&self) -> HashMap<u32, (u64, u64)> {
        let mut spans: HashMap<u32, (u64, u64)> = HashMap::new();

        for entry in &self.entries {
            for face in &entry.faces {
                spans
                    .entry(face.track_id)
                    .and_modify(|(_, end)| *end = entry.frame_idx)
                    .or_insert((entry.frame_idx, entry.frame_idx));
            }
        }

        spans
    }

    /// Get keyframe indices.
    pub fn keyframe_indices(&self) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|e| e.is_keyframe)
            .map(|e| e.frame_idx)
            .collect()
    }

    /// Get scene cut frame indices.
    pub fn scene_cut_indices(&self) -> Vec<u64> {
        let mut cuts = Vec::new();
        let mut prev_hash = 0u64;

        for entry in &self.entries {
            if entry.scene_hash != 0 && entry.scene_hash != prev_hash && prev_hash != 0 {
                cuts.push(entry.frame_idx);
            }
            prev_hash = entry.scene_hash;
        }

        cuts
    }
}

/// Timeline exporter for various output formats.
pub struct TimelineExporter;

impl TimelineExporter {
    /// Export timeline to JSON.
    pub fn to_json(timeline: &FaceTimeline) -> serde_json::Result<String> {
        serde_json::to_string_pretty(timeline)
    }

    /// Export timeline to compact JSON (no whitespace).
    pub fn to_json_compact(timeline: &FaceTimeline) -> serde_json::Result<String> {
        serde_json::to_string(timeline)
    }

    /// Write timeline to file.
    pub fn write_to_file<P: AsRef<Path>>(timeline: &FaceTimeline, path: P) -> std::io::Result<()> {
        let json = Self::to_json(timeline)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let mut file = std::fs::File::create(path.as_ref())?;
        file.write_all(json.as_bytes())?;

        info!("Wrote face timeline to {}", path.as_ref().display());
        Ok(())
    }

    /// Create timeline builder from engine configuration.
    pub fn builder(
        mode: EngineMode,
        inf_width: u32,
        inf_height: u32,
        detect_every_n: u32,
        confidence_threshold: f64,
        video_width: u32,
        video_height: u32,
        fps: f64,
        duration_ms: u64,
    ) -> FaceTimeline {
        let video_info = VideoInfo {
            width: video_width,
            height: video_height,
            fps,
            duration_ms,
        };

        let engine_config = EngineConfigSnapshot {
            mode: mode.to_string(),
            inference_size: [inf_width, inf_height],
            detect_every_n,
            confidence_threshold,
        };

        FaceTimeline::new(video_info, engine_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_timeline() -> FaceTimeline {
        TimelineExporter::builder(
            EngineMode::Optimized,
            960,
            540,
            5,
            0.3,
            1920,
            1080,
            30.0,
            60000,
        )
    }

    #[test]
    fn test_timeline_creation() {
        let timeline = create_test_timeline();
        assert_eq!(timeline.version, TIMELINE_VERSION);
        assert_eq!(timeline.video_info.width, 1920);
        assert_eq!(timeline.video_info.height, 1080);
        assert_eq!(timeline.engine_config.inference_size, [960, 540]);
    }

    #[test]
    fn test_face_bbox_from_normalized() {
        let normalized = NormalizedBBox::new(0.25, 0.1, 0.15, 0.2);
        let bbox: FaceBBox = normalized.into();

        assert!((bbox.x - 0.25).abs() < 0.001);
        assert!((bbox.y - 0.1).abs() < 0.001);
        assert!((bbox.w - 0.15).abs() < 0.001);
        assert!((bbox.h - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_track_ids() {
        let mut timeline = create_test_timeline();

        // Add some entries
        timeline.entries.push(FaceTimelineEntry {
            frame_idx: 0,
            timestamp_ms: 0,
            is_keyframe: true,
            scene_hash: 123,
            faces: vec![
                FaceEntry {
                    track_id: 0,
                    bbox: FaceBBox {
                        x: 0.2,
                        y: 0.1,
                        w: 0.1,
                        h: 0.15,
                    },
                    confidence: 0.9,
                },
                FaceEntry {
                    track_id: 1,
                    bbox: FaceBBox {
                        x: 0.5,
                        y: 0.2,
                        w: 0.1,
                        h: 0.15,
                    },
                    confidence: 0.8,
                },
            ],
        });

        timeline.entries.push(FaceTimelineEntry {
            frame_idx: 1,
            timestamp_ms: 33,
            is_keyframe: false,
            scene_hash: 123,
            faces: vec![FaceEntry {
                track_id: 0,
                bbox: FaceBBox {
                    x: 0.21,
                    y: 0.1,
                    w: 0.1,
                    h: 0.15,
                },
                confidence: 0.85,
            }],
        });

        let track_ids = timeline.track_ids();
        assert_eq!(track_ids, vec![0, 1]);
    }

    #[test]
    fn test_track_lifespans() {
        let mut timeline = create_test_timeline();

        for i in 0..10 {
            timeline.entries.push(FaceTimelineEntry {
                frame_idx: i,
                timestamp_ms: i * 33,
                is_keyframe: i % 5 == 0,
                scene_hash: 123,
                faces: if i < 8 {
                    vec![FaceEntry {
                        track_id: 0,
                        bbox: FaceBBox {
                            x: 0.2,
                            y: 0.1,
                            w: 0.1,
                            h: 0.15,
                        },
                        confidence: 0.9,
                    }]
                } else {
                    vec![]
                },
            });
        }

        let lifespans = timeline.track_lifespans();
        assert_eq!(lifespans.get(&0), Some(&(0, 7)));
    }

    #[test]
    fn test_json_serialization() {
        let timeline = create_test_timeline();
        let json = TimelineExporter::to_json(&timeline).unwrap();

        assert!(json.contains("\"version\":"));
        assert!(json.contains("\"video_info\":"));
        assert!(json.contains("\"engine_config\":"));
    }

    #[test]
    fn test_keyframe_indices() {
        let mut timeline = create_test_timeline();

        for i in 0..10 {
            timeline.entries.push(FaceTimelineEntry {
                frame_idx: i,
                timestamp_ms: i * 33,
                is_keyframe: i % 5 == 0,
                scene_hash: 123,
                faces: vec![],
            });
        }

        let keyframes = timeline.keyframe_indices();
        assert_eq!(keyframes, vec![0, 5]);
    }
}
