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
//!
//! # Known Issues
//! - OpenCV 4.6.0 has a bug with FaceDetectorYN where detection can fail with
//!   "Layer with requested id=-1 not found". This is handled gracefully with fallback.

use super::models::BoundingBox;
use crate::error::{MediaError, MediaResult};
#[cfg(feature = "opencv")]
use opencv::objdetect::FaceDetectorYN;
#[cfg(feature = "opencv")]
use opencv::prelude::FaceDetectorYNTrait;
use std::path::Path;
use std::sync::OnceLock;
use tracing::{debug, error, info, warn};

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

// # Model Download
// ```bash
// # Download and verify all models (recommended)
// ./download-yunet-models.sh
//
// # Or manually download to backend/models/face_detection/yunet/
// mkdir -p backend/models/face_detection/yunet
// curl -L -o backend/models/face_detection/yunet/face_detection_yunet_2023mar.onnx \
//   "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx"
// ```

/// Model paths in preference order
/// Priority: backend/models (committed) > container models > system paths
/// This ensures reproducible builds and offline development
///
/// IMPORTANT: 2023mar models require OpenCV 4.8+
/// The 2022mar model is compatible with OpenCV 4.5+ and is used as a fallback
pub(crate) const YUNET_MODEL_PATHS_2023: &[&str] = &[
    // Backend models directory (committed to version control)
    "/app/backend/models/face_detection/yunet/face_detection_yunet_2023mar.onnx",
    "/app/backend/models/face_detection/yunet/face_detection_yunet_2023mar_int8.onnx",
    "/app/backend/models/face_detection/yunet/face_detection_yunet_2023mar_int8bq.onnx",
    // Container models directory
    "/app/models/face_detection/yunet/face_detection_yunet_2023mar.onnx",
    "/app/models/face_detection/yunet/face_detection_yunet_2023mar_int8.onnx",
    "/app/models/face_detection/yunet/face_detection_yunet_2023mar_int8bq.onnx",
    // Legacy paths
    "/app/models/face_detection_yunet_2023mar.onnx",
    "/app/models/face_detection_yunet_2023mar_int8.onnx",
    "/app/models/face_detection_yunet_2023mar_int8bq.onnx",
    // Relative paths for development
    "./backend/models/face_detection/yunet/face_detection_yunet_2023mar.onnx",
    "./backend/models/face_detection/yunet/face_detection_yunet_2023mar_int8.onnx",
    "./backend/models/face_detection/yunet/face_detection_yunet_2023mar_int8bq.onnx",
    // System paths
    "/usr/share/opencv/models/face_detection_yunet_2023mar.onnx",
    "/usr/share/opencv/models/face_detection_yunet_2023mar_int8.onnx",
    "/usr/share/opencv/models/face_detection_yunet_2023mar_int8bq.onnx",
];

/// 2022mar model paths (compatible with OpenCV 4.5+)
/// Used as fallback when 2023mar models fail due to OpenCV compatibility issues
pub(crate) const YUNET_MODEL_PATHS_2022: &[&str] = &[
    // Backend models directory
    "/app/backend/models/face_detection/yunet/face_detection_yunet_2022mar.onnx",
    // Container models directory
    "/app/models/face_detection/yunet/face_detection_yunet_2022mar.onnx",
    // Legacy paths
    "/app/models/face_detection_yunet_2022mar.onnx",
    // Relative paths for development
    "./backend/models/face_detection/yunet/face_detection_yunet_2022mar.onnx",
    // System paths
    "/usr/share/opencv/models/face_detection_yunet_2022mar.onnx",
];

/// Score threshold for face detection.
/// Lowered to 0.3 to detect small faces in webcam overlays (streamer content).
/// YuNet is generally reliable, so false positives at this threshold are rare.
const SCORE_THRESHOLD: f32 = 0.3;

/// NMS threshold for face detection
const NMS_THRESHOLD: f32 = 0.3;

/// Top K faces to keep
const TOP_K: i32 = 10;

/// Find the YuNet 2023mar model file (requires OpenCV 4.8+)
fn find_model_path_2023() -> Option<&'static str> {
    for path in YUNET_MODEL_PATHS_2023 {
        if Path::new(path).exists() {
            return Some(path);
        }
    }
    None
}

