//! OpenCV YuNet face detector (2023mar model with quantization support).
//!
//! YuNet is a lightweight CNN face detector that is significantly faster and more accurate
//! than Haar cascades. It's exposed via OpenCV's FaceDetectorYN API.
//!
//! # Performance Comparison (2023mar models)
//! - Original: 0.8844 AP_easy, ~25ms/frame
//! - Int8 Quantized: 0.8810 AP_easy, ~8ms/frame
//! - Block-Quantized: 0.8845 AP_easy, ~6ms/frame
//!
//! # Requirements
//! - OpenCV 4.5+ with DNN module
/// Global YuNet availability flag
static YUNET_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Get model performance info for logging
fn get_model_info(path: &str) -> (&str, &str, &str) {
    if path.contains("int8bq") {
        ("2023mar Block-Quantized", "~6ms/frame", "0.8845 AP")
    } else if path.contains("int8") {
        ("2023mar Int8-Quantized", "~8ms/frame", "0.8810 AP")
    } else if path.contains("2023mar") {
        ("2023mar Original", "~25ms/frame", "0.8844 AP")
    } else {
        ("2022mar Fallback", "~30ms/frame", "0.834 AP")
    }
}

/// Download YuNet model if not present (requires internet access)
#[cfg(feature = "opencv")]
pub async fn ensure_yunet_models() -> MediaResult<()> {
    use tokio::process::Command;

    // Check if any model is already available
    if find_model_path().is_some() {
        return Ok(());
    }

    info!("YuNet models not found, attempting automatic download...");

    // Create models directory
    let model_dir = Path::new("/app/models");
    if !model_dir.exists() {
        tokio::fs::create_dir_all(model_dir).await
            .map_err(|e| MediaError::detection_failed(format!("Failed to create model directory: {}", e)))?;
    }

    // Try to download the block-quantized model first (fastest)
    let model_path = model_dir.join("face_detection_yunet_2023mar_int8bq.onnx");
    let model_url = "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx";

    info!("Downloading YuNet model from GitHub...");
    let status = Command::new("curl")
        .args(["-L", "-o", &model_path.to_string_lossy(), model_url])
        .status()
        .await
        .map_err(|e| MediaError::detection_failed(format!("Failed to download YuNet model: {}", e)))?;

    if status.success() && model_path.exists() {
        info!("✅ YuNet model downloaded successfully: {}", model_path.display());
        Ok(())
    } else {
        Err(MediaError::detection_failed(
            "Failed to download YuNet model. Please run download-yunet-models.sh or download manually."
        ))
    }
}

/// Check if YuNet is available and log which model was found
pub fn is_yunet_available() -> bool {
    *YUNET_AVAILABLE.get_or_init(|| {
        if let Some(path) = find_model_path() {
            let (version, speed, accuracy) = get_model_info(path);
            info!(
                "YuNet face detection model found: {} ({}, {}) at {}",
                version, speed, accuracy, path
            );
            true
        } else {
            warn!("YuNet model not found - using FFmpeg heuristic detection");
            warn!("To enable YuNet, download models to /app/models/:");
            warn!("  curl -L -o /app/models/face_detection_yunet_2023mar_int8bq.onnx \\");
            warn!("    https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx");
            false
        }
    })
}

/// Check if YuNet is available, with automatic download attempt
pub async fn ensure_yunet_available() -> bool {
    if is_yunet_available() {
        return true;
    }

    // Try to download models automatically
    #[cfg(feature = "opencv")]
    {
        if let Ok(()) = ensure_yunet_models().await {
            // Clear the cached availability and check again
            // Since OnceLock doesn't allow clearing, we'll just return the download success
            return true;
        }
    }

    false
}

//! # Model Download
//! ```bash
//! # Download the fastest block-quantized model (recommended)
//! curl -L -o /app/models/face_detection_yunet_2023mar_int8bq.onnx \
//!   https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx
//!
//! # Or download all variants for fallback
//! curl -L -o /app/models/face_detection_yunet_2023mar.onnx \
//!   https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx
//! curl -L -o /app/models/face_detection_yunet_2023mar_int8.onnx \
//!   https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8.onnx
//! ```

