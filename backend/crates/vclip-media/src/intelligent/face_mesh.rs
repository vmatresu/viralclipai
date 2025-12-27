//! MediaPipe Face Mesh ONNX inference for mouth activity and dense landmarks.
//!
//! This module provides a trait-based wrapper so the SpeakerAware tiers can
//! optionally refine YuNet detections with dense landmarks and a robust
//! mouth-openness signal without touching the lower tiers.
//!
//! Notes:
//! - OpenCV delivers frames as BGR; we convert to RGB before normalization.
//! - Coordinates are mapped back to frame space using a center-based transform
//!   to avoid drift when the ROI is clamped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use opencv::core::{Mat, Rect, Vector};
use opencv::imgproc;
use opencv::prelude::{MatTraitConst, MatTraitConstManual};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{Tensor, Value};

use crate::error::{MediaError, MediaResult};

/// Single face landmark in frame coordinates.
#[derive(Debug, Clone, Copy)]
pub struct FaceLandmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Face mesh inference result.
#[derive(Debug, Clone)]
pub struct FaceMeshResult {
    pub landmarks: Vec<FaceLandmark>,
    pub mouth_openness: f32,
    /// Expanded square crop used for inference (frame coordinates).
    pub crop_rect: Rect,
}

/// Simple debug toggle via env var.
fn debug_enabled() -> bool {
    std::env::var("DEBUG_RENDER_FACE_ACTIVITY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

static DEBUG_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Trait for detailed face analysis on top of YuNet detections.
pub trait FaceDetailAnalyzer: Send + Sync {
    fn analyze(&self, frame_bgr: &Mat, roi: &Rect) -> MediaResult<FaceMeshResult>;
}

/// ONNX Runtime-backed face mesh analyzer.
pub struct OrtFaceMeshAnalyzer {
    detector: Arc<FaceMeshDetector>,
}

impl OrtFaceMeshAnalyzer {
    /// Load analyzer with default model search paths.
    pub fn new_default() -> MediaResult<Self> {
        let model_path = find_default_model_path().ok_or_else(|| {
            MediaError::detection_failed(
                "face_landmark_with_attention.onnx not found; place it under backend/models/face_mesh/",
            )
        })?;
        let detector = FaceMeshDetector::load(&model_path)?;
        Ok(Self {
            detector: Arc::new(detector),
        })
    }

    pub fn new_with_model(model_path: &Path) -> MediaResult<Self> {
        let detector = FaceMeshDetector::load(model_path)?;
        Ok(Self {
            detector: Arc::new(detector),
        })
    }
}

impl FaceDetailAnalyzer for OrtFaceMeshAnalyzer {
    fn analyze(&self, frame_bgr: &Mat, roi: &Rect) -> MediaResult<FaceMeshResult> {
        self.detector.detect(frame_bgr, roi)
    }
}

/// ONNX Runtime wrapper for the MediaPipe Face Mesh model.
pub struct FaceMeshDetector {
    session: Mutex<Session>,
}

impl FaceMeshDetector {
    pub fn load(model_path: &Path) -> MediaResult<Self> {
        if !model_path.exists() {
            return Err(MediaError::detection_failed(format!(
                "Face mesh model not found at {}",
                model_path.display()
            )));
        }

        let model_bytes = std::fs::read(model_path)
            .map_err(|e| MediaError::detection_failed(format!("ORT read model file: {e}")))?;

        let session = Session::builder()
            .map_err(|e| MediaError::detection_failed(format!("ORT session builder: {e}")))?
            // Prefer optimized graph on CPU; CUDA feature can be enabled externally
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| MediaError::detection_failed(format!("ORT opt level: {e}")))?
            .commit_from_memory(model_bytes.as_slice())
            .map_err(|e| MediaError::detection_failed(format!("ORT load model: {e}")))?;

        Ok(Self {
            session: Mutex::new(session),
        })
    }

    /// Run face mesh on a YuNet ROI inside the full frame (BGR Mat).
    pub fn detect(&self, frame_bgr: &Mat, roi: &Rect) -> MediaResult<FaceMeshResult> {
        // 1) Expand ROI by 25%, make square, clamp to frame.
        let crop_rect = make_square_crop(frame_bgr, roi, 0.25)?;

        // 2) Extract and convert BGR -> RGB.
        let crop_rgb = extract_rgb_crop(frame_bgr, &crop_rect)?;

        // 3) Resize to 192x192.
        let mut resized = Mat::default();
        imgproc::resize(
            &crop_rgb,
            &mut resized,
            opencv::core::Size::new(192, 192),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        )
        .map_err(|e| MediaError::detection_failed(format!("Resize failed: {e}")))?;

        // 4) Normalize to [-1,1] CHW tensor.
        let tensor = mat_to_chw_tensor(&resized)?;

        // 5) Run inference.
        let mut session = self
            .session
            .lock()
            .map_err(|_| MediaError::detection_failed("ORT session poisoned"))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| MediaError::detection_failed(format!("ORT run failed: {e}")))?;

        let output = outputs
            .get("output")
            .ok_or_else(|| MediaError::detection_failed("ORT returned no outputs"))?;

        // Expect (1, 468, 3) or (468,3); handle both.
        let landmarks = extract_landmarks(output, &crop_rect)?;

        // Compute mouth openness via MAR.
        let mouth_openness = calculate_mouth_openness(&landmarks);

        let result = FaceMeshResult {
            landmarks,
            mouth_openness,
            crop_rect,
        };

        maybe_debug_render(frame_bgr, &result);

        Ok(result)
    }
}