/// Find the YuNet 2022mar model file (compatible with OpenCV 4.5+)
fn find_model_path_2022() -> Option<&'static str> {
    for path in YUNET_MODEL_PATHS_2022 {
        if Path::new(path).exists() {
            return Some(path);
        }
    }
    None
}

/// Find any available YuNet model file (prefers 2023mar, falls back to 2022mar)
fn find_model_path() -> Option<&'static str> {
    find_model_path_2023().or_else(find_model_path_2022)
}

/// YuNet face detector using OpenCV.
///
/// This detector wraps OpenCV's FaceDetectorYN with robust error handling
/// for known compatibility issues with certain OpenCV versions.
#[cfg(feature = "opencv")]
pub struct YuNetDetector {
    /// OpenCV FaceDetectorYN instance
    detector: opencv::core::Ptr<opencv::objdetect::FaceDetectorYN>,
    /// Input size for the detector (width, height)
    input_size: (i32, i32),
    /// Original frame dimensions for coordinate scaling
    frame_size: (u32, u32),
    /// Model path for diagnostics
    model_path: String,
}

#[cfg(feature = "opencv")]
impl YuNetDetector {
    /// Create a new YuNet detector with robust initialization.
    ///
    /// Handles OpenCV version compatibility issues and validates model loading.
    /// Automatically falls back to 2022mar model if 2023mar fails due to OpenCV compatibility.
    pub fn new(frame_width: u32, frame_height: u32) -> MediaResult<Self> {
        // First try 2023mar model (better accuracy)
        if let Some(model_path) = find_model_path_2023() {
            match Self::new_with_model(model_path, frame_width, frame_height) {
                Ok(detector) => return Ok(detector),
                Err(e) => {
                    let error_str = e.to_string();
                    // Check if this is the OpenCV 4.6.0 compatibility issue
                    if error_str.contains("Layer with requested id=-1")
                        || error_str.contains("StsObjectNotFound")
                        || error_str.contains("-204")
                    {
                        warn!(
                            "2023mar model failed due to OpenCV compatibility (likely OpenCV < 4.8), \
                            falling back to 2022mar model: {}",
                            error_str
                        );
                    } else {
                        // Other error - still try 2022mar as fallback
                        warn!("2023mar model failed: {}, trying 2022mar", error_str);
                    }
                }
            }
        }

        // Fallback to 2022mar model (compatible with OpenCV 4.5+)
        if let Some(model_path) = find_model_path_2022() {
            info!("Using 2022mar model (OpenCV 4.5+ compatible)");
            return Self::new_with_model(model_path, frame_width, frame_height);
        }

        Err(MediaError::detection_failed(
            "No YuNet model found. Run download-yunet-models.sh to download models"
        ))
    }

    /// Create a YuNet detector with a specific model path.
    ///
    /// This is useful for testing specific models or for fallback scenarios.
    pub fn new_with_model(model_path: &str, frame_width: u32, frame_height: u32) -> MediaResult<Self> {
        // Validate model file exists and has reasonable size
        let model_metadata = std::fs::metadata(model_path).map_err(|e| {
            MediaError::detection_failed(format!("Cannot read YuNet model file: {}", e))
        })?;
        
        if model_metadata.len() < 50_000 {
            return Err(MediaError::detection_failed(format!(
                "YuNet model file appears corrupted (size: {} bytes)",
                model_metadata.len()
            )));
        }

        // Calculate optimal input size for the neural network
        // YuNet works best with input sizes that are multiples of 32
        let (input_width, input_height) = Self::calculate_input_size(frame_width, frame_height);

        debug!(
            "Creating YuNet detector: frame={}x{}, input={}x{}, model={}",
            frame_width, frame_height, input_width, input_height, model_path
        );

        // Try to create detector with different backends if default fails
        let detector = Self::create_detector_with_fallback(
            model_path,
            input_width,
            input_height,
        )?;

        info!(
            "YuNet detector initialized: input_size={}x{}, model={}",
            input_width, input_height, model_path
        );

        Ok(Self {
            detector,
            input_size: (input_width, input_height),
            frame_size: (frame_width, frame_height),
            model_path: model_path.to_string(),
        })
    }