use super::models::BoundingBox;
use crate::error::{MediaError, MediaResult};
use std::path::Path;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// Model paths in preference order (fastest/most accurate first)
/// Priority: block-quantized (fastest) > int8 quantized > original
const YUNET_MODEL_PATHS: &[&str] = &[
    // 2023mar models (recommended for accuracy/performance)
    "/app/models/face_detection_yunet_2023mar_int8bq.onnx",    // Block-quantized: ~6ms, 0.8845 AP
    "/app/models/face_detection_yunet_2023mar_int8.onnx",      // Int8 quantized: ~8ms, 0.8810 AP
    "/app/models/face_detection_yunet_2023mar.onnx",           // Original: ~25ms, 0.8844 AP

    // Fallback to 2022mar models if 2023 not available
    "/app/models/face_detection_yunet_2022mar.onnx",
    "./models/face_detection_yunet_2023mar_int8bq.onnx",
    "./models/face_detection_yunet_2023mar_int8.onnx",
    "./models/face_detection_yunet_2023mar.onnx",
    "./models/face_detection_yunet_2022mar.onnx",

    // System paths (last resort)
    "/usr/share/opencv/models/face_detection_yunet_2023mar_int8bq.onnx",
    "/usr/share/opencv/models/face_detection_yunet_2023mar_int8.onnx",
    "/usr/share/opencv/models/face_detection_yunet_2023mar.onnx",
    "/usr/share/opencv/models/face_detection_yunet_2022mar.onnx",
];

/// Score threshold for face detection
const SCORE_THRESHOLD: f32 = 0.7;

/// NMS threshold for face detection
const NMS_THRESHOLD: f32 = 0.3;

/// Top K faces to keep
const TOP_K: i32 = 10;

/// Find the YuNet model file (prefers fastest available model)
fn find_model_path() -> Option<&'static str> {
    for path in YUNET_MODEL_PATHS {
        if Path::new(path).exists() {
            return Some(path);
        }
    }
    None
}

/// YuNet face detector using OpenCV.
#[cfg(feature = "opencv")]
pub struct YuNetDetector {
    /// OpenCV FaceDetectorYN instance
    detector: opencv::objdetect::FaceDetectorYN,
    /// Input size for the detector
    input_size: (i32, i32),
}

#[cfg(feature = "opencv")]
impl YuNetDetector {
    /// Create a new YuNet detector.
    pub fn new(frame_width: u32, frame_height: u32) -> MediaResult<Self> {
        use opencv::objdetect::FaceDetectorYN;

        let model_path = find_model_path().ok_or_else(|| {
            MediaError::detection_failed("YuNet model not found")
        })?;

        // Create detector with input size matching frame dimensions
        // Scale down for faster processing
        let scale = (frame_width as f64 / 640.0).max(frame_height as f64 / 480.0).max(1.0);
        let input_width = (frame_width as f64 / scale).round() as i32;
        let input_height = (frame_height as f64 / scale).round() as i32;

        let detector = FaceDetectorYN::create(
            model_path,
            "",
            opencv::core::Size::new(input_width, input_height),
            SCORE_THRESHOLD,
            NMS_THRESHOLD,
            TOP_K,
            0, // Backend ID (default)
            0, // Target ID (default)
        ).map_err(|e| MediaError::detection_failed(format!("Failed to create YuNet detector: {}", e)))?;

        info!(
            "YuNet detector initialized: input_size={}x{}, model={}",
            input_width, input_height, model_path
        );

        Ok(Self {
            detector,
            input_size: (input_width, input_height),
        })
    }

    /// Detect faces in a frame.
    ///
    /// Returns bounding boxes in normalized coordinates (0.0-1.0).
    pub fn detect_in_frame(&mut self, frame: &opencv::core::Mat) -> MediaResult<Vec<(BoundingBox, f64)>> {
        use opencv::core::{Mat, Size};
        use opencv::imgproc;

        // Resize frame to input size
        let mut resized = Mat::default();
        imgproc::resize(
            frame,
            &mut resized,
            Size::new(self.input_size.0, self.input_size.1),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        ).map_err(|e| MediaError::detection_failed(format!("Failed to resize frame: {}", e)))?;

        // Run detection
        let mut faces = Mat::default();
        self.detector.detect(&resized, &mut faces)
            .map_err(|e| MediaError::detection_failed(format!("YuNet detection failed: {}", e)))?;

        let num_faces = faces.rows();
        if num_faces == 0 {
            return Ok(Vec::new());
        }

        let frame_width = frame.cols() as f64;
        let frame_height = frame.rows() as f64;
        let scale_x = frame_width / self.input_size.0 as f64;
        let scale_y = frame_height / self.input_size.1 as f64;

        let mut results = Vec::with_capacity(num_faces as usize);

        // YuNet output format: x, y, w, h, x_re, y_re, x_le, y_le, x_n, y_n, x_ml, y_ml, x_mr, y_mr, score
        // We only need x, y, w, h, score
        for i in 0..num_faces {
            let x = *faces.at_2d::<f32>(i, 0).unwrap_or(&0.0) as f64 * scale_x;
            let y = *faces.at_2d::<f32>(i, 1).unwrap_or(&0.0) as f64 * scale_y;
            let w = *faces.at_2d::<f32>(i, 2).unwrap_or(&0.0) as f64 * scale_x;
            let h = *faces.at_2d::<f32>(i, 3).unwrap_or(&0.0) as f64 * scale_y;
            let score = *faces.at_2d::<f32>(i, 14).unwrap_or(&0.0) as f64;

            if w > 0.0 && h > 0.0 && score > SCORE_THRESHOLD as f64 {
                let bbox = BoundingBox::new(x, y, w, h);
                results.push((bbox, score));
            }
        }

        debug!("YuNet detected {} faces", results.len());
        Ok(results)
    }
}