/// Compute mouth openness using inner-lip landmarks (indices 13, 14, 78, 308).
pub fn calculate_mouth_openness(landmarks: &[FaceLandmark]) -> f32 {
    const TOP: usize = 13;
    const BOT: usize = 14;
    const LEFT: usize = 78;
    const RIGHT: usize = 308;
    const EPS: f32 = 1e-6;

    let valid = |idx: usize| idx < landmarks.len();
    if !(valid(TOP) && valid(BOT) && valid(LEFT) && valid(RIGHT)) {
        return 0.0;
    }

    let p_top = landmarks[TOP];
    let p_bot = landmarks[BOT];
    let p_left = landmarks[LEFT];
    let p_right = landmarks[RIGHT];

    let v = ((p_top.x - p_bot.x).powi(2) + (p_top.y - p_bot.y).powi(2)).sqrt();
    let h = ((p_left.x - p_right.x).powi(2) + (p_left.y - p_right.y).powi(2)).sqrt();
    if h <= EPS {
        return 0.0;
    }
    v / h
}

/// Expand ROI, square it, and clamp.
fn make_square_crop(frame: &Mat, roi: &Rect, pad_ratio: f32) -> MediaResult<Rect> {
    let w = roi.width as f32;
    let h = roi.height as f32;
    let size = w.max(h) * (1.0 + pad_ratio);

    let center_x = roi.x as f32 + w / 2.0;
    let center_y = roi.y as f32 + h / 2.0;

    let mut x = center_x - size / 2.0;
    let mut y = center_y - size / 2.0;
    let mut s = size;

    let frame_w = frame.cols() as f32;
    let frame_h = frame.rows() as f32;

    if x < 0.0 {
        s += x;
        x = 0.0;
    }
    if y < 0.0 {
        s += y;
        y = 0.0;
    }
    if x + s > frame_w {
        s = frame_w - x;
    }
    if y + s > frame_h {
        s = frame_h - y;
    }

    if s < 8.0 {
        return Err(MediaError::detection_failed("ROI too small for face mesh"));
    }

    Ok(Rect::new(
        x.round() as i32,
        y.round() as i32,
        s.round() as i32,
        s.round() as i32,
    ))
}

/// Extract RGB crop from BGR frame.
fn extract_rgb_crop(frame_bgr: &Mat, crop: &Rect) -> MediaResult<Mat> {
    let roi = Mat::roi(frame_bgr, *crop)
        .map_err(|e| MediaError::detection_failed(format!("ROI failed: {e}")))?;
    let mut rgb = Mat::default();
    imgproc::cvt_color(
        &roi,
        &mut rgb,
        imgproc::COLOR_BGR2RGB,
        0,
        opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
    )
    .map_err(|e| MediaError::detection_failed(format!("BGR2RGB failed: {e}")))?;
    Ok(rgb)
}

/// Convert Mat (RGB, HxWx3) to ORT tensor (1,3,192,192) normalized to [-1,1].
fn mat_to_chw_tensor(mat_rgb: &Mat) -> MediaResult<Value> {
    let size = mat_rgb
        .size()
        .map_err(|e| MediaError::detection_failed(format!("Mat size: {e}")))?;
    let (h, w) = (size.height, size.width);
    let channels = mat_rgb.channels();
    if channels != 3 {
        return Err(MediaError::detection_failed("Expected 3-channel RGB Mat"));
    }

    let data = mat_rgb
        .data_typed::<u8>()
        .map_err(|e| MediaError::detection_failed(format!("Mat data: {e}")))?;

    let mut chw = Vec::with_capacity((h * w * 3) as usize);
    // HWC -> CHW
    for c in 0..3 {
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w * 3 + x * 3 + c) as usize;
                let v = data[idx] as f32 / 255.0;
                chw.push(v * 2.0 - 1.0);
            }
        }
    }

    let shape = vec![1usize, 3, h as usize, w as usize];
    let boxed = chw.into_boxed_slice();
    Tensor::from_array((shape, boxed))
        .map(Value::from)
        .map_err(|e| MediaError::detection_failed(format!("ORT tensor: {e}")))
}

