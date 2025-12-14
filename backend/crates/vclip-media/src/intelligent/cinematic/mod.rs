//! Cinematic pipeline for AutoAI-inspired smooth camera motion.
//!
//! This module implements a new detection tier (`DetectionTier::Cinematic`) that provides
//! professional-quality camera motion through:
//!
//! 1. **Shot Detection**: Color histogram-based scene boundary detection
//! 2. **Camera Mode Analysis**: Automatically selects between stationary, panning, and tracking modes
//! 3. **Polynomial Trajectory Optimization**: Smooth cubic curve fitting for cinematic camera paths
//! 4. **Adaptive Zoom**: Dynamic zoom based on subject count and activity
//!
//! # Architecture
//!
//! ```text
//! Input: Video segment + target aspect ratio
//!     │
//!     ▼
//! [NEW] Shot Detector → Shot boundaries
//!     │
//!     ▼
//! [REUSE] Face Detection (YuNet) → Detections
//!     │
//!     ▼
//! [REUSE] IoU Tracker → Track IDs
//!     │
//!     ▼
//! [NEW] Camera Mode Analyzer → CameraMode per shot
//!     │
//!     ▼
//! [NEW] Adaptive Zoom → zoom levels per keyframe
//!     │
//!     ▼
//! [NEW] Trajectory Optimizer → smooth polynomial path per shot
//!     │
//!     ▼
//! [REUSE] Crop Planner → CropWindows
//!     │
//!     ▼
//! [REUSE] FFmpeg Renderer (sendcmd) → Output video
//! ```

pub mod camera_mode;
pub mod composition;
pub mod config;
pub mod l1_optimizer;
pub mod processor;
pub mod scene_window;
pub mod shot_detector;
pub mod signal_fusion;
pub mod signals;
pub mod trajectory;
pub mod zoom;

pub use camera_mode::{CameraMode, CameraModeAnalyzer};
pub use composition::{CameraHint, FocusZone, SceneComposition, SceneCompositionAnalyzer, SubjectArrangement};
pub use config::{CinematicConfig, TrajectoryMethod};
pub use l1_optimizer::{L1TrajectoryOptimizer, L1OptimizerConfig, L1Error};
pub use processor::{CinematicProcessor, create_cinematic_clip, create_cinematic_clip_with_cache};
pub use scene_window::{SceneWindowAnalyzer, WindowAnalysis};
pub use shot_detector::{Shot, ShotDetector};
pub use signal_fusion::{SaliencySignal, SignalFusingCalculator, SignalSource};
pub use trajectory::TrajectoryOptimizer;
pub use zoom::AdaptiveZoom;


