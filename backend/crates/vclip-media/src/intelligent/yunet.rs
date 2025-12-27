//! OpenCV YuNet face detector with INT8 quantization support.
//!
//! YuNet is a lightweight CNN face detector exposed via OpenCV's FaceDetectorYN API.
//! This module handles model loading, backend selection, and inference.
//!
//! # Model Selection
//!
//! Models are selected via [`super::model_config::ModelConfig`] with priority:
//! 1. Environment variable `YUNET_MODEL_VARIANT` (int8bq, int8, fp32, legacy)
//! 2. Auto-detection preferring fastest available (int8bq > int8 > fp32)
//!
//! # Requirements
//! - OpenCV 4.5+ with DNN module
//! - 2023mar models require OpenCV 4.8+
//!
//! # Known Issues
//! - OpenCV 4.6.0 has a bug with FaceDetectorYN ("Layer with requested id=-1").
//!   Handled gracefully with fallback to 2022mar model.

use super::model_config::{get_resolved_model, is_model_available, ModelVariant};
use super::models::BoundingBox;
use crate::error::{MediaError, MediaResult};
#[cfg(feature = "opencv")]
use opencv::objdetect::FaceDetectorYN;
#[cfg(feature = "opencv")]
use opencv::prelude::FaceDetectorYNTrait;
use std::path::Path;
use std::sync::OnceLock;
use tracing::{debug, error, info, warn};

/// Global YuNet availability flag (cached).
static YUNET_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Check if YuNet is available.
///
/// Uses the centralized model configuration from [`super::model_config`].
pub fn is_yunet_available() -> bool {
    *YUNET_AVAILABLE.get_or_init(|| {
        let available = is_model_available();
        if !available {
            warn!("YuNet model not found - using FFmpeg heuristic detection");
            warn!("Download: curl -L -o /app/models/face_detection_yunet_2023mar_int8bq.onnx \\");
            warn!("  https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx");
        }
        available
    })
}

/// Check if YuNet is available, with automatic download attempt.
pub async fn ensure_yunet_available() -> bool {
    if is_yunet_available() {
        return true;
    }

    #[cfg(feature = "opencv")]
    {
        if let Ok(()) = download_best_model().await {
            return true;
        }
    }

    false
}

/// Download the fastest available model (INT8-BQ).
#[cfg(feature = "opencv")]
async fn download_best_model() -> MediaResult<()> {
    use tokio::process::Command;

    let model_dir = Path::new("/app/models/face_detection/yunet");
    if !model_dir.exists() {
        tokio::fs::create_dir_all(model_dir)
            .await
            .map_err(|e| MediaError::detection_failed(format!("Create dir: {}", e)))?;
    }

    let model_path = model_dir.join(ModelVariant::Int8BlockQuantized.filename_pattern());
    let model_url = "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar_int8bq.onnx";

    info!("Downloading YuNet INT8-BQ model...");
    let status = Command::new("curl")
        .args(["-L", "-o", &model_path.to_string_lossy(), model_url])
        .status()
        .await
        .map_err(|e| MediaError::detection_failed(format!("Download: {}", e)))?;

    if status.success() && model_path.exists() {
        info!("YuNet model downloaded: {}", model_path.display());
        Ok(())
    } else {
        Err(MediaError::detection_failed(
            "Failed to download YuNet model",
        ))
    }
}

/// Find the resolved model path (deprecated, use model_config directly).
#[deprecated(since = "0.2.0", note = "Use model_config::get_resolved_model instead")]
pub fn find_model_path() -> Option<String> {
    get_resolved_model()
        .ok()
        .map(|(_, p)| p.to_string_lossy().to_string())
}

/// Score threshold for face detection.
/// Lowered to 0.3 to detect small faces in webcam overlays.
const SCORE_THRESHOLD: f32 = 0.3;

/// NMS threshold for face detection.
const NMS_THRESHOLD: f32 = 0.3;

/// Top K faces to keep.
const TOP_K: i32 = 10;

/// YuNet face detector using OpenCV.
///
/// Wraps OpenCV's FaceDetectorYN with robust error handling
/// for known compatibility issues with certain OpenCV versions.
#[cfg(feature = "opencv")]
pub struct YuNetDetector {
    detector: opencv::core::Ptr<opencv::objdetect::FaceDetectorYN>,
    input_size: (i32, i32),
    frame_size: (u32, u32),
    model_path: String,
    variant: ModelVariant,
}