/// Extract frames from video and detect faces using YuNet.
#[cfg(feature = "opencv")]
pub async fn detect_faces_with_yunet<P: AsRef<Path>>(
    video_path: P,
    start_time: f64,
    end_time: f64,
    frame_width: u32,
    frame_height: u32,
    sample_fps: f64,
) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
    use opencv::videoio::{VideoCapture, CAP_PROP_POS_MSEC, CAP_PROP_FRAME_WIDTH, CAP_PROP_FRAME_HEIGHT};
    use opencv::core::Mat;

    let video_path = video_path.as_ref();
    let duration = end_time - start_time;
    let sample_interval = 1.0 / sample_fps;
    let num_samples = (duration / sample_interval).ceil() as usize;

    // Open video with OpenCV
    let mut cap = VideoCapture::from_file(
        video_path.to_str().unwrap_or(""),
        opencv::videoio::CAP_ANY,
    ).map_err(|e| MediaError::detection_failed(format!("Failed to open video: {}", e)))?;

    if !cap.is_opened().unwrap_or(false) {
        return Err(MediaError::detection_failed("Failed to open video file"));
    }

    // Get actual video dimensions
    let actual_width = cap.get(CAP_PROP_FRAME_WIDTH).unwrap_or(frame_width as f64) as u32;
    let actual_height = cap.get(CAP_PROP_FRAME_HEIGHT).unwrap_or(frame_height as f64) as u32;

    // Create detector
    let mut detector = YuNetDetector::new(actual_width, actual_height)?;

    let mut all_detections = Vec::with_capacity(num_samples);
    let mut current_time = start_time;

    info!("Detecting faces with YuNet: {} frames at {:.1} fps", num_samples, sample_fps);

    for _ in 0..num_samples {
        // Seek to current time
        cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0)
            .map_err(|e| MediaError::detection_failed(format!("Failed to seek: {}", e)))?;

        // Read frame
        let mut frame = Mat::default();
        let success = cap.read(&mut frame)
            .map_err(|e| MediaError::detection_failed(format!("Failed to read frame: {}", e)))?;

        if !success || frame.empty() {
            // End of video or seek past end
            all_detections.push(Vec::new());
        } else {
            // Detect faces
            let detections = detector.detect_in_frame(&frame)?;
            all_detections.push(detections);
        }

        current_time += sample_interval;
    }

    info!("YuNet detection complete: {} detections across {} frames",
        all_detections.iter().map(|d| d.len()).sum::<usize>(),
        all_detections.len()
    );

    Ok(all_detections)
}

/// Stub for when OpenCV is not available
#[cfg(not(feature = "opencv"))]
pub struct YuNetDetector;

#[cfg(not(feature = "opencv"))]
impl YuNetDetector {
    pub fn new(_frame_width: u32, _frame_height: u32) -> MediaResult<Self> {
        Err(MediaError::detection_failed("OpenCV feature not enabled"))
    }
}

#[cfg(not(feature = "opencv"))]
pub async fn detect_faces_with_yunet<P: AsRef<Path>>(
    _video_path: P,
    _start_time: f64,
    _end_time: f64,
    _frame_width: u32,
    _frame_height: u32,
    _sample_fps: f64,
) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
    Err(MediaError::detection_failed("OpenCV feature not enabled"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_paths() {
        // Just verify the paths are defined
        assert!(!YUNET_MODEL_PATHS.is_empty());
    }
}
