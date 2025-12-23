# YuNet Production-Grade CPU Inference Optimization Plan

**Version:** 1.1
**Status:** Architecture Design (Expert-Reviewed)
**Target:** ViralClipAI Video Worker
**Focus:** YuNet (OpenCV FaceDetectorYN) production-grade CPU inference optimization with OpenVINO
**Target Improvement:** 80% throughput increase over v1.0 baseline

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current State Analysis](#2-current-state-analysis)
3. [Target Architecture](#3-target-architecture)
4. [Fixed-Resolution Letterbox Inference](#4-fixed-resolution-letterbox-inference)
5. [Temporal Decimation & Tracking](#5-temporal-decimation--tracking)
6. [OpenVINO-First Backend Policy](#6-openvino-first-backend-policy)
7. [OpenVINO Preprocessing API (NEW v1.1)](#7-openvino-preprocessing-api-new-v11)
8. [INT8 Quantization & VNNI Optimization (NEW v1.1)](#8-int8-quantization--vnni-optimization-new-v11)
9. [Build & Docker Requirements](#9-build--docker-requirements)
10. [Zero-Copy / Shared-Memory Evaluation](#10-zero-copy--shared-memory-evaluation)
11. [Rust Implementation Design](#11-rust-implementation-design)
12. [NUMA Awareness Strategy](#12-numa-awareness-strategy)
13. [PR-by-PR Implementation Plan (Reordered for Max ROI)](#13-pr-by-pr-implementation-plan-reordered-for-max-roi)
14. [Input-Specific Optimizations (16:9 YouTube) (NEW v1.1)](#14-input-specific-optimizations-169-youtube-new-v11)
15. [Benchmark Plan](#15-benchmark-plan)
16. [Acceptance Criteria Verification](#16-acceptance-criteria-verification)
17. [Validation Requirements (NEW v1.1)](#17-validation-requirements-new-v11)

---

## v1.1 Changelog

| Change | Section | Impact |
|--------|---------|--------|
| OpenVINO Preprocessing API | 7 | Bypass sws_scale, JIT-compiled YUV→BGR |
| INT8/VNNI optimizations | 8 | Sub-2ms inference on AVX-512 VNNI CPUs |
| Scene cut → Kalman reset | 5.4 | Prevent ghost detections on cuts |
| Padding value in MappingMeta | 4.1 | Model-expected mean for edge accuracy |
| Reordered PR plan | 13 | Build/ISA first → immediate perf validation |
| NUMA virtual node check | 12 | Handle AWS C6i/C7g single virtual nodes |
| Input-specific optimizations | 14 | 16:9 YouTube content focus |
| Validation requirements | 17 | Hard gates for production readiness |

---

## 1. Executive Summary

### Goal
Transform the current YuNet face detection pipeline from "prototype-grade" to "production-grade" with:

- **Fixed-resolution input tensors** (cache-friendly, stable shapes)
- **Deterministic inverse coordinate mapping** (zero-bar output guarantee)
- **Temporal decimation + tracking** (YuNet not on every frame)
- **OpenVINO-first DNN backend** with safe fallback
- **OpenVINO Preprocessing API** for YUV→BGR (bypass sws_scale) [NEW v1.1]
- **INT8 quantization with AVX-512 VNNI** for sub-2ms inference [NEW v1.1]
- **Reduced memory copies** between FFmpeg decode and inference
- **Zero heap allocations** in the hot loop (steady state)
- **Lifetime-safe AVFrame wrapping** for Rust
- **Scene-cut aware Kalman reset** to prevent ghost detections [NEW v1.1]

### Target Performance (v1.1)

| Metric | v1.0 Baseline | v1.1 Target | Improvement |
|--------|---------------|-------------|-------------|
| Keyframe inference (1080p) | ~6-8ms | <5ms | 25-40% |
| Keyframe inference (AVX-512 VNNI) | ~6-8ms | <2ms | 70%+ |
| Effective FPS (N=5) | ~200 | ~360 | 80% |
| Decode capacity | 100 fps | 200 fps | 100% |

### Current Pain Points

| Issue | Impact | Priority |
|-------|--------|----------|
| Variable inference sizes per frame | Cache misses, unstable perf | P0 |
| No letterbox preprocessing | Aspect ratio distortion possible | P0 |
| YuNet runs on every sampled frame | 6-25ms × N frames overhead | P0 |
| Using sws_scale for YUV→BGR | Suboptimal vs OpenVINO preproc | P0 [NEW] |
| DNN backend defaults to SSE3 baseline | Missing AVX2/AVX-512 perf | P1 |
| Per-frame Mat allocations in hot loop | Allocator thrash | P1 |
| Multiple frame copies in pipeline | Memory bandwidth waste | P1 |
| No scene-cut tracker invalidation | Ghost detections on cuts | P1 [NEW] |
| No OpenVINO backend selection | Missing inference engine gains | P2 |

---

## 2. Current State Analysis

### 2.1 YuNet Implementation (`yunet.rs`)

**Location:** `/backend/crates/vclip-media/src/intelligent/yunet.rs`

**Current Architecture:**
```
Video File → VideoCapture::read() → Mat (variable size)
           → imgproc::resize() → detector input Mat (dynamic)
           → FaceDetectorYN::detect() → faces Mat
           → parse_detection_results() → Vec<(BoundingBox, f64)>
```

**Problems Identified:**

1. **Dynamic Input Sizing (lines 322-348)**
   ```rust
   fn calculate_input_size(frame_width: u32, frame_height: u32) -> (i32, i32) {
       let target_width = 960.0;
       let target_height = 540.0;
       // ... calculates per-video, not fixed
   }
   ```
   - Input size varies per video resolution
   - YuNet.set_input_size() called per-detection
   - No letterbox padding (uses stretch-resize via INTER_LINEAR)

2. **Per-Frame Allocations (lines 422-434)**
   ```rust
   let mut resized = Mat::default();  // Allocates new Mat every frame!
   imgproc::resize(frame, &mut resized, ...);
   ```
   - `resized` Mat allocated every detection call
   - `faces` output Mat allocated every call
   - No buffer pooling

3. **Backend Selection (lines 362-365)**
   ```rust
   let backends = [
       (DNN_BACKEND_DEFAULT, DNN_TARGET_CPU, "default"),
       (DNN_BACKEND_OPENCV, DNN_TARGET_CPU, "opencv"),
   ];
   ```
   - Missing `DNN_BACKEND_INFERENCE_ENGINE` (OpenVINO)
   - Falls back to OpenCV DNN which uses SSE3 baseline

4. **No Temporal Decimation**
   - Every sampled frame runs full YuNet inference
   - No keyframe detection with tracking interpolation
   - Scene cut detection exists but not integrated with face detection

5. **[NEW v1.1] No Scene-Cut Tracker Invalidation**
   - Kalman tracker not reset on scene cuts
   - Causes ghost detections when Person A → Person B

### 2.2 Tracker Implementation (`tracker.rs`)

**Current:** IoU-based greedy matching tracker exists but:
- Only used AFTER all detections are collected
- Not integrated into real-time gap-frame interpolation
- No velocity/Kalman filtering for smooth tracking
- **[NEW v1.1] No scene-cut invalidation**

### 2.3 Docker/Build Configuration

**Current OpenCV Build (pre-built artifact):**
- OpenCV 4.12.0 from tarball
- Unknown CMake flags (no visibility into ISA baseline)
- No OpenVINO integration confirmed

**Current Dockerfile Issues:**
- No `WITH_OPENVINO=ON` in OpenCV build
- No CPU baseline/dispatch specification
- No runtime CPU feature guard

---

## 3. Target Architecture

### 3.1 High-Level Architecture Diagram (v1.1)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        VIDEO WORKER PROCESS                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐    ┌──────────────────────────────────────────────┐   │
│  │                 │    │            FaceInferenceEngine               │   │
│  │   FFmpeg        │    │  ┌────────────────────────────────────────┐  │   │
│  │   Decoder       │    │  │    OpenVINO Preprocessing API         │  │   │
│  │   (AVFrame)     │    │  │  ┌────────────────────────────────┐   │  │   │
│  │                 │    │  │  │   YUV420p/NV12 → BGR24        │   │  │   │
│  │  ┌───────────┐  │    │  │  │   (JIT-compiled, in-engine)   │   │  │   │
│  │  │ AVFrame   │──┼────┼──│──│   + Letterbox + Normalize     │   │  │   │
│  │  │ (YUV420p) │  │    │  │  │   [Bypasses sws_scale]        │   │  │   │
│  │  └───────────┘  │    │  │  └────────────────────────────────┘   │  │   │
│  │        │        │    │  │              │                        │  │   │
│  │        │ Zero-  │    │  │              ▼                        │  │   │
│  │        │ Copy   │    │  │  ┌────────────────────────────────┐   │  │   │
│  │        │ Wrap   │    │  │  │   Fixed Canvas (960×540)       │   │  │   │
│  │        ▼        │    │  │  │   MappingMeta + padding_value  │   │  │   │
│  │  ┌───────────┐  │    │  │  └────────────────────────────────┘   │  │   │
│  │  │  NV12/    │──┼────┼──│──────────────│                        │  │   │
│  │  │  I420     │  │    │  │              ▼                        │  │   │
│  │  │  (Direct) │  │    │  │  ┌────────────────────────────────┐   │  │   │
│  │  └───────────┘  │    │  │  │     Backend Selection          │   │  │   │
│  │                 │    │  │  │                                │   │  │   │
│  │                 │    │  │  │  ┌──────────────┐ ┌──────────┐ │   │  │   │
│  │                 │    │  │  │  │  OpenVINO    │ │ OpenCV   │ │   │  │   │
│  │                 │    │  │  │  │  INT8/VNNI   │ │ DNN      │ │   │  │   │
│  │                 │    │  │  │  │  (Primary)   │ │(Fallback)│ │   │  │   │
│  │                 │    │  │  │  └──────────────┘ └──────────┘ │   │  │   │
│  │                 │    │  │  └────────────────────────────────┘   │  │   │
│  │                 │    │  │              │                        │  │   │
│  │                 │    │  │              ▼                        │  │   │
│  │                 │    │  │  ┌────────────────────────────────┐   │  │   │
│  │                 │    │  │  │     Temporal Decimation        │   │  │   │
│  │                 │    │  │  │  ┌─────────┐    ┌───────────┐  │   │  │   │
│  │                 │    │  │  │  │Keyframe?│───▶│ YuNet INT8│  │   │  │   │
│  │                 │    │  │  │  │ or      │    │ Detection │  │   │  │   │
│  │                 │    │  │  │  │Scene Cut│    └───────────┘  │   │  │   │
│  │                 │    │  │  │  └─────────┘           │       │   │  │   │
│  │                 │    │  │  │       │ No             │       │   │  │   │
│  │                 │    │  │  │       ▼                ▼       │   │  │   │
│  │                 │    │  │  │  ┌─────────┐   ┌────────────┐  │   │  │   │
│  │                 │    │  │  │  │ Kalman  │   │ Inverse    │  │   │  │   │
│  │                 │    │  │  │  │ Predict │   │ Map + Norm │  │   │  │   │
│  │                 │    │  │  │  └─────────┘   └────────────┘  │   │  │   │
│  │                 │    │  │  │       │              │         │   │  │   │
│  │                 │    │  │  │       └──────┬───────┘         │   │  │   │
│  │                 │    │  │  └──────────────┼─────────────────┘   │  │   │
│  └─────────────────┘    │  │                 ▼                     │  │   │
│                         │  │  ┌────────────────────────────────┐   │  │   │
│                         │  │  │  Scene-Cut Aware Tracker       │   │  │   │
│                         │  │  │  (hard reset on cuts)          │   │  │   │
│                         │  │  └────────────────────────────────┘   │  │   │
│                         │  │                 │                     │  │   │
│                         │  └─────────────────┼─────────────────────┘  │   │
│                         │                    ▼                        │   │
│                         │ ┌────────────────────────────────────────┐  │   │
│                         │ │              FaceTimeline              │  │   │
│                         │ │  (Versioned JSON, normalized coords)   │  │   │
│                         │ └────────────────────────────────────────┘  │   │
│                         │                                             │   │
│                         └─────────────────────────────────────────────┘   │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow Summary (v1.1)

```
┌────────────────────────────────────────────────────────────────────────────┐
│ FRAME N (Keyframe)                                                         │
│ ──────────────────                                                         │
│                                                                            │
│ raw_frame(YUV420p, 1920x1080)                                             │
│     │                                                                      │
│     ├─▶ OpenVINO PrePostProcessor (if available)                          │
│     │      └─▶ YUV→BGR (JIT-compiled) + Letterbox + Normalize             │
│     │      └─▶ Returns: tensor ready for inference                        │
│     │                                                                      │
│     └─▶ Fallback: sws_scale → letterbox(960x540) → BGR Mat                │
│                                                                            │
│ MappingMeta {                                                              │
│   scale: 0.5,                                                              │
│   pad_left: 0,                                                             │
│   pad_top: 30,                                                             │
│   padding_value: 0  // [NEW v1.1] Model-expected mean                      │
│ }                                                                          │
│     │                                                                      │
│     ▼                                                                      │
│ YuNet.detect() ─▶ detections_inf[]                                        │
│     │                                                                      │
│     ▼                                                                      │
│ inverse_map(detections_inf, meta)                                          │
│     │                                                                      │
│     ▼                                                                      │
│ detections_raw[] ─▶ normalize() ─▶ [0..1]                                 │
│     │                                                                      │
│     ▼                                                                      │
│ tracker.update(detections_raw)                                             │
│     │                                                                      │
│     ▼                                                                      │
│ FaceTimeline.push(keyframe=true)                                           │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ FRAME N+1, N+2 (Gap frames)                                                │
│ ──────────────────────────                                                 │
│                                                                            │
│ (No YuNet inference)                                                       │
│                                                                            │
│ tracker.predict() ─▶ interpolated_positions ─▶ normalize()                │
│     │                                                                      │
│     ▼                                                                      │
│ FaceTimeline.push(keyframe=false, tracking_method="kalman")                │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ SCENE CUT DETECTED [NEW v1.1]                                              │
│ ─────────────────────────────                                              │
│                                                                            │
│ scene_cut_detector.is_cut(frame_n, frame_n+1) == true                      │
│     │                                                                      │
│     ▼                                                                      │
│ tracker.hard_reset()  // CRITICAL: Invalidate all Kalman states           │
│     │                                                                      │
│     ▼                                                                      │
│ Force keyframe detection on next frame                                     │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ Re-detect Triggers:                                                        │
│   • Every N frames (configurable, default: 5)                              │
│   • Scene cut detected [ENHANCED v1.1]                                     │
│   • Confidence < threshold                                                 │
│   • Predicted position drifted > threshold                                 │
│   • Face lost (track age exceeded)                                         │
│   • Track ID swap risk detected [NEW v1.1]                                 │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Fixed-Resolution Letterbox Inference

### 4.1 Letterbox Preprocessing Specification (v1.1)

**Fixed Canvas Size:** `(W_inf, H_inf) = (960, 540)` (configurable)

**[NEW v1.1] Padding Value:** Must use model-expected mean value for padding.
- YuNet expects padding_value = 0 (black)
- Some models expect 128 (gray mid-point)
- **Critical:** Using wrong padding_value degrades edge detection accuracy

**Algorithm:**
```
INPUT:  raw_frame of size (W_raw, H_raw)
OUTPUT: letterboxed_frame of size (W_inf, H_inf), MappingMeta

1. Compute scale factor (aspect-preserving):
   S = min(W_inf / W_raw, H_inf / H_raw)

2. Compute scaled dimensions:
   W_scaled = round(W_raw × S)
   H_scaled = round(H_raw × S)

3. Compute padding:
   P_left  = floor((W_inf - W_scaled) / 2)
   P_right = W_inf - W_scaled - P_left
   P_top   = floor((H_inf - H_scaled) / 2)
   P_bottom = H_inf - H_scaled - P_top

4. Apply:
   a) resize(raw_frame, (W_scaled, H_scaled), INTER_AREA)  // for downscale
   b) copyMakeBorder(resized, P_top, P_bottom, P_left, P_right,
                     BORDER_CONSTANT, color=(padding_value, padding_value, padding_value))

5. Return:
   MappingMeta {
     raw_width: W_raw,
     raw_height: H_raw,
     inf_width: W_inf,
     inf_height: H_inf,
     scale: S,
     pad_left: P_left,
     pad_top: P_top,
     padding_value: 0,     // [NEW v1.1] Model-expected mean
     scaled_width: W_scaled,
     scaled_height: H_scaled,
   }
```

**Interpolation Choice:**
- **Downscaling:** `INTER_AREA` (anti-aliasing, avoids Moiré patterns)
- **Upscaling (if ever needed):** `INTER_LINEAR` (good balance)
- **Tracking ops:** `INTER_LINEAR` (fast, sufficient for motion)
- **[NEW v1.1] Avoid INTER_CUBIC:** Adds CPU overhead with minimal AI gain

### 4.2 Inverse Mapping Specification (v1.1 - Mathematical Formulas)

**Point Mapping (inference → raw):**

$$x_{raw} = \text{clamp}\left(\frac{x_{inf} - P_{left}}{S}, 0, W_{raw}\right)$$

$$y_{raw} = \text{clamp}\left(\frac{y_{inf} - P_{top}}{S}, 0, H_{raw}\right)$$

Where:
- $S$ = scale factor
- $P_{left}$, $P_{top}$ = padding offsets
- $W_{raw}$, $H_{raw}$ = original frame dimensions

**Code Implementation:**
```rust
/// Map point from inference space to raw space.
///
/// Formula: x_raw = clamp((x_inf - P_left) / S, 0, W_raw)
#[inline]
pub fn map_point(&self, x_inf: f64, y_inf: f64) -> (f64, f64) {
    let x_raw = (x_inf - self.pad_left as f64) / self.scale;
    let y_raw = (y_inf - self.pad_top as f64) / self.scale;

    (
        x_raw.clamp(0.0, self.raw_width as f64 - 1.0),
        y_raw.clamp(0.0, self.raw_height as f64 - 1.0),
    )
}
```

**Rectangle Mapping:**
```
1. Map corners:
   (x1_raw, y1_raw) = map_point(x_inf, y_inf)
   (x2_raw, y2_raw) = map_point(x_inf + w_inf, y_inf + h_inf)

2. Clamp to frame bounds:
   x1_raw = clamp(x1_raw, 0, W_raw)
   y1_raw = clamp(y1_raw, 0, H_raw)
   x2_raw = clamp(x2_raw, 0, W_raw)
   y2_raw = clamp(y2_raw, 0, H_raw)

3. Compute final rect:
   w_raw = x2_raw - x1_raw
   h_raw = y2_raw - y1_raw
```

### 4.3 Required Tests

```rust
#[cfg(test)]
mod inverse_mapping_tests {
    // Round-trip: raw → inf → raw ≈ original (within 1px tolerance)
    #[test]
    fn test_round_trip_center_point();

    #[test]
    fn test_round_trip_corners();

    // Golden tests for aspect ratios
    #[test]
    fn test_16x9_source();    // Common YouTube

    #[test]
    fn test_9x16_source();    // Vertical video

    #[test]
    fn test_1x1_source();     // Square

    #[test]
    fn test_21x9_ultrawide(); // Ultrawide content

    // Edge cases
    #[test]
    fn test_detection_at_padding_boundary();

    #[test]
    fn test_clamp_behavior_outside_bounds();

    // Zero-bar verification
    #[test]
    fn test_no_padding_in_output_coordinates();

    // [NEW v1.1] Padding value tests
    #[test]
    fn test_padding_value_zero_for_yunet();

    #[test]
    fn test_edge_detection_accuracy_with_correct_padding();
}
```

---

## 5. Temporal Decimation & Tracking

### 5.1 Keyframe Detection Strategy

**Configuration:**
```rust
pub struct TemporalConfig {
    /// Run full YuNet every N frames
    pub detect_every_n: u32,  // default: 5

    /// OR time-based interval (takes precedence if set)
    pub detect_interval_ms: Option<u64>,  // e.g., 200ms

    /// Scene cut detection threshold (0.0-1.0)
    pub scene_cut_threshold: f64,  // default: 0.3

    /// Confidence below which to force re-detect
    pub min_confidence: f64,  // default: 0.4

    /// Position drift threshold (fraction of frame width)
    pub drift_threshold: f64,  // default: 0.15

    /// Max frames without detection before track loss
    pub max_gap_frames: u32,  // default: 30
}
```

**Re-detect Triggers:**
1. Every N frames (baseline)
2. **Scene cut detected** (histogram comparison or shot boundary) [ENHANCED v1.1]
3. Tracker confidence dropped below threshold
4. Predicted position drifted too far from last known
5. All tracks lost (no active faces)
6. **[NEW v1.1] Track ID swap risk** (overlapping tracks detected)

### 5.2 Tracking Algorithm Selection

**Recommendation: Kalman Filter**

| Method | Pros | Cons | Decision |
|--------|------|------|----------|
| Centroid + Velocity | Simple, fast | No uncertainty modeling | No |
| Kalman Filter | Smooth predictions, handles noise | Slightly more complex | **Yes** |
| Optical Flow | Accurate for small motion | CPU intensive, complex | No |
| SORT/DeepSORT | State-of-art MOT | Requires ReID model | No |

**Kalman Filter State:**
```
State vector: [cx, cy, w, h, vx, vy, vw, vh]
             (center, size, velocities)

Measurement: [cx, cy, w, h]
             (from YuNet detection)

Process model: constant velocity
Measurement model: direct observation of position/size
```

### 5.3 FaceTrack Implementation (v1.1)

```rust
pub struct FaceTrack {
    track_id: u32,
    kalman: KalmanFilter<8, 4>,  // 8-dim state, 4-dim measurement
    age: u32,
    hits: u32,
    time_since_update: u32,
    confidence: f64,
    /// [NEW v1.1] Hash of scene when track was created
    /// Used to invalidate on scene cuts
    scene_hash: u64,
}

impl FaceTrack {
    /// Predict next state (called on gap frames)
    pub fn predict(&mut self) -> BoundingBox;

    /// Update with detection (called on keyframes)
    pub fn update(&mut self, detection: &BoundingBox, confidence: f64);

    /// Get current estimated position
    pub fn get_state(&self) -> (BoundingBox, f64);  // (bbox, confidence)

    /// [NEW v1.1] Check if track is valid for current scene
    pub fn is_valid_for_scene(&self, current_scene_hash: u64) -> bool {
        self.scene_hash == current_scene_hash
    }
}
```

### 5.4 Scene-Cut Tracker Invalidation [NEW v1.1]

**CRITICAL:** Kalman tracker MUST hard-reset on scene cuts. Without this, cuts from Person A → Person B create "ghost" detections as Kalman drags old tracks.

```rust
impl KalmanTracker {
    /// Handle scene cut by invalidating all tracks.
    ///
    /// MUST be called when scene_cut_detector.is_cut() returns true.
    /// Without this, tracks "ghost" across cuts.
    pub fn handle_scene_cut(&mut self, new_scene_hash: u64) {
        tracing::info!(
            tracks_invalidated = self.tracks.len(),
            old_scene = self.current_scene_hash,
            new_scene = new_scene_hash,
            "Scene cut detected, hard-resetting tracker"
        );

        // HARD RESET: Clear all tracks
        self.tracks.clear();
        self.current_scene_hash = new_scene_hash;

        // Emit metric
        metrics::counter!("face_tracker_scene_cuts_total").increment(1);
    }

    /// Update with scene-cut awareness
    pub fn update_with_scene_check(
        &mut self,
        detections: &[(BoundingBox, f64)],
        timestamp_ms: u64,
        current_scene_hash: u64,
    ) -> Vec<(u32, BoundingBox, f64)> {
        // Check for scene cut
        if current_scene_hash != self.current_scene_hash {
            self.handle_scene_cut(current_scene_hash);
        }

        // Normal update
        self.update(detections, timestamp_ms)
    }
}
```

**Scene Hash Computation:**
```rust
/// Compute scene hash from frame histogram.
///
/// Simple but effective: uses color histogram to detect cuts.
fn compute_scene_hash(frame: &Mat) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    // Compute histogram (8 bins per channel = 512 bins total)
    let hist = compute_color_histogram(frame, 8);

    // Hash the histogram
    let mut hasher = DefaultHasher::new();
    for bin in hist.iter() {
        (*bin as u64).hash(&mut hasher);
    }
    hasher.finish()
}

/// Check if scene cut occurred.
fn is_scene_cut(prev_hash: u64, curr_hash: u64, threshold: f64) -> bool {
    // Simple XOR difference (could use histogram intersection instead)
    let diff_bits = (prev_hash ^ curr_hash).count_ones();
    let similarity = 1.0 - (diff_bits as f64 / 64.0);
    similarity < threshold  // threshold ~= 0.7 for scene cuts
}
```

### 5.5 Face Data Stream (FaceTimeline) Schema

```json
{
  "schema_version": "1.1.0",
  "video_id": "abc123",
  "source_width": 1920,
  "source_height": 1080,
  "inference_width": 960,
  "inference_height": 540,
  "detect_every_n": 5,
  "interval_ms": null,
  "backend_selected": "openvino",
  "model_id": "face_detection_yunet_2023mar_int8bq.onnx",
  "model_hash": "sha256:abc123...",
  "padding_value": 0,
  "detections": [
    {
      "timestamp_ms": 0,
      "bbox_norm": {
        "x": 0.45,
        "y": 0.30,
        "w": 0.10,
        "h": 0.18
      },
      "landmarks_norm": [
        {"x": 0.47, "y": 0.33},
        {"x": 0.53, "y": 0.33},
        {"x": 0.50, "y": 0.38},
        {"x": 0.48, "y": 0.42},
        {"x": 0.52, "y": 0.42}
      ],
      "confidence": 0.95,
      "track_id": 0,
      "is_keyframe": true,
      "tracking_method": null,
      "scene_id": 0
    },
    {
      "timestamp_ms": 33,
      "bbox_norm": { "x": 0.451, "y": 0.301, "w": 0.10, "h": 0.18 },
      "landmarks_norm": null,
      "confidence": 0.92,
      "track_id": 0,
      "is_keyframe": false,
      "tracking_method": "kalman",
      "scene_id": 0
    }
  ],
  "scene_cuts": [
    {"timestamp_ms": 5000, "scene_id": 1}
  ]
}
```

---

## 6. OpenVINO-First Backend Policy

### 6.1 Backend Selection Logic

```rust
pub enum InferenceBackend {
    OpenVINO,
    OpenCVDnn,
}

pub struct BackendMetrics {
    pub backend: InferenceBackend,
    pub initialization_time_ms: u64,
    pub avg_inference_time_ms: f64,
    pub cpu_features_used: Vec<String>,
    pub uses_vnni: bool,  // [NEW v1.1]
}

impl FaceInferenceEngine {
    pub fn select_backend() -> (InferenceBackend, BackendMetrics) {
        // 1. Try OpenVINO (DNN_BACKEND_INFERENCE_ENGINE)
        if Self::try_openvino().is_ok() {
            return (InferenceBackend::OpenVINO, metrics);
        }

        // 2. Fallback to OpenCV DNN
        if Self::try_opencv_dnn().is_ok() {
            return (InferenceBackend::OpenCVDnn, metrics);
        }

        panic!("No suitable inference backend available");
    }

    fn try_openvino() -> Result<(), BackendError> {
        use opencv::dnn::{DNN_BACKEND_INFERENCE_ENGINE, DNN_TARGET_CPU};

        FaceDetectorYN::create(
            model_path,
            "",
            Size::new(960, 540),
            0.3,
            0.3,
            10,
            DNN_BACKEND_INFERENCE_ENGINE,
            DNN_TARGET_CPU,
        )?;

        Ok(())
    }
}
```

### 6.2 Metrics Export

```rust
// Prometheus metrics
static BACKEND_TYPE: Lazy<IntGaugeVec> = Lazy::new(|| {
    IntGaugeVec::new(
        Opts::new("face_inference_backend", "Active inference backend"),
        &["backend", "uses_vnni"]  // [NEW v1.1]
    ).unwrap()
});

static INFERENCE_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    HistogramVec::new(
        HistogramOpts::new("face_inference_latency_ms", "Face detection inference latency")
            .buckets(vec![0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0]),  // [v1.1] Added sub-2ms buckets
        &["backend", "is_keyframe", "model_type"]
    ).unwrap()
});
```

---

## 7. OpenVINO Preprocessing API (NEW v1.1)

### 7.1 Problem: sws_scale Overhead

Currently, the plan uses FFmpeg's `sws_scale` for YUV420p → BGR conversion. This is suboptimal because:
- sws_scale is not integrated with inference pipeline
- Two separate operations: convert → infer
- Memory copy between conversion and inference

### 7.2 Solution: OpenVINO PrePostProcessor

OpenVINO has a **Preprocessing API** (`ov::preprocess::PrePostProcessor`) that can:
- Take YUV420p (NV12/I420) directly as input
- Perform color conversion inside the execution graph
- Use JIT-compiled kernels optimized for the target CPU
- Often faster than sws_scale

**Architecture with PrePostProcessor:**
```
AVFrame (YUV420p)
    │
    │ Zero-copy wrap into ov::Tensor
    ▼
OpenVINO PrePostProcessor
    │
    ├── input().tensor()
    │   └── set_element_type(u8)
    │   └── set_color_format(NV12_TWO_PLANES or I420)
    │   └── set_spatial_static_shape(1080, 1920)
    │
    ├── input().preprocess()
    │   └── convert_color(BGR)
    │   └── resize(ResizeAlgorithm::RESIZE_LINEAR, 540, 960)
    │   └── convert_layout({0,3,1,2})  // NHWC → NCHW
    │
    └── output()
        └── set_element_type(f32)
```

### 7.3 Implementation

```cpp
// C++ OpenVINO API (called via FFI from Rust)
ov::Core core;
auto model = core.read_model("yunet_int8.onnx");

// Configure preprocessing
ov::preprocess::PrePostProcessor ppp(model);

// Input: YUV420p (I420 format - 3 planes)
auto& input = ppp.input();
input.tensor()
    .set_element_type(ov::element::u8)
    .set_color_format(ov::preprocess::ColorFormat::I420)
    .set_spatial_static_shape(1080, 1920);

// Preprocessing steps (all JIT-compiled)
input.preprocess()
    .convert_color(ov::preprocess::ColorFormat::BGR)
    .resize(ov::preprocess::ResizeAlgorithm::RESIZE_LINEAR, 540, 960)
    .mean({0.0f, 0.0f, 0.0f})  // YuNet normalization
    .scale({1.0f, 1.0f, 1.0f});

// Build optimized model
auto optimized_model = ppp.build();
auto compiled_model = core.compile_model(optimized_model, "CPU");
```

```rust
// Rust wrapper
pub struct OpenVinoYuNet {
    compiled_model: CompiledModel,
    infer_request: InferRequest,
    uses_preprocessing_api: bool,
}

impl OpenVinoYuNet {
    /// Create with preprocessing API for direct YUV input.
    pub fn new_with_yuv_preprocessing(model_path: &str) -> Result<Self, Error> {
        // ... FFI call to create model with ppp
    }

    /// Infer directly from YUV420p AVFrame (zero-copy if aligned).
    pub fn infer_yuv(&mut self, frame: &AVFrame) -> Result<Vec<Detection>, Error> {
        // Set input tensor pointing to AVFrame planes
        let y_plane = unsafe { frame.data[0] };
        let u_plane = unsafe { frame.data[1] };
        let v_plane = unsafe { frame.data[2] };

        // Create tensor views (no copy)
        // ... FFI calls
    }
}
```

### 7.4 Benchmark Comparison

| Method | 1080p YUV→BGR+Resize | Notes |
|--------|---------------------|-------|
| sws_scale + resize | ~2.5ms | Current approach |
| OpenVINO PreProc | ~0.8ms | JIT-compiled kernels |
| **Improvement** | **3x faster** | Saves ~1.7ms/frame |

**Action Item:** Benchmark OpenVINO PrePostProcessor vs sws_scale on 1080p@30fps in PR #3.

---

## 8. INT8 Quantization & VNNI Optimization (NEW v1.1)

### 8.1 INT8 Model Selection

The plan uses `face_detection_yunet_2023mar_int8bq.onnx` (block-quantized INT8).

**Key Requirements for Maximum Performance:**

1. **OpenVINO Build with NNCF Support**
   - NNCF (Neural Network Compression Framework) enables advanced INT8 inference
   - Required for optimal performance on AVX-512 VNNI CPUs

2. **AVX-512 VNNI (Vector Neural Network Instructions)**
   - Available on: Skylake-X, Cascade Lake, Ice Lake, Sapphire Rapids
   - Provides 2-4x INT8 throughput vs regular AVX-512
   - YuNet INT8 on VNNI: **sub-2ms inference at 640x360**

### 8.2 Performance by CPU Feature

| CPU Feature | INT8 Inference (640x360) | FP32 Inference |
|-------------|-------------------------|----------------|
| SSE3 (baseline) | ~8ms | ~25ms |
| AVX2 | ~4ms | ~12ms |
| AVX-512 | ~2.5ms | ~8ms |
| **AVX-512 VNNI** | **<2ms** | ~8ms |

### 8.3 Padding Value for INT8 Models

**CRITICAL:** Letterbox padding MUST use model-expected mean value.

```rust
/// MappingMeta with padding value for INT8 accuracy
#[derive(Debug, Clone, Copy)]
pub struct MappingMeta {
    // ... existing fields ...

    /// [NEW v1.1] Padding value for letterbox border.
    /// YuNet expects 0 (black). Wrong value degrades edge accuracy.
    pub padding_value: u8,
}

impl MappingMeta {
    pub fn for_yunet(raw_w: u32, raw_h: u32, inf_w: u32, inf_h: u32) -> Self {
        let mut meta = Self::compute(raw_w, raw_h, inf_w, inf_h);
        meta.padding_value = 0;  // YuNet: black padding
        meta
    }
}
```

### 8.4 OpenVINO Model Optimization

```bash
# Convert ONNX to OpenVINO IR with INT8 optimization
mo --input_model face_detection_yunet_2023mar_int8bq.onnx \
   --compress_to_fp16=False \
   --output_dir ./openvino_models

# Or use OpenVINO's NNCF for further optimization
pot -q default \
    -m face_detection_yunet.xml \
    -w face_detection_yunet.bin \
    --output-dir ./nncf_optimized
```

### 8.5 Runtime VNNI Detection

```rust
impl CpuFeatures {
    pub fn detect() -> Self {
        Self {
            avx2: is_x86_feature_detected!("avx2"),
            avx512f: is_x86_feature_detected!("avx512f"),
            avx512bw: is_x86_feature_detected!("avx512bw"),
            avx512vl: is_x86_feature_detected!("avx512vl"),
            avx512_vnni: is_x86_feature_detected!("avx512vnni"),  // [NEW v1.1]
        }
    }

    /// Log CPU capabilities for diagnostics
    pub fn log_capabilities(&self) {
        tracing::info!(
            avx2 = self.avx2,
            avx512 = self.avx512f,
            vnni = self.avx512_vnni,
            "CPU feature detection"
        );

        if self.avx512_vnni {
            tracing::info!("AVX-512 VNNI available: INT8 inference will be optimal");
        }
    }
}
```

---

## 9. Build & Docker Requirements

### 9.1 OpenCV CMake Flags

**CRITICAL: OpenVINO Integration with NNCF**

```cmake
# Core OpenVINO flags (OpenVINO 2022.1+)
-D WITH_OPENVINO=ON
-D OpenVINO_DIR=/opt/intel/openvino/runtime/cmake

# [NEW v1.1] Ensure NNCF support for INT8 optimization
# NNCF is included in OpenVINO 2024.x by default

# DEPRECATED - DO NOT USE:
# -D WITH_INF_ENGINE=ON  # Legacy, replaced by WITH_OPENVINO

# Parallelism
-D WITH_TBB=ON
-D WITH_OPENMP=OFF  # Use TBB, not OpenMP

# IPP (Intel Performance Primitives)
-D WITH_IPP=ON  # Recommended for Intel CPUs
-D BUILD_IPP_IW=ON

# Disable GUI (not needed for server)
-D WITH_GTK=OFF
-D WITH_QT=OFF
```

### 9.2 CPU ISA Profiles

#### Profile 1: PORTABLE (Safe Across Mixed Fleets)

```cmake
-D CPU_BASELINE=AVX2
-D CPU_DISPATCH=
-D ENABLE_AVX2=ON
-D ENABLE_AVX512=OFF
```

**Target CPUs:**
- Any x86_64 from ~2013+ (Haswell and later)
- AMD Excavator (2015+), Zen (2017+)
- All modern cloud instances (AWS, GCP, Azure)

**Use Case:** Default production image, works everywhere

#### Profile 2: TUNED (Pinned Fleet Only - AVX-512 VNNI)

```cmake
-D CPU_BASELINE=AVX2
-D CPU_DISPATCH=AVX512_SKX,AVX512_ICL
-D ENABLE_AVX512=ON
```

**Target CPUs:**
- Intel Skylake-X, Cascade Lake, Ice Lake, Sapphire Rapids (with VNNI)
- AMD EPYC 7002/7003/7004 (Zen 2/3/4)

**Runtime Guard (MUST IMPLEMENT):**
```rust
fn verify_cpu_features() -> Result<(), CpuMismatchError> {
    #[cfg(target_arch = "x86_64")]
    {
        if !is_x86_feature_detected!("avx512f") {
            return Err(CpuMismatchError::MissingFeature("avx512f"));
        }
        if !is_x86_feature_detected!("avx512bw") {
            return Err(CpuMismatchError::MissingFeature("avx512bw"));
        }
        // [NEW v1.1] Check for VNNI for optimal INT8 perf
        if !is_x86_feature_detected!("avx512vnni") {
            tracing::warn!("AVX-512 VNNI not available, INT8 will be suboptimal");
        }
    }
    Ok(())
}
```

### 9.3 Complete Dockerfile Changes

```dockerfile
# =============================================================================
# Stage: OpenCV Builder with OpenVINO (v1.1)
# =============================================================================
FROM ubuntu:24.04 AS opencv-builder

ARG OPENCV_VERSION=4.12.0
ARG OPENVINO_VERSION=2024.4

# Install OpenVINO (includes NNCF)
RUN curl -fsSL https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB \
    | gpg --dearmor -o /usr/share/keyrings/intel-openvino.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/intel-openvino.gpg] https://apt.repos.intel.com/openvino/2024 ubuntu24 main" \
    > /etc/apt/sources.list.d/intel-openvino.list \
    && apt-get update \
    && apt-get install -y openvino-${OPENVINO_VERSION}

# OpenCV build dependencies
RUN apt-get install -y \
    cmake ninja-build \
    libtbb-dev libopenblas-dev \
    libavcodec-dev libavformat-dev libswscale-dev \
    libpng-dev libtiff-dev

# Build OpenCV with OpenVINO
WORKDIR /opencv
RUN git clone --depth 1 -b ${OPENCV_VERSION} https://github.com/opencv/opencv.git \
    && git clone --depth 1 -b ${OPENCV_VERSION} https://github.com/opencv/opencv_contrib.git

WORKDIR /opencv/build-portable
RUN cmake -G Ninja ../opencv \
    -D CMAKE_BUILD_TYPE=Release \
    -D CMAKE_INSTALL_PREFIX=/usr/local \
    -D OPENCV_EXTRA_MODULES_PATH=../opencv_contrib/modules \
    \
    # OpenVINO Integration
    -D WITH_OPENVINO=ON \
    \
    # CPU Baseline: Portable (AVX2)
    -D CPU_BASELINE=AVX2 \
    -D CPU_DISPATCH= \
    \
    # Parallelism
    -D WITH_TBB=ON \
    -D WITH_OPENMP=OFF \
    \
    # Intel Performance Primitives
    -D WITH_IPP=ON \
    -D BUILD_IPP_IW=ON \
    \
    # Core modules only (minimize size)
    -D BUILD_opencv_world=OFF \
    -D BUILD_opencv_python=OFF \
    -D BUILD_TESTS=OFF \
    -D BUILD_PERF_TESTS=OFF \
    -D BUILD_EXAMPLES=OFF \
    \
    && ninja -j$(nproc) \
    && ninja install \
    && ninja clean

# Create artifact tarball
RUN tar -czf /opencv-4.12.0-openvino-portable.tar.gz -C /usr/local .

# [NEW v1.1] Store build info for validation
RUN python3 -c "import cv2; print(cv2.getBuildInformation())" > /opencv-build-info.txt
```

### 9.4 CI Verification

```yaml
# .github/workflows/opencv-build-verification.yml
jobs:
  verify-opencv-build:
    runs-on: ubuntu-24.04
    steps:
      - name: Extract OpenCV artifact
        run: tar -xzf opencv-artifacts/opencv-4.12.0-ubuntu24.04-amd64.tar.gz -C /usr/local

      - name: Verify OpenVINO backend
        run: |
          python3 -c "
          import cv2
          info = cv2.getBuildInformation()
          print(info)
          assert 'OpenVINO:' in info and 'YES' in info.split('OpenVINO:')[1].split('\n')[0], \
              'OpenVINO not enabled in build'
          "

      - name: Verify CPU baseline
        run: |
          python3 -c "
          import cv2
          info = cv2.getBuildInformation()
          assert 'CPU_BASELINE:' in info
          baseline = info.split('CPU_BASELINE:')[1].split('\n')[0]
          print(f'CPU Baseline: {baseline}')
          assert 'AVX2' in baseline, 'AVX2 baseline not set'
          "

      # [NEW v1.1] Store build info as artifact
      - name: Store build info artifact
        run: |
          python3 -c "import cv2; print(cv2.getBuildInformation())" > opencv-build-info.txt

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: opencv-build-info
          path: opencv-build-info.txt
```

---

## 10. Zero-Copy / Shared-Memory Evaluation

### 10.1 Current Memory Flow (Copies Identified)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ CURRENT PIPELINE (ESTIMATED COPIES)                                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│ FFmpeg Decoder                                                          │
│     │                                                                   │
│     ▼                                                                   │
│ AVFrame (YUV420p, 1920x1080)                                           │
│     │                                                                   │
│     │ COPY #1: sws_scale YUV→BGR (AVOIDABLE with OpenVINO PreProc)     │
│     ▼                                                                   │
│ OpenCV Mat (BGR, 1920x1080)                                            │
│     │                                                                   │
│     │ COPY #2: imgproc::resize() → resized Mat                         │
│     ▼                                                                   │
│ Resized Mat (BGR, 960x540)                                             │
│     │                                                                   │
│     │ COPY #3: detector.detect() internal preprocessing                │
│     ▼                                                                   │
│ DNN Blob (float32, NCHW)                                               │
│                                                                         │
│ TOTAL: 3 full-frame copies per detection                               │
│        @ 1080p: 3 × 6.2MB = 18.6MB bandwidth per frame                 │
│        @ 30fps keyframes: 558 MB/s memory bandwidth                    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 10.2 v1.1 Optimized Pipeline with OpenVINO PreProc

```
┌─────────────────────────────────────────────────────────────────────────┐
│ v1.1 OPTIMIZED PIPELINE                                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│ FFmpeg Decoder                                                          │
│     │                                                                   │
│     ▼                                                                   │
│ AVFrame (YUV420p, 1920x1080)                                           │
│     │                                                                   │
│     │ ZERO-COPY: Wrap Y/U/V planes as ov::Tensor                       │
│     ▼                                                                   │
│ OpenVINO PrePostProcessor                                              │
│     │                                                                   │
│     │ JIT-compiled: YUV→BGR + Resize + Normalize (SINGLE PASS)         │
│     ▼                                                                   │
│ Inference Tensor (ready for YuNet)                                     │
│                                                                         │
│ TOTAL: 1 fused operation (vs 3 copies before)                          │
│        Memory bandwidth: 558 MB/s → ~200 MB/s (64% reduction)          │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 10.3 AVFrame to OpenVINO Tensor (Zero-Copy)

```rust
/// Zero-copy wrapper from AVFrame to OpenVINO tensor.
pub struct AvFrameOpenVinoView<'a> {
    /// Y plane tensor
    y_tensor: ov::Tensor,
    /// UV plane tensor (for NV12) or U/V tensors (for I420)
    uv_tensors: UvTensors,
    /// Phantom to tie lifetime
    _marker: PhantomData<&'a AVFrame>,
}

enum UvTensors {
    Nv12(ov::Tensor),
    I420 { u: ov::Tensor, v: ov::Tensor },
}

impl<'a> AvFrameOpenVinoView<'a> {
    /// Create zero-copy tensor view of AVFrame.
    pub fn try_new(frame: &'a AVFrame) -> Result<Self, FrameError> {
        // Validate format
        match frame.format {
            AV_PIX_FMT_NV12 => Self::from_nv12(frame),
            AV_PIX_FMT_YUV420P => Self::from_i420(frame),
            _ => Err(FrameError::UnsupportedFormat),
        }
    }

    fn from_nv12(frame: &'a AVFrame) -> Result<Self, FrameError> {
        // Y plane: height × width
        let y_tensor = unsafe {
            ov::Tensor::new_with_data(
                ov::element::u8,
                ov::Shape::new(&[1, frame.height as usize, frame.width as usize, 1]),
                frame.data[0] as *mut _,
                frame.linesize[0] as usize,
            )?
        };

        // UV plane: (height/2) × (width)
        let uv_tensor = unsafe {
            ov::Tensor::new_with_data(
                ov::element::u8,
                ov::Shape::new(&[1, (frame.height/2) as usize, frame.width as usize, 1]),
                frame.data[1] as *mut _,
                frame.linesize[1] as usize,
            )?
        };

        Ok(Self {
            y_tensor,
            uv_tensors: UvTensors::Nv12(uv_tensor),
            _marker: PhantomData,
        })
    }
}
```

---

## 11. Rust Implementation Design

### 11.1 FaceInferenceEngine (v1.1)

```rust
//! Face inference engine with fixed-resolution letterbox and temporal decimation.

use opencv::prelude::*;
use std::sync::Arc;

/// Thread-local detector instance (never recreate per frame).
thread_local! {
    static DETECTOR: RefCell<Option<YuNetDetector>> = RefCell::new(None);
}

/// Mapping metadata for coordinate transformation.
#[derive(Debug, Clone, Copy)]
pub struct MappingMeta {
    pub raw_width: u32,
    pub raw_height: u32,
    pub inf_width: u32,
    pub inf_height: u32,
    pub scale: f64,
    pub pad_left: i32,
    pub pad_top: i32,
    pub scaled_width: i32,
    pub scaled_height: i32,
    /// [NEW v1.1] Padding value for letterbox border
    pub padding_value: u8,
}

impl MappingMeta {
    /// Compute letterbox mapping for given dimensions.
    pub fn compute(raw_w: u32, raw_h: u32, inf_w: u32, inf_h: u32) -> Self {
        let scale = (inf_w as f64 / raw_w as f64)
            .min(inf_h as f64 / raw_h as f64);

        let scaled_w = (raw_w as f64 * scale).round() as i32;
        let scaled_h = (raw_h as f64 * scale).round() as i32;

        let pad_left = (inf_w as i32 - scaled_w) / 2;
        let pad_top = (inf_h as i32 - scaled_h) / 2;

        Self {
            raw_width: raw_w,
            raw_height: raw_h,
            inf_width: inf_w,
            inf_height: inf_h,
            scale,
            pad_left,
            pad_top,
            scaled_width: scaled_w,
            scaled_height: scaled_h,
            padding_value: 0,  // YuNet default
        }
    }

    /// [NEW v1.1] Create for YuNet with correct padding value
    pub fn for_yunet(raw_w: u32, raw_h: u32, inf_w: u32, inf_h: u32) -> Self {
        let mut meta = Self::compute(raw_w, raw_h, inf_w, inf_h);
        meta.padding_value = 0;  // YuNet expects black padding
        meta
    }

    /// Map point from inference space to raw space.
    /// Formula: x_raw = clamp((x_inf - P_left) / S, 0, W_raw)
    #[inline]
    pub fn map_point(&self, x_inf: f64, y_inf: f64) -> (f64, f64) {
        let x_raw = (x_inf - self.pad_left as f64) / self.scale;
        let y_raw = (y_inf - self.pad_top as f64) / self.scale;

        (
            x_raw.clamp(0.0, self.raw_width as f64 - 1.0),
            y_raw.clamp(0.0, self.raw_height as f64 - 1.0),
        )
    }

    /// Map bounding box from inference space to raw space.
    pub fn map_rect(&self, bbox_inf: &BoundingBox) -> BoundingBox {
        let (x1, y1) = self.map_point(bbox_inf.x, bbox_inf.y);
        let (x2, y2) = self.map_point(bbox_inf.x + bbox_inf.width, bbox_inf.y + bbox_inf.height);

        let x1 = x1.clamp(0.0, self.raw_width as f64);
        let y1 = y1.clamp(0.0, self.raw_height as f64);
        let x2 = x2.clamp(0.0, self.raw_width as f64);
        let y2 = y2.clamp(0.0, self.raw_height as f64);

        BoundingBox::new(x1, y1, x2 - x1, y2 - y1)
    }

    /// Normalize coordinates to [0, 1] range.
    pub fn normalize(&self, bbox: &BoundingBox) -> NormalizedBBox {
        NormalizedBBox {
            x: bbox.x / self.raw_width as f64,
            y: bbox.y / self.raw_height as f64,
            w: bbox.width / self.raw_width as f64,
            h: bbox.height / self.raw_height as f64,
        }
    }
}

/// Face inference engine with thread-safe hot-path design.
pub struct FaceInferenceEngine {
    inf_size: (i32, i32),
    converter: FrameConverter,
    tracker: KalmanTracker,
    temporal_config: TemporalConfig,
    frame_count: u64,
    last_keyframe: u64,
    backend: InferenceBackend,
    /// [NEW v1.1] Current scene hash for cut detection
    current_scene_hash: u64,
}

impl FaceInferenceEngine {
    /// Create engine with fixed inference size.
    pub fn new(inf_width: i32, inf_height: i32) -> Result<Self, EngineError> {
        let backend = Self::select_backend(inf_width, inf_height)?;

        // [NEW v1.1] Log startup configuration
        tracing::info!(
            backend = ?backend,
            input_size = format!("{}x{}", inf_width, inf_height),
            cpu_features = ?CpuFeatures::detect(),
            "FaceInferenceEngine initialized"
        );

        Ok(Self {
            inf_size: (inf_width, inf_height),
            converter: FrameConverter::new(inf_width, inf_height),
            tracker: KalmanTracker::new(TemporalConfig::default()),
            temporal_config: TemporalConfig::default(),
            frame_count: 0,
            last_keyframe: 0,
            backend,
            current_scene_hash: 0,
        })
    }

    /// [NEW v1.1] Process frame with scene-cut awareness
    pub fn process_frame(
        &mut self,
        frame: &Mat,
        timestamp_ms: u64,
    ) -> Result<Vec<FaceDetection>, EngineError> {
        // Compute scene hash for cut detection
        let scene_hash = compute_scene_hash(frame);

        // Check for scene cut
        if self.is_scene_cut(scene_hash) {
            self.handle_scene_cut(scene_hash);
            // Force keyframe on scene cut
            return self.detect_keyframe(frame, timestamp_ms, scene_hash);
        }

        // Normal processing
        if self.should_detect() {
            self.detect_keyframe(frame, timestamp_ms, scene_hash)
        } else {
            Ok(self.track_gap_frame(timestamp_ms))
        }
    }

    fn is_scene_cut(&self, new_hash: u64) -> bool {
        if self.current_scene_hash == 0 {
            return false;  // First frame
        }
        let diff = (self.current_scene_hash ^ new_hash).count_ones();
        diff > 20  // Threshold: ~30% of bits differ
    }

    fn handle_scene_cut(&mut self, new_hash: u64) {
        tracing::info!(
            old_hash = self.current_scene_hash,
            new_hash = new_hash,
            tracks_cleared = self.tracker.active_count(),
            "Scene cut detected, resetting tracker"
        );

        self.tracker.hard_reset();
        self.current_scene_hash = new_hash;

        metrics::counter!("face_inference_scene_cuts").increment(1);
    }

    /// Process keyframe: full YuNet detection.
    fn detect_keyframe(
        &mut self,
        frame: &Mat,
        timestamp_ms: u64,
        scene_hash: u64,
    ) -> Result<Vec<FaceDetection>, EngineError> {
        let start = std::time::Instant::now();

        // 1. Letterbox preprocess (uses pre-allocated buffers)
        let (letterboxed, meta) = self.converter.letterbox(frame)?;

        // 2. Run YuNet detection
        let detections_inf = DETECTOR.with(|d| {
            let mut detector = d.borrow_mut();
            detector.as_mut().unwrap().detect(letterboxed)
        })?;

        // 3. Inverse map to raw coordinates
        let detections_raw: Vec<_> = detections_inf
            .iter()
            .map(|(bbox_inf, score)| {
                let bbox_raw = meta.map_rect(bbox_inf);
                (bbox_raw, *score)
            })
            .collect();

        // 4. Update tracker with scene hash
        let tracked = self.tracker.update_with_scene(
            &detections_raw,
            timestamp_ms,
            scene_hash,
        );

        // 5. Normalize to [0,1]
        let normalized: Vec<FaceDetection> = tracked
            .iter()
            .map(|(track_id, bbox, score)| {
                FaceDetection {
                    timestamp_ms,
                    bbox_norm: meta.normalize(bbox),
                    landmarks_norm: None,
                    confidence: *score,
                    track_id: *track_id,
                    is_keyframe: true,
                    tracking_method: None,
                    scene_id: scene_hash,
                }
            })
            .collect();

        self.last_keyframe = self.frame_count;
        self.frame_count += 1;
        self.current_scene_hash = scene_hash;

        // Record latency
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        metrics::histogram!("face_inference_latency_ms", "is_keyframe" => "true")
            .record(latency_ms);

        Ok(normalized)
    }

    /// Process gap frame: tracker prediction only (no YuNet).
    fn track_gap_frame(&mut self, timestamp_ms: u64) -> Vec<FaceDetection> {
        let predictions = self.tracker.predict(timestamp_ms);

        let normalized: Vec<FaceDetection> = predictions
            .iter()
            .map(|(track_id, bbox, confidence)| {
                FaceDetection {
                    timestamp_ms,
                    bbox_norm: NormalizedBBox::from_raw(bbox, self.converter.raw_dims()),
                    landmarks_norm: None,
                    confidence: *confidence,
                    track_id: *track_id,
                    is_keyframe: false,
                    tracking_method: Some(TrackingMethod::Kalman),
                    scene_id: self.current_scene_hash,
                }
            })
            .collect();

        self.frame_count += 1;
        normalized
    }

    /// Check if current frame should be a keyframe.
    fn should_detect(&self) -> bool {
        // Every N frames
        if self.frame_count - self.last_keyframe >= self.temporal_config.detect_every_n as u64 {
            return true;
        }

        // Confidence dropped
        if self.tracker.min_confidence() < self.temporal_config.min_confidence {
            return true;
        }

        // No active tracks
        if self.tracker.active_count() == 0 {
            return true;
        }

        false
    }
}
```

---

## 12. NUMA Awareness Strategy

### 12.1 The Problem

On dual-socket systems (e.g., Dual EPYC 7282):
- If decoder runs on Socket 0 but inference runs on Socket 1
- Frame data must traverse interconnect (Infinity Fabric/QPI)
- **Result:** ~100ns latency per cache line vs ~10ns local
- Zero-copy gains destroyed by NUMA penalty

### 12.2 v1.1 Refinement: Virtual NUMA Node Detection

**[NEW v1.1]** Many cloud instances (AWS C6i, C7g, etc.) present as a single virtual NUMA node even on multi-socket hardware. Complex mbind() is wasteful in these cases.

```rust
/// NUMA-aware thread affinity for video processing.
pub struct NumaAwareWorker {
    numa_node: u32,
    cores: Vec<u32>,
    is_virtual_numa: bool,  // [NEW v1.1]
}

impl NumaAwareWorker {
    /// Create worker with NUMA awareness.
    ///
    /// [NEW v1.1] Detects virtual NUMA nodes (common in cloud VMs)
    /// and skips complex affinity when not beneficial.
    pub fn new(numa_node: u32) -> Result<Self, NumaError> {
        #[cfg(target_os = "linux")]
        {
            // Check for virtual NUMA (single node even on multi-socket)
            let is_virtual_numa = Self::detect_virtual_numa()?;

            if is_virtual_numa {
                tracing::info!(
                    "Virtual NUMA detected (likely cloud VM), skipping complex affinity"
                );
                return Ok(Self {
                    numa_node: 0,
                    cores: vec![],
                    is_virtual_numa: true,
                });
            }

            // Real NUMA: get cores for this node
            let cores = numa_node_to_cpus(numa_node)?;
            set_thread_affinity(&cores)?;

            Ok(Self {
                numa_node,
                cores,
                is_virtual_numa: false,
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                numa_node: 0,
                cores: vec![],
                is_virtual_numa: true,
            })
        }
    }

    /// Detect virtual NUMA (single node on multi-socket).
    #[cfg(target_os = "linux")]
    fn detect_virtual_numa() -> Result<bool, NumaError> {
        use std::process::Command;

        // Run numactl --hardware
        let output = Command::new("numactl")
            .arg("--hardware")
            .output()
            .map_err(|_| NumaError::NumactlNotAvailable)?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Count "node X cpus:" lines
        let node_count = stdout.lines()
            .filter(|l| l.contains("node") && l.contains("cpus:"))
            .count();

        // Virtual NUMA: only one node visible
        Ok(node_count <= 1)
    }
}
```

### 12.3 Deployment Consideration

```yaml
# docker-compose.yml for NUMA-aware deployment
services:
  worker-node0:
    image: viralclip-worker:latest
    cpuset: "0-15"  # Socket 0 cores only
    environment:
      NUMA_NODE: "0"
      # [NEW v1.1] Skip NUMA complexity on cloud VMs
      NUMA_AUTO_DETECT: "true"
    deploy:
      resources:
        limits:
          cpus: "16"
          memory: "32G"
```

---

## 13. PR-by-PR Implementation Plan (Reordered for Max ROI)

### v1.1 PR Order (Max ROI First)

The original v1.0 plan started with letterbox/mapping. **v1.1 reorders to get performance validation immediately:**

```
v1.0 Order:                    v1.1 Order (Max ROI):
1. Letterbox + Mapping     →   1. ISA/Build (OpenCV+OpenVINO)  ← MEASURE REAL PERF
2. Buffer Pooling          →   2. Letterbox + Fixed Canvas     ← STABILITY WIN
3. Kalman Tracker          →   3. Temporal Decimation          ← 5x THROUGHPUT
4. Temporal Decimation     →   4. Buffer Pooling/Zero-Copy     ← MEMORY OPT
5. OpenCV+OpenVINO Build   →   5. Scene-Cut Tracker Reset      ← ACCURACY
6. Backend Selection       →   6. Kalman Tracker               ← SMOOTH TRACKING
7-10. Remaining            →   7-10. Remaining
```

### Phase 1: Build Infrastructure (2 PRs) - DO FIRST

**PR 1.1: OpenCV Build with OpenVINO + ISA Profiles**
```
Files:
  + opencv-build/Dockerfile.openvino  # OpenCV build with OpenVINO
  + opencv-build/build-portable.sh    # AVX2 baseline
  + opencv-build/build-tuned.sh       # AVX-512 with VNNI
  ~ Dockerfile                        # Use new OpenCV artifacts
  ~ docker-compose.yml

CI:
  + .github/workflows/opencv-build.yml
  + .github/workflows/opencv-build-verification.yml

Artifacts:
  + opencv-build-info.txt (REQUIRED - validates OpenVINO+AVX2)
```

**PR 1.2: Runtime Backend Selection + CPU Guard**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    + backend.rs           # InferenceBackend, selection logic
    + cpu_features.rs      # CPU detection, VNNI check
    ~ yunet.rs             # Use backend selection

Tests:
  + OpenVINO backend test
  + CPU feature guard test
  + VNNI detection test
```

### Phase 2: Fixed Letterbox (1 PR)

**PR 2.1: MappingMeta + Letterbox + Padding Value**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    + mapping.rs           # MappingMeta with padding_value
    + letterbox.rs         # Fixed-size letterbox with copyMakeBorder
    ~ yunet.rs             # Use new letterbox, remove dynamic sizing

Tests:
  + Round-trip mapping tests
  + Golden tests for 16:9, 9:16, 1:1, 21:9
  + Padding value accuracy test
```

### Phase 3: Temporal Decimation (2 PRs)

**PR 3.1: Temporal Decimation Core**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    + temporal.rs          # TemporalConfig, keyframe/gap logic
    + scene_cut.rs         # Scene hash computation, cut detection
    ~ detector.rs          # Integrate temporal logic

Tests:
  + Keyframe trigger tests
  + Scene cut detection tests
```

**PR 3.2: Scene-Cut Tracker Reset**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    ~ tracker.rs           # Add hard_reset(), scene awareness
    + kalman_tracker.rs    # KalmanTracker with scene_hash

Tests:
  + Ghost detection prevention test
  + Scene cut → tracker reset test
```

### Phase 4: Buffer Pooling (1 PR)

**PR 4.1: Buffer Pooling and Pre-allocation**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    + frame_converter.rs   # FrameConverter with pooled Mats
    ~ yunet.rs             # Use FrameConverter

Tests:
  + Allocation profiler test (heaptrack/dhat)
  + Zero allocations in steady state
```

### Phase 5: Zero-Copy and OpenVINO PreProc (2 PRs)

**PR 5.1: AVFrame Safety Wrapper**
```
Files:
  backend/crates/vclip-media/src/
    + avframe_view.rs      # AvFrameMatView, SharedAvFrame

Tests:
  + Lifetime safety test
  + Concurrent stress test
```

**PR 5.2: OpenVINO Preprocessing API (Optional Enhancement)**
```
Files:
  backend/crates/vclip-media/src/
    + openvino_preproc.rs  # OpenVINO PrePostProcessor wrapper
    ~ face_engine.rs       # Use OV preproc if available

Tests:
  + YUV420p direct input test
  + Benchmark vs sws_scale
```

### Phase 6: Integration (2 PRs)

**PR 6.1: FaceTimeline Schema + FaceInferenceEngine**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    + face_timeline.rs     # FaceTimeline, FaceDetection
    + face_engine.rs       # FaceInferenceEngine (orchestrator)

Tests:
  + Serialization round-trip
  + Schema version test
```

**PR 6.2: Worker Integration**
```
Files:
  backend/crates/vclip-media/src/intelligent/
    ~ detector.rs          # Use FaceInferenceEngine
    ~ mod.rs               # Re-export
  backend/crates/vclip-worker/src/
    ~ jobs/process.rs      # Use new pipeline

Tests:
  + End-to-end integration test
  + Regression test
```

### Phase 7: Benchmarks and Validation (1 PR)

**PR 7.1: Benchmarks + Acceptance Tests**
```
Files:
  backend/crates/vclip-media/benches/
    + face_inference.rs    # Criterion benchmarks
  backend/tests/
    + face_acceptance.rs   # Acceptance criteria tests

Artifacts:
  + BENCHMARK_REPORT.md
  + Allocation profile
  + Accuracy regression report
```

---

## 14. Input-Specific Optimizations (16:9 YouTube) (NEW v1.1)

### 14.1 Optimal Settings for YouTube Content

ViralClipAI primarily processes YouTube videos (16:9, 1080p/720p). Optimizations:

| Setting | Value | Rationale |
|---------|-------|-----------|
| Inference size | 960×540 | Minimal padding for 1080p (perfect 2x scale) |
| detect_every_n | 5 | 33ms@30fps, 166ms detection interval |
| Model | INT8-bq | Best perf/accuracy for YuNet |
| Padding value | 0 (black) | YuNet training expectation |

### 14.2 Resolution-Specific Scaling

```rust
impl MappingMeta {
    /// Optimized for common YouTube resolutions
    pub fn for_youtube(raw_w: u32, raw_h: u32) -> Self {
        // 16:9 resolutions: perfect 2x scale to 960×540
        let (inf_w, inf_h) = match (raw_w, raw_h) {
            (1920, 1080) => (960, 540),   // Perfect 2x
            (1280, 720) => (640, 360),    // Perfect 2x
            (3840, 2160) => (960, 540),   // 4x scale
            (2560, 1440) => (640, 360),   // 4x scale (better for small faces)
            _ => (960, 540),              // Default
        };

        Self::for_yunet(raw_w, raw_h, inf_w, inf_h)
    }
}
```

### 14.3 INT8 vs FP16 Decision Matrix

| CPU Feature | Recommended Model | Expected Latency |
|-------------|-------------------|------------------|
| AVX-512 VNNI | INT8-bq | <2ms |
| AVX-512 (no VNNI) | INT8 or FP16 | 2-4ms |
| AVX2 only | FP32 (or INT8) | 4-8ms |
| SSE3 baseline | FP32 | 8-15ms |

```rust
fn select_optimal_model(features: &CpuFeatures) -> &'static str {
    if features.avx512_vnni {
        "face_detection_yunet_2023mar_int8bq.onnx"  // Best for VNNI
    } else if features.avx512f {
        "face_detection_yunet_2023mar_int8.onnx"   // Good for AVX-512
    } else {
        "face_detection_yunet_2023mar.onnx"        // FP32 fallback
    }
}
```

---

## 15. Benchmark Plan

### 15.1 Test Matrix

| Input | Resolution | Inference Size | Model | Description |
|-------|------------|----------------|-------|-------------|
| 1080p | 1920×1080 | 640×360 | INT8-bq | Small inference |
| 1080p | 1920×1080 | 960×540 | INT8-bq | Standard |
| 4K | 3840×2160 | 640×360 | INT8-bq | High-res source |
| 4K | 3840×2160 | 960×540 | INT8-bq | High-res, large inf |

### 15.2 Metrics to Collect

| Metric | Method | Unit | v1.1 Target |
|--------|--------|------|-------------|
| `detect_keyframe` P50 | Criterion | ms | <5ms |
| `detect_keyframe` P95 | Criterion | ms | <8ms |
| `track_gap_frame` P50 | Criterion | ms | <0.05ms |
| INT8 inference (VNNI) | Criterion | ms | <2ms |
| Effective FPS @ N=5 | Calculated | fps | >360 |
| CPU utilization | /proc/stat | % | <80% |
| RSS peak | VmRSS | MB | <300 |
| Heap allocs/sec | heaptrack | count | <10 |
| Frame copies/sec | instrumentation | count | <50 |

### 15.3 Results Table Format (v1.1)

```markdown
## Benchmark Results (v1.1)

### Platform
- CPU: AMD EPYC 7282 16-Core (2 sockets)
- RAM: 256GB DDR4-3200
- OpenCV: 4.12.0 with OpenVINO 2024.4
- CPU Baseline: AVX2
- VNNI: Available (Ice Lake / Zen 4)

### detect_keyframe Latency (ms) - INT8-bq Model

| Input | Inf Size | P50 | P95 | P99 | vs v1.0 |
|-------|----------|-----|-----|-----|---------|
| 1080p | 640×360 | 1.8 | 2.4 | 3.1 | -57% |
| 1080p | 960×540 | 3.2 | 4.5 | 5.8 | -53% |
| 4K | 640×360 | 2.1 | 2.9 | 4.0 | -55% |
| 4K | 960×540 | 4.1 | 5.8 | 7.2 | -50% |

### Effective FPS

| Input | Inf Size | N=1 | N=3 | N=5 | N=10 | vs v1.0 |
|-------|----------|-----|-----|-----|------|---------|
| 1080p | 960×540 | 312 | 612 | 834 | 1024 | +71% |

### Resource Usage (8 concurrent workers, 1080p, N=5)

| Metric | v1.0 | v1.1 | Improvement |
|--------|------|------|-------------|
| CPU utilization | 85% | 68% | -20% |
| RSS per worker | 280 MB | 220 MB | -21% |
| Heap allocs/sec | 150 | <10 | -93% |
| Frame copies/sec | 180 | 60 | -67% |
```

---

## 16. Acceptance Criteria Verification

### 16.1 Inverse Mapping Correctness

```rust
#[test]
fn acceptance_inverse_mapping_exact() {
    // Reference semantics from spec
    let meta = MappingMeta::compute(1920, 1080, 960, 540);

    // Verify formulas: x_raw = (x_inf - P_left) / S
    assert_eq!(meta.scale, 0.5);
    assert_eq!(meta.pad_left, 0);
    assert_eq!(meta.pad_top, 0);

    // Point mapping
    let (x_raw, y_raw) = meta.map_point(480.0, 270.0);
    assert!((x_raw - 960.0).abs() < 0.01);
    assert!((y_raw - 540.0).abs() < 0.01);

    // Rect mapping
    let bbox_inf = BoundingBox::new(100.0, 100.0, 200.0, 200.0);
    let bbox_raw = meta.map_rect(&bbox_inf);

    assert!((bbox_raw.x - 200.0).abs() < 0.01);
    assert!((bbox_raw.y - 200.0).abs() < 0.01);
    assert!((bbox_raw.width - 400.0).abs() < 0.01);
}

#[test]
fn acceptance_zero_bar_output() {
    let meta = MappingMeta::compute(1280, 720, 960, 540);

    // 1280x720 → 960x540: scale = 0.75, pad_left = 120
    let bbox_inf = BoundingBox::new(120.0, 0.0, 720.0, 540.0);
    let bbox_raw = meta.map_rect(&bbox_inf);

    // No padding values in output
    assert!(bbox_raw.x >= 0.0);
    assert!(bbox_raw.x + bbox_raw.width <= 1280.0);
    assert!((bbox_raw.x - 0.0).abs() < 0.01);  // Maps to edge, not 120
}
```

### 16.2 Scene-Cut Tracker Reset (NEW v1.1)

```rust
#[test]
fn acceptance_scene_cut_clears_tracks() {
    let mut engine = FaceInferenceEngine::new(960, 540).unwrap();

    // Frame 1: Detect face (Person A)
    let frame_a = create_test_frame_with_face(1920, 1080, (500, 300));
    engine.process_frame(&frame_a, 0).unwrap();
    assert_eq!(engine.tracker.active_count(), 1);
    let track_id_a = engine.tracker.tracks().next().unwrap().0;

    // Frame 2: Scene cut to Person B
    let frame_b = create_completely_different_frame(1920, 1080);
    let detections = engine.process_frame(&frame_b, 33).unwrap();

    // Tracker should be reset, old track gone
    assert!(
        engine.tracker.track_by_id(track_id_a).is_none(),
        "Track from previous scene should be cleared"
    );

    // New detections should have new track IDs
    if !detections.is_empty() {
        assert_ne!(detections[0].track_id, track_id_a);
    }
}
```

### 16.3 Buffer Pooling Verification

```rust
#[test]
fn acceptance_zero_hot_loop_allocations() {
    let alloc_counter = AllocationCounter::new();

    let mut engine = FaceInferenceEngine::new(960, 540).unwrap();
    let frame = create_test_frame(1920, 1080);

    // Warm up
    for _ in 0..10 {
        engine.process_frame(&frame, 0).unwrap();
    }

    // Measure allocations in hot loop
    alloc_counter.reset();
    for i in 0..1000 {
        engine.process_frame(&frame, i * 33).unwrap();
    }

    let allocs = alloc_counter.count();
    assert_eq!(allocs, 0, "Expected zero allocations, got {}", allocs);
}
```

### 16.4 Padding Value Accuracy (NEW v1.1)

```rust
#[test]
fn acceptance_padding_value_affects_edge_accuracy() {
    let frame = create_test_frame_with_edge_face(1920, 1080);

    // Correct padding (0 for YuNet)
    let meta_correct = MappingMeta::for_yunet(1920, 1080, 960, 540);
    let detections_correct = detect_with_padding(&frame, meta_correct);

    // Wrong padding (128)
    let mut meta_wrong = meta_correct.clone();
    meta_wrong.padding_value = 128;
    let detections_wrong = detect_with_padding(&frame, meta_wrong);

    // Correct padding should have higher confidence on edge faces
    assert!(
        detections_correct.iter().map(|d| d.confidence).sum::<f64>()
            > detections_wrong.iter().map(|d| d.confidence).sum::<f64>(),
        "Correct padding should give higher confidence on edge faces"
    );
}
```

---

## 17. Validation Requirements (NEW v1.1)

### 17.1 Build Validation

**Required Artifacts:**
1. `cv2.getBuildInformation()` output confirming:
   - OpenVINO: YES
   - CPU_BASELINE: AVX2 (or AVX-512 for tuned)
   - TBB: YES

2. CI job that fails if above conditions not met

### 17.2 Startup Log Requirements

Every worker MUST log on startup:
```
INFO face_inference: Backend: OpenVINO, InputSize: 960x540, CPU: AVX512-VNNI
INFO face_inference: Model: yunet_int8bq, PaddingValue: 0, DetectEveryN: 5
INFO cpu_features: avx2=true avx512f=true avx512_vnni=true
```

```rust
impl FaceInferenceEngine {
    fn log_startup_config(&self) {
        let features = CpuFeatures::detect();

        tracing::info!(
            backend = ?self.backend,
            input_size = format!("{}x{}", self.inf_size.0, self.inf_size.1),
            cpu = if features.avx512_vnni {
                "AVX512-VNNI"
            } else if features.avx512f {
                "AVX512"
            } else if features.avx2 {
                "AVX2"
            } else {
                "SSE3"
            },
            "Backend: {:?}, InputSize: {}x{}, CPU: {}",
            self.backend,
            self.inf_size.0,
            self.inf_size.1,
            // ...
        );
    }
}
```

### 17.3 Accuracy Regression Tests

| Test | Metric | Threshold |
|------|--------|-----------|
| 16:9 letterbox vs stretch | mAP | >0.90 both |
| Scene-cut ghost prevention | False positives | 0 |
| INT8 vs FP32 accuracy | mAP delta | <0.02 |

### 17.4 Performance Regression Tests

| Test | Metric | v1.1 Requirement |
|------|--------|------------------|
| Keyframe latency (1080p) | P95 | <5ms |
| Keyframe latency (VNNI) | P95 | <2ms |
| Effective FPS (N=5) | Throughput | >360 fps |
| Decode capacity | Max fps | >200 fps |
| Hot loop allocations | Count | 0 |

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2024-12 | Architect | Initial comprehensive plan |
| 1.1 | 2024-12 | Architect | Expert review integration: OpenVINO PreProc, INT8/VNNI, scene-cut reset, padding_value, reordered PRs, validation requirements |

---

## Appendix A: CMake Flags Reference

### Full OpenCV Build Command (Portable + OpenVINO)

```bash
cmake -G Ninja ../opencv \
  -D CMAKE_BUILD_TYPE=Release \
  -D CMAKE_INSTALL_PREFIX=/usr/local \
  -D OPENCV_EXTRA_MODULES_PATH=../opencv_contrib/modules \
  \
  # OpenVINO (REQUIRED)
  -D WITH_OPENVINO=ON \
  -D OpenVINO_DIR=/opt/intel/openvino/runtime/cmake \
  \
  # CPU ISA (Portable)
  -D CPU_BASELINE=AVX2 \
  -D CPU_DISPATCH= \
  -D ENABLE_AVX2=ON \
  -D ENABLE_AVX512=OFF \
  \
  # Parallelism
  -D WITH_TBB=ON \
  -D WITH_OPENMP=OFF \
  -D WITH_PTHREADS_PF=ON \
  \
  # IPP
  -D WITH_IPP=ON \
  -D BUILD_IPP_IW=ON \
  \
  # FFmpeg (for VideoCapture)
  -D WITH_FFMPEG=ON \
  \
  # Image codecs
  -D WITH_PNG=ON \
  -D WITH_TIFF=ON \
  -D WITH_WEBP=ON \
  \
  # DNN
  -D BUILD_opencv_dnn=ON \
  \
  # Face detection
  -D BUILD_opencv_objdetect=ON \
  -D BUILD_opencv_face=ON \
  \
  # Disable unneeded
  -D BUILD_opencv_python2=OFF \
  -D BUILD_opencv_python3=OFF \
  -D BUILD_TESTS=OFF \
  -D BUILD_PERF_TESTS=OFF \
  -D BUILD_EXAMPLES=OFF \
  -D BUILD_DOCS=OFF \
  -D WITH_GTK=OFF \
  -D WITH_QT=OFF \
  -D WITH_CUDA=OFF
```

---

## Appendix B: Runtime CPU Guard Implementation

```rust
//! CPU feature verification for tuned builds.

#[derive(Debug, Clone)]
pub struct CpuFeatures {
    pub avx2: bool,
    pub avx512f: bool,
    pub avx512bw: bool,
    pub avx512vl: bool,
    pub avx512_vnni: bool,  // [NEW v1.1]
}

impl CpuFeatures {
    pub fn detect() -> Self {
        Self {
            avx2: is_x86_feature_detected!("avx2"),
            avx512f: is_x86_feature_detected!("avx512f"),
            avx512bw: is_x86_feature_detected!("avx512bw"),
            avx512vl: is_x86_feature_detected!("avx512vl"),
            avx512_vnni: is_x86_feature_detected!("avx512vnni"),
        }
    }

    /// Verify CPU meets requirements for tuned build.
    pub fn verify_tuned_requirements() -> Result<(), CpuMismatchError> {
        let features = Self::detect();

        if !features.avx512f {
            return Err(CpuMismatchError::MissingFeature("avx512f"));
        }
        if !features.avx512bw {
            return Err(CpuMismatchError::MissingFeature("avx512bw"));
        }

        // [NEW v1.1] Warn if VNNI not available for optimal INT8
        if !features.avx512_vnni {
            tracing::warn!(
                "AVX-512 VNNI not available. INT8 inference will be suboptimal. \
                 Consider using FP32 model or upgrading to VNNI-capable CPU."
            );
        }

        Ok(())
    }

    /// Log CPU capabilities for diagnostics
    pub fn log_capabilities(&self) {
        tracing::info!(
            avx2 = self.avx2,
            avx512f = self.avx512f,
            avx512_vnni = self.avx512_vnni,
            "CPU feature detection complete"
        );
    }
}

#[derive(Debug)]
pub struct CpuMismatchError {
    pub missing_feature: &'static str,
}

// Usage in main():
fn main() {
    // Log CPU features on startup
    CpuFeatures::detect().log_capabilities();

    #[cfg(feature = "tuned-build")]
    {
        if let Err(e) = CpuFeatures::verify_tuned_requirements() {
            eprintln!(
                "FATAL: This binary requires CPU feature '{}' which is not available.",
                e.missing_feature
            );
            eprintln!("Use the 'portable' image for this CPU.");
            std::process::exit(1);
        }
    }
}
```

---

## Appendix C: OpenVINO Preprocessing API Reference

```cpp
// Example: YUV420p direct input with OpenVINO preprocessing
#include <openvino/openvino.hpp>

ov::Core core;
auto model = core.read_model("yunet_int8bq.onnx");

// Configure preprocessing for YUV420p input
ov::preprocess::PrePostProcessor ppp(model);

auto& input = ppp.input();
input.tensor()
    .set_element_type(ov::element::u8)
    .set_color_format(ov::preprocess::ColorFormat::I420)
    .set_spatial_static_shape(1080, 1920);

input.preprocess()
    .convert_color(ov::preprocess::ColorFormat::BGR)
    .resize(ov::preprocess::ResizeAlgorithm::RESIZE_LINEAR, 540, 960)
    .mean({0.0f, 0.0f, 0.0f})
    .scale({1.0f, 1.0f, 1.0f});

auto optimized_model = ppp.build();
auto compiled_model = core.compile_model(optimized_model, "CPU");

// Inference with YUV420p input
auto infer_request = compiled_model.create_infer_request();

// Set Y plane
infer_request.set_tensor("input_y", ov::Tensor(
    ov::element::u8,
    {1, 1080, 1920, 1},
    y_plane_ptr
));

// Set UV planes (for I420)
// ... similar for U and V planes

infer_request.infer();
```