#[cfg(feature = "opencv")]
impl YuNetDetector {
    /// Create a new YuNet detector using centralized model configuration.
    ///
    /// Uses [`super::model_config`] to resolve the best available model,
    /// with automatic fallback to legacy models for OpenCV compatibility.
    pub fn new(frame_width: u32, frame_height: u32) -> MediaResult<Self> {
        use super::model_config::ModelConfig;

        let config = ModelConfig::from_env();
        let (variant, model_path) = config
            .resolve()
            .map_err(|e| MediaError::detection_failed(e.to_string()))?;

        let path_str = model_path.to_string_lossy();
        match Self::new_with_model_and_variant(&path_str, frame_width, frame_height, variant) {
            Ok(detector) => Ok(detector),
            Err(e) if Self::is_opencv_compat_error(&e) => {
                // Try legacy 2022 model for OpenCV < 4.8
                warn!("Model failed (OpenCV compat), trying legacy: {}", e);
                Self::try_legacy_fallback(frame_width, frame_height)
            }
            Err(e) => Err(e),
        }
    }

    /// Check if error is an OpenCV version compatibility issue.
    fn is_opencv_compat_error(e: &MediaError) -> bool {
        let s = e.to_string();
        s.contains("Layer with requested id=-1")
            || s.contains("StsObjectNotFound")
            || s.contains("-204")
    }

    /// Try legacy 2022 model as fallback for older OpenCV versions.
    fn try_legacy_fallback(frame_width: u32, frame_height: u32) -> MediaResult<Self> {
        use super::model_config::ModelConfig;

        let config = ModelConfig::with_variant(ModelVariant::Legacy2022);
        match config.resolve() {
            Ok((variant, path)) => {
                info!("Using legacy 2022mar model (OpenCV 4.5+ compatible)");
                Self::new_with_model_and_variant(
                    &path.to_string_lossy(),
                    frame_width,
                    frame_height,
                    variant,
                )
            }
            Err(e) => Err(MediaError::detection_failed(format!(
                "No fallback model available: {}",
                e
            ))),
        }
    }

    /// Create with explicit model path (for testing or custom models).
    pub fn new_with_model(
        model_path: &str,
        frame_width: u32,
        frame_height: u32,
    ) -> MediaResult<Self> {
        // Infer variant from path
        let variant = if model_path.contains("int8bq") {
            ModelVariant::Int8BlockQuantized
        } else if model_path.contains("int8") {
            ModelVariant::Int8
        } else if model_path.contains("2022") {
            ModelVariant::Legacy2022
        } else {
            ModelVariant::Fp32
        };
        Self::new_with_model_and_variant(model_path, frame_width, frame_height, variant)
    }

    /// Create with explicit model path and variant.
    fn new_with_model_and_variant(
        model_path: &str,
        frame_width: u32,
        frame_height: u32,
        variant: ModelVariant,
    ) -> MediaResult<Self> {
        // Validate model file
        let metadata = std::fs::metadata(model_path)
            .map_err(|e| MediaError::detection_failed(format!("Read model: {}", e)))?;

        if metadata.len() < 50_000 {
            return Err(MediaError::detection_failed(format!(
                "Model corrupted ({} bytes)",
                metadata.len()
            )));
        }

        let (input_width, input_height) = Self::calculate_input_size(frame_width, frame_height);

        debug!(
            "Creating YuNet: frame={}x{}, input={}x{}, variant={:?}",
            frame_width, frame_height, input_width, input_height, variant
        );

        let detector = Self::create_detector_with_fallback(model_path, input_width, input_height)?;

        info!(
            "YuNet initialized: {} @ {}x{} (path: {})",
            variant, input_width, input_height, model_path
        );

        Ok(Self {
            detector,
            input_size: (input_width, input_height),
            frame_size: (frame_width, frame_height),
            model_path: model_path.to_string(),
            variant,
        })
    }