    /// Calculate optimal input size for YuNet.
    ///
    /// Ensures dimensions are multiples of 32 for CNN compatibility
    /// and within reasonable bounds for performance.
    /// Uses larger dimensions to better detect small faces in webcam overlays.
    fn calculate_input_size(frame_width: u32, frame_height: u32) -> (i32, i32) {
        // Target dimensions optimized for small face detection (streamer webcam overlays).
        // A 160x200 webcam face in 1920x1080 becomes ~80x100 pixels at 960x540 input,
        // which is more reliably detected than ~53x67 at 640x480.
        let target_width = 960.0;
        let target_height = 540.0;
        
        // Calculate scale factor
        let scale = (frame_width as f64 / target_width)
            .max(frame_height as f64 / target_height)
            .max(1.0);

        // Calculate scaled dimensions
        let mut input_width = (frame_width as f64 / scale).round() as i32;
        let mut input_height = (frame_height as f64 / scale).round() as i32;

        // Round to nearest multiple of 32 for CNN feature map alignment
        const ALIGNMENT: i32 = 32;
        input_width = ((input_width + ALIGNMENT / 2) / ALIGNMENT) * ALIGNMENT;
        input_height = ((input_height + ALIGNMENT / 2) / ALIGNMENT) * ALIGNMENT;

        // Clamp to reasonable bounds (increased upper bound for small face detection)
        input_width = input_width.clamp(160, 960);
        input_height = input_height.clamp(120, 540);

        (input_width, input_height)
    }

    /// Create detector with fallback to different backends.
    ///
    /// OpenCV DNN supports multiple backends (default, OpenCV, OpenVINO, CUDA, etc.)
    /// Some backends may work better with certain model formats.
    fn create_detector_with_fallback(
        model_path: &str,
        input_width: i32,
        input_height: i32,
    ) -> MediaResult<opencv::core::Ptr<opencv::objdetect::FaceDetectorYN>> {
        use opencv::dnn::{DNN_BACKEND_DEFAULT, DNN_BACKEND_OPENCV, DNN_TARGET_CPU};

        // Backend configurations to try in order of preference
        let backends = [
            (DNN_BACKEND_DEFAULT, DNN_TARGET_CPU, "default"),
            (DNN_BACKEND_OPENCV, DNN_TARGET_CPU, "opencv"),
        ];

        let mut last_error = String::new();

        for (backend_id, target_id, backend_name) in backends {
            debug!("Trying YuNet with {} backend", backend_name);
            
            match FaceDetectorYN::create(
                model_path,
                "",
                opencv::core::Size::new(input_width, input_height),
                SCORE_THRESHOLD,
                NMS_THRESHOLD,
                TOP_K,
                backend_id,
                target_id,
            ) {
                Ok(detector) => {
                    debug!("YuNet created successfully with {} backend", backend_name);
                    return Ok(detector);
                }
                Err(e) => {
                    warn!("YuNet {} backend failed: {}", backend_name, e);
                    last_error = e.to_string();
                }
            }
        }

        Err(MediaError::detection_failed(format!(
            "Failed to create YuNet detector with any backend: {}",
            last_error
        )))
    }

    /// Detect faces in a frame with robust error handling.
    ///
    /// Returns bounding boxes in pixel coordinates.
    /// Handles the OpenCV 4.6.0 "Layer with requested id=-1" bug gracefully.
    pub fn detect_in_frame(&mut self, frame: &opencv::core::Mat) -> MediaResult<Vec<(BoundingBox, f64)>> {
        use opencv::core::{Mat, Size};
        use opencv::imgproc;
        use opencv::prelude::MatTraitConst;

        // Validate input frame
        if frame.empty() {
            debug!("Empty frame provided to YuNet detector");
            return Ok(Vec::new());
        }

        let frame_width = frame.cols();
        let frame_height = frame.rows();

        if frame_width <= 0 || frame_height <= 0 {
            debug!("Invalid frame dimensions: {}x{}", frame_width, frame_height);
            return Ok(Vec::new());
        }

        // Resize frame to detector input size
        let mut resized = Mat::default();
        if let Err(e) = imgproc::resize(
            frame,
            &mut resized,
            Size::new(self.input_size.0, self.input_size.1),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        ) {
            warn!("Failed to resize frame for YuNet: {}", e);
            return Ok(Vec::new());
        }

        // Update detector input size (required for some OpenCV versions)
        if let Err(e) = self.detector.set_input_size(Size::new(self.input_size.0, self.input_size.1)) {
            debug!("Failed to set input size (may be OK): {}", e);
        }

        // Run detection with comprehensive error handling
        let mut faces = Mat::default();
        match self.detector.detect(&resized, &mut faces) {
            Ok(_) => {}
            Err(e) => {
                let error_str = e.to_string();
                
                // Check for known OpenCV 4.6.0 bug
                if error_str.contains("Layer with requested id=-1") 
                    || error_str.contains("StsObjectNotFound")
                    || error_str.contains("-204")
                {
                    // This is the known OpenCV 4.6.0 bug - return empty gracefully
                    // The caller should fall back to heuristic detection
                    debug!("YuNet hit known OpenCV 4.6.0 bug, returning empty");
                    return Err(MediaError::detection_failed(format!(
                        "YuNet detection failed: {} (known OpenCV 4.6.0 compatibility issue)",
                        error_str
                    )));
                }
                
                // Other errors - log and return empty
                warn!("YuNet detection error: {}", e);
                return Ok(Vec::new());
            }
        }

        // Parse detection results
        self.parse_detection_results(&faces, frame_width as f64, frame_height as f64)
    }

