//! Detection module for tiered video analysis pipelines.
//!
//! This module provides composable detection pipelines that vary by `DetectionTier`:
//!
//! | Tier | Face Detection | Audio Activity | Face Activity |
//! |------|----------------|----------------|---------------|
//! | `None` | — | — | — |
//! | `Basic` | YuNet | — | — |
//! | `SpeakerAware` | YuNet + FaceMesh | — | FaceActivityAnalyzer (visual) |
//! | `MotionAware` | — (heuristic motion) | — | — |
//! | `Cinematic` | YuNet + FaceMesh + Objects | — | FaceActivityAnalyzer + Objects |
//!
//! Use `PipelineBuilder` to create pipelines with automatic fallback handling.

pub mod object_detector;
pub mod pipeline;
pub mod pipeline_builder;
pub mod pipelines;
pub mod providers;

pub use object_detector::{ObjectDetection, ObjectDetector, ObjectDetectorConfig, COCO_CLASSES};
pub use pipeline::{DetectionPipeline, DetectionResult, FrameResult};
pub use pipeline_builder::PipelineBuilder;
pub use pipelines::{BasicPipeline, MotionAwarePipeline, NonePipeline, SpeakerAwarePipeline};
pub use providers::{FaceActivityProvider, FaceProvider};