    /// Get the model variant in use.
    pub fn variant(&self) -> ModelVariant {
        self.variant
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
    ///
    /// Priority order:
    /// 1. OpenVINO (DNN_BACKEND_INFERENCE_ENGINE) - Best performance on Intel CPUs
    /// 2. OpenCV DNN (DNN_BACKEND_OPENCV) - Universal fallback
    /// 3. Default (DNN_BACKEND_DEFAULT) - Let OpenCV decide
    fn create_detector_with_fallback(
        model_path: &str,
        input_width: i32,
        input_height: i32,
    ) -> MediaResult<opencv::core::Ptr<opencv::objdetect::FaceDetectorYN>> {
        use opencv::dnn::{
            DNN_BACKEND_DEFAULT, DNN_BACKEND_INFERENCE_ENGINE, DNN_BACKEND_OPENCV, DNN_TARGET_CPU,
        };

        // Backend configurations to try in order of preference
        // OpenVINO first for optimal performance on Intel CPUs
        let backends = [
            (DNN_BACKEND_INFERENCE_ENGINE, DNN_TARGET_CPU, "OpenVINO"),
            (DNN_BACKEND_OPENCV, DNN_TARGET_CPU, "OpenCV"),
            (DNN_BACKEND_DEFAULT, DNN_TARGET_CPU, "default"),
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
                    info!("YuNet initialized with {} backend", backend_name);
                    return Ok(detector);
                }
                Err(e) => {
                    debug!("YuNet {} backend failed: {}", backend_name, e);
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
    pub fn detect_in_frame(
        &mut self,
        frame: &opencv::core::Mat,
    ) -> MediaResult<Vec<(BoundingBox, f64)>> {
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
        if let Err(e) = self
            .detector
            .set_input_size(Size::new(self.input_size.0, self.input_size.1))
        {
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
            warn!(
                "YuNet output has unexpected format: {} columns (expected 15)",
                num_cols
            );
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
            debug!(
                "YuNet detected {} faces (from {} candidates)",
                results.len(),
                num_faces
            );
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
    use opencv::core::Mat;
    use opencv::prelude::{MatTraitConst, VideoCaptureTrait, VideoCaptureTraitConst};
    use opencv::videoio::{
        VideoCapture, CAP_PROP_FRAME_HEIGHT, CAP_PROP_FRAME_WIDTH, CAP_PROP_POS_MSEC,
    };

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
    let mut cap = VideoCapture::from_file(video_path_str, opencv::videoio::CAP_ANY)
        .map_err(|e| MediaError::detection_failed(format!("Failed to open video: {}", e)))?;

    if !cap.is_opened().unwrap_or(false) {
        return Err(MediaError::detection_failed(format!(
            "Failed to open video file: {}",
            video_path_str
        )));
    }

    // Get actual video dimensions
    let actual_width = cap.get(CAP_PROP_FRAME_WIDTH).unwrap_or(frame_width as f64) as u32;
    let actual_height = cap
        .get(CAP_PROP_FRAME_HEIGHT)
        .unwrap_or(frame_height as f64) as u32;

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
    info!(
        "Detecting faces with YuNet: {} frames at {:.1} fps (cap {}), threshold={}, input={}x{}",
        num_samples,
        sample_fps,
        max_samples,
        SCORE_THRESHOLD,
        detector.input_size.0,
        detector.input_size.1
    );

    for frame_idx in 0..max_samples {
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
                detections
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
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
                    // Try legacy 2022 model as fallback
                    use super::model_config::ModelConfig;
                    let legacy_config = ModelConfig::with_variant(ModelVariant::Legacy2022);
                    if let Ok((_, fallback_path)) = legacy_config.resolve() {
                        let fallback_path_str = fallback_path.to_string_lossy();
                        warn!(
                            "YuNet hit OpenCV compat bug, trying legacy model: {}",
                            fallback_path_str
                        );

                        match YuNetDetector::new_with_model(
                            &fallback_path_str,
                            actual_width,
                            actual_height,
                        ) {
                            Ok(new_detector) => {
                                detector = new_detector;
                                using_fallback_model = true;
                                info!("Switched to legacy 2022mar model");

                                // Retry detection with new model
                                match detector.detect_in_frame(&frame) {
                                    Ok(detections) => {
                                        all_detections.push(detections);
                                        current_time += sample_interval;
                                        continue;
                                    }
                                    Err(e2) => {
                                        warn!("Legacy model also failed: {}", e2);
                                    }
                                }
                            }
                            Err(e2) => {
                                warn!("Failed to create legacy detector: {}", e2);
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
            warn!(total, "Stopping YuNet early: hit total detection cap");
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
        warn!(
            "YuNet found no faces in {} frames - video may not contain faces",
            num_samples
        );
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
    use crate::intelligent::yunet::{YUNET_MODEL_PATHS_2022, YUNET_MODEL_PATHS_2023};

    #[test]
    fn test_model_paths() {
        // Just verify the paths are defined
        assert!(!YUNET_MODEL_PATHS_2023.is_empty());
        assert!(!YUNET_MODEL_PATHS_2022.is_empty());
    }
}