    /// Parse YuNet detection output matrix into bounding boxes.
    ///
    /// YuNet output format per row:
    /// [x, y, w, h, x_re, y_re, x_le, y_le, x_n, y_n, x_ml, y_ml, x_mr, y_mr, score]
    /// - (x, y, w, h): face bounding box
    /// - (x_re, y_re): right eye
    /// - (x_le, y_le): left eye  
    /// - (x_n, y_n): nose tip
    /// - (x_ml, y_ml): right mouth corner
    /// - (x_mr, y_mr): left mouth corner
    /// - score: confidence score
    fn parse_detection_results(
        &self,
        faces: &opencv::core::Mat,
        frame_width: f64,
        frame_height: f64,
    ) -> MediaResult<Vec<(BoundingBox, f64)>> {
        use opencv::prelude::MatTraitConst;

        let num_faces = faces.rows();
        if num_faces <= 0 {
            return Ok(Vec::new());
        }

        // Validate matrix dimensions
        let num_cols = faces.cols();
        if num_cols < 15 {
            warn!("YuNet output has unexpected format: {} columns (expected 15)", num_cols);
            return Ok(Vec::new());
        }

        // Calculate scale factors for coordinate transformation
        let scale_x = frame_width / self.input_size.0 as f64;
        let scale_y = frame_height / self.input_size.1 as f64;

        let mut results = Vec::with_capacity(num_faces as usize);

        for i in 0..num_faces {
            // Safely extract values with bounds checking
            let x = match faces.at_2d::<f32>(i, 0) {
                Ok(v) => *v as f64 * scale_x,
                Err(_) => continue,
            };
            let y = match faces.at_2d::<f32>(i, 1) {
                Ok(v) => *v as f64 * scale_y,
                Err(_) => continue,
            };
            let w = match faces.at_2d::<f32>(i, 2) {
                Ok(v) => *v as f64 * scale_x,
                Err(_) => continue,
            };
            let h = match faces.at_2d::<f32>(i, 3) {
                Ok(v) => *v as f64 * scale_y,
                Err(_) => continue,
            };
            let score = match faces.at_2d::<f32>(i, 14) {
                Ok(v) => *v as f64,
                Err(_) => continue,
            };

            // Validate detection
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            if score < SCORE_THRESHOLD as f64 {
                continue;
            }
            if x < 0.0 || y < 0.0 || x + w > frame_width || y + h > frame_height {
                // Clamp to frame bounds
                let x_clamped = x.max(0.0);
                let y_clamped = y.max(0.0);
                let w_clamped = (w - (x_clamped - x)).min(frame_width - x_clamped);
                let h_clamped = (h - (y_clamped - y)).min(frame_height - y_clamped);
                
                if w_clamped > 0.0 && h_clamped > 0.0 {
                    let bbox = BoundingBox::new(x_clamped, y_clamped, w_clamped, h_clamped);
                    results.push((bbox, score));
                }
                continue;
            }

            let bbox = BoundingBox::new(x, y, w, h);
            results.push((bbox, score));
        }

        if results.is_empty() && num_faces > 0 {
            debug!(
                "YuNet filtered all {} candidates (threshold={}, all below confidence or invalid)",
                num_faces, SCORE_THRESHOLD
            );
        } else {
            debug!("YuNet detected {} faces (from {} candidates)", results.len(), num_faces);
        }
        Ok(results)
    }