/// Extract landmarks and map back to frame coordinates using center-based mapping.
fn extract_landmarks(output: &Value, crop: &Rect) -> MediaResult<Vec<FaceLandmark>> {
    let (shape, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| MediaError::detection_failed(format!("ORT extract: {e}")))?;

    // Accept [1,468,3] or [468,3]
    let (points, dim3) = match shape.len() {
        3 if shape[0] == 1 => (shape[1] as usize, shape[2] as usize),
        2 => (shape[0] as usize, shape[1] as usize),
        _ => {
            return Err(MediaError::detection_failed(format!(
                "Unexpected face mesh output shape: {:?}",
                shape
            )))
        }
    };

    if dim3 < 3 || data.len() < points * dim3 {
        return Err(MediaError::detection_failed(
            "Face mesh output missing Z channel",
        ));
    }

    let mut landmarks = Vec::with_capacity(points as usize);
    for i in 0..points {
        let base = i * dim3;
        let nx = data[base];
        let ny = data[base + 1];
        let nz = data[base + 2];

        let (x, y) = map_normalized_to_frame(nx, ny, crop);
        landmarks.push(FaceLandmark { x, y, z: nz });
    }

    Ok(landmarks)
}

/// Center-based mapping from normalized crop coords to frame coords.
#[inline]
pub fn map_normalized_to_frame(nx: f32, ny: f32, crop: &Rect) -> (f32, f32) {
    let center_x = crop.x as f32 + crop.width as f32 / 2.0;
    let center_y = crop.y as f32 + crop.height as f32 / 2.0;
    let box_w = crop.width as f32;
    (center_x + (nx - 0.5) * box_w, center_y + (ny - 0.5) * box_w)
}

/// When enabled, draw debug overlays and dump frames to /tmp/face_mesh_debug/.
fn maybe_debug_render(frame_bgr: &Mat, result: &FaceMeshResult) {
    if !debug_enabled() {
        return;
    }

    let mut frame = frame_bgr.clone();

    // Draw crop rect (blue)
    let color_blue = opencv::core::Scalar::new(255.0, 0.0, 0.0, 0.0);
    let color_green = opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0);
    let color_white = opencv::core::Scalar::new(255.0, 255.0, 255.0, 0.0);

    let _ = opencv::imgproc::rectangle(
        &mut frame,
        result.crop_rect,
        color_blue,
        2,
        opencv::imgproc::LINE_8,
        0,
    );

    // Draw lip landmarks (13,14,78,308)
    let idxs = [13, 14, 78, 308];
    for &i in &idxs {
        if let Some(lm) = result.landmarks.get(i) {
            let _ = opencv::imgproc::circle(
                &mut frame,
                opencv::core::Point::new(lm.x.round() as i32, lm.y.round() as i32),
                3,
                color_green,
                opencv::imgproc::FILLED,
                opencv::imgproc::LINE_8,
                0,
            );
        }
    }

    // Put mouth openness text
    let text = format!("mouth:{:.3}", result.mouth_openness);
    let _ = opencv::imgproc::put_text(
        &mut frame,
        &text,
        opencv::core::Point::new(result.crop_rect.x, (result.crop_rect.y - 8).max(0)),
        opencv::imgproc::FONT_HERSHEY_SIMPLEX,
        0.5,
        color_white,
        1,
        opencv::imgproc::LINE_AA,
        false,
    );

    // Write to /tmp/face_mesh_debug/ (disabled if imgcodecs feature not present)
    #[cfg(feature = "opencv")]
    {
        use opencv::imgcodecs;
        if let Ok(_) = std::fs::create_dir_all("/tmp/face_mesh_debug") {
            let idx = DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = format!("/tmp/face_mesh_debug/frame_{idx:06}.jpg");
            let _ = imgcodecs::imwrite(&path, &frame, &Vector::<i32>::new());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouth_openness_open_gt_closed() {
        let mut closed = vec![
            FaceLandmark {
                x: 0.0,
                y: 0.0,
                z: 0.0
            };
            309
        ];
        closed[13].y = 100.0;
        closed[14].y = 100.0;
        closed[78].x = 90.0;
        closed[308].x = 110.0;

        let mut open = closed.clone();
        open[13].y = 90.0;
        open[14].y = 110.0;

        let closed_score = calculate_mouth_openness(&closed);
        let open_score = calculate_mouth_openness(&open);
        assert!(open_score > closed_score, "open should be greater");
    }

    #[test]
    fn test_center_mapping_is_correct() {
        let crop = opencv::core::Rect::new(10, 20, 100, 100);
        let (cx, cy) = map_normalized_to_frame(0.5, 0.5, &crop);
        assert!((cx - 60.0).abs() < 1e-3 && (cy - 70.0).abs() < 1e-3);

        let (x0, y0) = map_normalized_to_frame(0.0, 0.0, &crop);
        assert!((x0 - 10.0).abs() < 1e-3 && (y0 - 20.0).abs() < 1e-3);
    }
}

/// Search common locations for the face mesh model.
fn find_default_model_path() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "./backend/models/face_mesh/face_landmark_with_attention.onnx",
        "/app/backend/models/face_mesh/face_landmark_with_attention.onnx",
        "/app/models/face_mesh/face_landmark_with_attention.onnx",
    ];

    for p in CANDIDATES {
        let path = Path::new(p);
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }
    None
}
