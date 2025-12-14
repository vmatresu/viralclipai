# Cinematic Pipeline Design Document

## Overview

The Cinematic pipeline is an AutoAI-inspired video reframing system that provides smooth, professional camera motion for converting landscape videos to portrait (9:16) format. It is implemented as a new `DetectionTier::Cinematic` that operates independently from existing detection tiers.

## Goals

1. **Smooth Camera Motion**: Eliminate jitter and create cinematic camera movements
2. **Intelligent Camera Behavior**: Automatically select between stationary, panning, and tracking modes
3. **Adaptive Framing**: Dynamic zoom that adapts to single vs. multi-subject scenarios
4. **Backward Compatibility**: New tier that doesn't affect existing pipelines

## Architecture

### Pipeline Flow

```
Input: Video segment + target aspect ratio (9:16)
    │
    ▼
┌─────────────────────────────────────────┐
│  [REUSE] Face Detection (YuNet)         │
│  - Extracts face bounding boxes         │
│  - Returns Detection[] per frame        │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [REUSE] IoU Tracker                    │
│  - Assigns persistent track IDs         │
│  - Maintains identity across frames     │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [NEW] Camera Mode Analyzer             │
│  - Analyzes motion patterns             │
│  - Selects: Stationary/Panning/Tracking │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [NEW] Adaptive Zoom                    │
│  - Computes zoom level per keyframe     │
│  - Single face: tight framing           │
│  - Multiple active: wide framing        │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [NEW] Trajectory Optimizer             │
│  - Polynomial curve fitting             │
│  - Produces smooth camera path          │
│  - Minimizes jerk (acceleration change) │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [REUSE] Crop Planner                   │
│  - Converts keyframes to crop windows   │
│  - Ensures face containment             │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [REUSE] FFmpeg Renderer (sendcmd)      │
│  - Renders with dynamic crop updates    │
│  - Single-pass encoding                 │
└─────────────────────────────────────────┘
    │
    ▼
Output: Portrait video (1080x1920)
```

### Module Structure

```
backend/crates/vclip-media/src/intelligent/cinematic/
├── mod.rs           # Module exports
├── config.rs        # CinematicConfig
├── camera_mode.rs   # CameraMode enum + CameraModeAnalyzer
├── trajectory.rs    # TrajectoryOptimizer (polynomial fitting)
├── zoom.rs          # AdaptiveZoom
└── processor.rs     # CinematicProcessor (orchestration)
```

## Components

### 1. Camera Mode Selection

The camera mode determines how the virtual camera behaves throughout the clip.

#### Modes

| Mode           | Behavior                         | When Selected           |
| -------------- | -------------------------------- | ----------------------- |
| **Stationary** | Camera locks to median position  | Low motion (std < 5%)   |
| **Panning**    | Linear interpolation start → end | Moderate motion (5-15%) |
| **Tracking**   | Full polynomial smoothing        | High motion (> 15%)     |

#### Algorithm

```rust
pub fn analyze_motion(keyframes: &[CameraKeyframe], frame_size: (u32, u32)) -> CameraMode {
    let (w, h) = frame_size;

    // Compute normalized statistics
    let cx_values: Vec<f64> = keyframes.iter().map(|k| k.cx / w as f64).collect();
    let cy_values: Vec<f64> = keyframes.iter().map(|k| k.cy / h as f64).collect();
    let width_values: Vec<f64> = keyframes.iter().map(|k| k.width / w as f64).collect();

    let position_std = (std_dev(&cx_values) + std_dev(&cy_values)) / 2.0;
    let size_std = std_dev(&width_values);

    if position_std < 0.05 && size_std < 0.05 {
        CameraMode::Stationary
    } else if position_std > 0.15 || size_std > 0.10 {
        CameraMode::Tracking
    } else {
        CameraMode::Panning
    }
}
```

### 2. Trajectory Optimization

Uses regularized least-squares polynomial fitting to create smooth camera paths.

#### Mathematical Formulation

For each dimension (cx, cy, width), fit a cubic polynomial:

```
f(t) = a₀ + a₁t + a₂t² + a₃t³
```

Minimize the objective:

```
J = ||Ax - y||² + λ||Dx||²
```

Where:

- `A` = Vandermonde matrix (polynomial basis)
- `x` = polynomial coefficients
- `y` = observed values (keyframe positions)
- `D` = second derivative matrix (smoothness regularization)
- `λ` = smoothness weight (default: 0.3)

#### Solution

```rust
// Closed-form solution via normal equations
let coeffs = (A.transpose() * A + lambda * D.transpose() * D)
    .try_inverse()
    .map(|inv| inv * A.transpose() * y);
```