    /// Get diagnostic information about the detector.
    #[allow(dead_code)]
    pub fn diagnostics(&self) -> String {
        format!(
            "YuNet[model={}, input={}x{}, frame={}x{}]",
            self.model_path,
            self.input_size.0,
            self.input_size.1,
            self.frame_size.0,
            self.frame_size.1,
        )
    }
}

/// Extract frames from video and detect faces using YuNet.
///
/// This function handles the full pipeline:
/// 1. Opens video with OpenCV VideoCapture
/// 2. Samples frames at the specified FPS
/// 3. Runs YuNet face detection on each frame
/// 4. Returns detections with proper error handling
///
/// If YuNet fails due to OpenCV compatibility issues, returns an error
/// so the caller can fall back to heuristic detection.
#[cfg(feature = "opencv")]
pub async fn detect_faces_with_yunet<P: AsRef<Path>>(
    video_path: P,
    start_time: f64,
    end_time: f64,
    frame_width: u32,
    frame_height: u32,
    sample_fps: f64,
) -> MediaResult<Vec<Vec<(BoundingBox, f64)>>> {
    use std::time::{Duration, Instant};
    use opencv::videoio::{VideoCapture, CAP_PROP_POS_MSEC, CAP_PROP_FRAME_WIDTH, CAP_PROP_FRAME_HEIGHT};
    use opencv::core::Mat;
    use opencv::prelude::{VideoCaptureTraitConst, VideoCaptureTrait, MatTraitConst};

    let video_path = video_path.as_ref();
    let video_path_str = video_path.to_str().unwrap_or("");
    
    // Validate inputs
    if video_path_str.is_empty() {
        return Err(MediaError::detection_failed("Empty video path"));
    }
    if end_time <= start_time {
        return Err(MediaError::detection_failed(format!(
            "Invalid time range: {} to {}",
            start_time, end_time
        )));
    }
    if sample_fps <= 0.0 {
        return Err(MediaError::detection_failed(format!(
            "Invalid sample FPS: {}",
            sample_fps
        )));
    }

    let duration = end_time - start_time;
    let sample_interval = 1.0 / sample_fps;
    let num_samples = (duration / sample_interval).ceil() as usize;
    let max_samples = num_samples.min(360);
    let max_total_detections: usize = 300;
    let max_detections_per_frame: usize = 5;
    let heartbeat_every = 40usize;

    // Open video with OpenCV
    let mut cap = VideoCapture::from_file(
        video_path_str,
        opencv::videoio::CAP_ANY,
    ).map_err(|e| MediaError::detection_failed(format!("Failed to open video: {}", e)))?;

    if !cap.is_opened().unwrap_or(false) {
        return Err(MediaError::detection_failed(format!(
            "Failed to open video file: {}",
            video_path_str
        )));
    }

    // Get actual video dimensions
    let actual_width = cap.get(CAP_PROP_FRAME_WIDTH).unwrap_or(frame_width as f64) as u32;
    let actual_height = cap.get(CAP_PROP_FRAME_HEIGHT).unwrap_or(frame_height as f64) as u32;

    debug!(
        "Video opened: {}x{} (requested: {}x{})",
        actual_width, actual_height, frame_width, frame_height
    );

    // Create detector - this may fail if model is incompatible
    let mut detector = YuNetDetector::new(actual_width, actual_height)?;
    let mut using_fallback_model = false;

    let mut all_detections = Vec::with_capacity(max_samples);
    let mut current_time = start_time;
    let mut consecutive_failures = 0;
    const MAX_CONSECUTIVE_FAILURES: usize = 3;
    let started_at = Instant::now();
    let max_wall = Duration::from_secs_f64(duration.min(120.0).max(30.0)); // bound wall time even on long clips

    info!(
        "Detecting faces with YuNet: {} frames at {:.1} fps (cap {}), threshold={}, input={}x{}",
        num_samples, sample_fps, max_samples, SCORE_THRESHOLD,
        detector.input_size.0, detector.input_size.1
    );

    for frame_idx in 0..max_samples {
        if started_at.elapsed() > max_wall {
            warn!(
                elapsed = ?started_at.elapsed(),
                "Stopping YuNet early due to wall-clock budget"
            );
            break;
        }

        // Seek to current time
        if let Err(e) = cap.set(CAP_PROP_POS_MSEC, current_time * 1000.0) {
            warn!("Failed to seek to {:.2}s: {}", current_time, e);
            all_detections.push(Vec::new());
            current_time += sample_interval;
            continue;
        }

        // Read frame
        let mut frame = Mat::default();
        let success = match cap.read(&mut frame) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read frame at {:.2}s: {}", current_time, e);
                all_detections.push(Vec::new());
                current_time += sample_interval;
                continue;
            }
        };

        if !success || frame.empty() {
            // End of video or seek past end
            debug!("Empty frame at {:.2}s (frame {})", current_time, frame_idx);
            all_detections.push(Vec::new());
            current_time += sample_interval;
            continue;
        }

        // Detect faces
        match detector.detect_in_frame(&frame) {
            Ok(detections) => {
                consecutive_failures = 0;
                let mut detections = detections;
                // Keep highest-confidence faces and cap per-frame count
                detections.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                if detections.len() > max_detections_per_frame {
                    detections.truncate(max_detections_per_frame);
                }
                all_detections.push(detections);
            }
            Err(e) => {
                let error_str = e.to_string();
                
                // Check if this is the known OpenCV 4.6.0 bug
                let is_opencv_compat_issue = error_str.contains("Layer with requested id=-1")
                    || error_str.contains("StsObjectNotFound")
                    || error_str.contains("-204")
                    || error_str.contains("OpenCV 4.6.0 compatibility");
                
                if is_opencv_compat_issue && !using_fallback_model {
                    // Try to fall back to 2022mar model on first detection failure
                    if let Some(fallback_path) = find_model_path_2022() {
                        warn!(
                            "YuNet 2023mar hit OpenCV compatibility bug on first frame, \
                            switching to 2022mar model: {}",
                            fallback_path
                        );
                        
                        match YuNetDetector::new_with_model(fallback_path, actual_width, actual_height) {
                            Ok(new_detector) => {
                                detector = new_detector;
                                using_fallback_model = true;
                                info!("Successfully switched to 2022mar model");
                                
                                // Retry detection with new model
                                match detector.detect_in_frame(&frame) {
                                    Ok(detections) => {
                                        all_detections.push(detections);
                                        current_time += sample_interval;
                                        continue;
                                    }
                                    Err(e2) => {
                                        warn!("2022mar model also failed: {}", e2);
                                    }
                                }
                            }
                            Err(e2) => {
                                warn!("Failed to create 2022mar detector: {}", e2);
                            }
                        }
                    }
                    
                    // If fallback failed, propagate the error
                    error!("YuNet hit OpenCV compatibility bug: {}", error_str);
                    return Err(MediaError::detection_failed(format!(
                        "YuNet detection failed: {} (OpenCV compatibility issue)",
                        error_str
                    )));
                }
                
                // Other errors - track consecutive failures
                consecutive_failures += 1;
                warn!(
                    "YuNet detection failed at frame {} ({:.2}s): {}",
                    frame_idx, current_time, e
                );
                
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    error!(
                        "YuNet failed {} consecutive times, aborting",
                        consecutive_failures
                    );
                    return Err(MediaError::detection_failed(format!(
                        "YuNet detection failed {} consecutive times: {}",
                        consecutive_failures, e
                    )));
                }
                
                all_detections.push(Vec::new());
            }
        }

        current_time += sample_interval;

        let total: usize = all_detections.iter().map(|d| d.len()).sum();
        if total >= max_total_detections {
            warn!(
                total,
                "Stopping YuNet early: hit total detection cap"
            );
            break;
        }

        if frame_idx % heartbeat_every == 0 {
            info!(
                frame = frame_idx,
                total_frames = max_samples,
                total_detections = total,
                "YuNet detection progress"
            );
        }
    }

    let total_detections: usize = all_detections.iter().map(|d| d.len()).sum();
    info!(
        "YuNet detection complete: {} detections across {} frames",
        total_detections,
        all_detections.len()
    );

    // If we got zero detections, that's suspicious but not necessarily an error
    if total_detections == 0 {
        warn!("YuNet found no faces in {} frames - video may not contain faces", num_samples);
    }

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
    use crate::intelligent::yunet::{YUNET_MODEL_PATHS_2023, YUNET_MODEL_PATHS_2022};

    #[test]
    fn test_model_paths() {
        // Just verify the paths are defined
        assert!(!YUNET_MODEL_PATHS_2023.is_empty());
        assert!(!YUNET_MODEL_PATHS_2022.is_empty());
    }
}