#### Output

Sample the polynomial at the output frame rate (30 fps) to generate smooth keyframes.

### 3. Adaptive Zoom

Dynamically adjusts zoom level based on scene content.

#### Behavior

| Scenario              | Zoom Behavior                      |
| --------------------- | ---------------------------------- |
| No active faces       | Wide shot (min zoom)               |
| Single active face    | Tight framing (face ~25% of frame) |
| Multiple active faces | Wide enough to frame all           |
| One dominant speaker  | Focus on speaker despite others    |

#### Algorithm

```rust
pub fn compute_zoom(
    faces: &[Detection],
    activities: &HashMap<u32, f64>,
) -> f64 {
    let active_faces: Vec<_> = faces.iter()
        .filter(|f| activities.get(&f.track_id).unwrap_or(&0.0) > &self.multi_face_threshold)
        .collect();

    match active_faces.len() {
        0 => self.min_zoom,
        1 => self.compute_single_face_zoom(&active_faces[0]),
        _ => self.compute_multi_face_zoom(&active_faces),
    }
}

fn compute_single_face_zoom(&self, face: &Detection) -> f64 {
    // Zoom so face occupies ideal_face_ratio of frame height
    let target_face_height = self.frame_height * self.ideal_face_ratio;
    let zoom = target_face_height / face.bbox.height;
    zoom.clamp(self.min_zoom, self.max_zoom)
}

fn compute_multi_face_zoom(&self, faces: &[&Detection]) -> f64 {
    // Compute union bounding box
    let union = faces.iter().fold(faces[0].bbox.clone(), |acc, f| acc.union(&f.bbox));
    // Zoom so union fits with padding
    let target_height = self.frame_height * 0.7;  // 70% of frame
    let zoom = target_height / union.height;
    zoom.clamp(self.min_zoom, self.max_zoom)
}
```

### 4. Cinematic Processor

Orchestrates the full pipeline.

```rust
impl CinematicProcessor {
    pub async fn process(&self, input: &Path, config: &CinematicConfig) -> Result<CropWindows> {
        // 1. Run face detection (reuse existing)
        let detections = self.detector.detect_in_video(input).await?;

        // 2. Track faces (reuse existing)
        let tracked = self.tracker.track(detections);

        // 3. Compute initial keyframes from detections
        let raw_keyframes = self.compute_keyframes(&tracked);

        // 4. Analyze camera mode
        let mode = CameraModeAnalyzer::new(config).analyze(&raw_keyframes, self.frame_size);

        // 5. Apply mode-specific processing
        let smoothed_keyframes = match mode {
            CameraMode::Stationary => self.apply_stationary(&raw_keyframes),
            CameraMode::Panning => self.apply_panning(&raw_keyframes),
            CameraMode::Tracking => {
                // Apply adaptive zoom
                let zoomed = self.adaptive_zoom.apply(&raw_keyframes, &tracked);
                // Apply polynomial smoothing
                self.trajectory_optimizer.optimize(&zoomed)
            }
        };

        // 6. Convert to crop windows (reuse existing)
        let crop_windows = self.crop_planner.plan(&smoothed_keyframes, self.target_aspect);

        Ok(crop_windows)
    }
}
```

## Configuration

```rust
pub struct CinematicConfig {
    // Camera mode thresholds
    pub stationary_threshold: f64,      // 0.05 (5% motion = stationary)
    pub panning_threshold: f64,         // 0.15 (15% motion = panning vs tracking)

    // Trajectory optimization
    pub polynomial_degree: usize,       // 3 (cubic)
    pub smoothness_weight: f64,         // 0.3 (regularization)
    pub output_sample_rate: f64,        // 30.0 fps

    // Adaptive zoom
    pub min_zoom: f64,                  // 1.0
    pub max_zoom: f64,                  // 3.0
    pub ideal_face_ratio: f64,          // 0.25 (face = 25% of frame)
    pub multi_face_threshold: f64,      // 0.3 (activity to be "active")
    pub zoom_smoothing: f64,            // 0.1 (slow zoom transitions)

    // Face margins (inherited from existing)
    pub vertical_margin: f64,           // 0.12
    pub horizontal_margin: f64,         // 0.08
}
```

## Integration Points

### Detection Tier

```rust
// In vclip-models/src/detection_tier.rs
pub enum DetectionTier {
    None,
    Basic,
    SpeakerAware,
    MotionAware,
    Cinematic,  // NEW
}
```

### Style

```rust
// In vclip-models/src/style.rs
pub enum Style {
    // ... existing ...
    IntelligentCinematic,       // Single subject
    IntelligentCinematicSplit,  // Multi-subject (future)
}
```

### Style Routing

```rust
// In vclip-media/src/styles/mod.rs
match style {
    Style::IntelligentCinematic => {
        Box::new(CinematicProcessor::new(config))
    }
    // ... existing routing ...
}
```

## Dependencies

- `nalgebra` - Matrix operations for polynomial fitting (already available)
- Reuses: `FaceDetector`, `IoUTracker`, `CropPlanner`, `IntelligentRenderer`

## Implementation Status

### ✅ Implemented (Phase 1-3)

| Feature                 | Status | Module                    | Notes                                               |
| ----------------------- | ------ | ------------------------- | --------------------------------------------------- |
| Camera Mode Selection   | ✅     | `camera_mode.rs`          | Stationary/Panning/Tracking auto-detection          |
| Trajectory Optimization | ✅     | `trajectory.rs`           | Cubic polynomial fitting with regularization        |
| Adaptive Zoom           | ✅     | `zoom.rs`                 | Dynamic zoom based on face count and activity       |
| Shot Detection          | ✅     | `signals/shot_signals.rs` | FFmpeg-based histogram extraction (no OpenCV)       |
| Signal Fusion           | ✅     | `signals/face_signals.rs` | Weighted saliency via `SignalFusingCalculator`      |
| Per-Shot Processing     | ✅     | `processor.rs`            | Each shot gets independent camera mode + trajectory |
| Cache Integration       | ✅     | `vclip-models`            | `CinematicSignalsCache` in `SceneNeuralAnalysis`    |

### Updated Pipeline Flow

```
Input: Video segment + target aspect ratio (9:16)
    │
    ▼
┌─────────────────────────────────────────┐
│  [NEW] Shot Detection                   │  ← Added
│  - FFmpeg histogram extraction          │
│  - Chi-squared distance for boundaries  │
│  - Configurable threshold/min duration  │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [REUSE] Face Detection (YuNet)         │
│  - Extracts face bounding boxes         │
│  - Returns Detection[] per frame        │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [NEW] Signal Fusion                    │  ← Added
│  - Face weight + activity boost         │
│  - Weighted focus point calculation     │
└─────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────┐
│  [PER-SHOT] Camera Mode + Trajectory    │  ← Updated
│  - Independent mode per shot            │
│  - Polynomial optimization per shot     │
└─────────────────────────────────────────┘
    │
    ▼
└── Crop Planning → FFmpeg Rendering ──▶ Output
```

### Updated Module Structure

```
backend/crates/vclip-media/src/intelligent/cinematic/
├── mod.rs              # Module exports
├── config.rs           # CinematicConfig (shot detection + signal fusion params)
├── camera_mode.rs      # CameraMode enum + CameraModeAnalyzer
├── trajectory.rs       # TrajectoryOptimizer (polynomial fitting)
├── zoom.rs             # AdaptiveZoom
├── processor.rs        # CinematicProcessor (7-step pipeline)
├── shot_detector.rs    # ShotDetector (histogram chi-squared)
├── signal_fusion.rs    # SignalFusingCalculator
└── signals/            # [NEW] Cacheable signal extraction
    ├── mod.rs          # CinematicSignals (cache struct)
    ├── shot_signals.rs # ShotSignals (FFmpeg histogram extraction)
    └── face_signals.rs # FaceSignals (saliency wrapper)
```

### 🔲 Remaining Work (Phase 4-5)

| Feature               | Priority | Description                                                       |
| --------------------- | -------- | ----------------------------------------------------------------- |
| Debug Visualization   | Medium   | FFmpeg `drawbox` overlay for crop window debugging                |
| Per-Shot Caching      | Low      | Cache shot boundaries to R2 (structure exists, not persisted yet) |
| Object Detection      | Future   | Integrate ONNX MobileNet SSD for non-face saliency                |
| Cinematic Split Style | Future   | Multi-subject variant with split screen                           |

## Testing Strategy

1. **Unit Tests**

   - Camera mode classification with synthetic keyframes
   - Polynomial fitting correctness and smoothness
   - Adaptive zoom decisions

2. **Integration Tests**

   - Static single person → STATIONARY mode
   - Moving single person → TRACKING with smooth trajectory
   - Interview (2 people) → Adaptive zoom focusing on speaker

3. **Visual Comparison**
   - A/B testing: Cinematic vs. existing Intelligent styles
   - Subjective quality assessment

## Risk Mitigation

1. **Backward Compatibility**: Separate tier, existing code unchanged
2. **Graceful Degradation**: Falls back to Gaussian smoothing if polynomial fitting fails
3. **Performance**: Polynomial fitting is O(n³) but n is small (~100-500 keyframes)
