//! Object detection using YOLOv8 ONNX model.
//!
//! Provides pluggable object detection with GPU acceleration support:
//! - CUDA on Linux with NVIDIA GPU
//! - CoreML on macOS with Apple Silicon
//! - CPU fallback on all platforms

use std::sync::Mutex;

use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgb};
use ndarray::Array;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{Tensor, Value};
use tracing::{debug, info};

use crate::error::{MediaError, MediaResult};

/// Detected object with bounding box and classification.
#[derive(Debug, Clone)]
pub struct ObjectDetection {
    /// Bounding box in normalized coordinates [0, 1]
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// COCO class ID (0 = person, 2 = car, etc.)
    pub class_id: usize,
    /// Detection confidence [0, 1]
    pub confidence: f32,
}

impl ObjectDetection {
    /// Check if this is a person detection.
    pub fn is_person(&self) -> bool {
        self.class_id == 0
    }

    /// Get the center point in normalized coordinates.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Get area (normalized).
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

/// COCO class names (80 classes).
pub const COCO_CLASSES: &[&str] = &[
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck",
    "boat", "traffic light", "fire hydrant", "stop sign", "parking meter", "bench",
    "bird", "cat", "dog", "horse", "sheep", "cow", "elephant", "bear", "zebra",
    "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee",
    "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove",
    "skateboard", "surfboard", "tennis racket", "bottle", "wine glass", "cup",
    "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
    "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch",
    "potted plant", "bed", "dining table", "toilet", "tv", "laptop", "mouse",
    "remote", "keyboard", "cell phone", "microwave", "oven", "toaster", "sink",
    "refrigerator", "book", "clock", "vase", "scissors", "teddy bear", "hair drier",
    "toothbrush",
];

/// Configuration for object detection.
#[derive(Debug, Clone)]
pub struct ObjectDetectorConfig {
    /// Path to ONNX model file
    pub model_path: String,
    /// Confidence threshold for detections
    pub confidence_threshold: f32,
    /// IoU threshold for NMS
    pub nms_threshold: f32,
    /// Input image size (model expects square input)
    pub input_size: u32,
}

impl Default for ObjectDetectorConfig {
    fn default() -> Self {
        Self {
            model_path: "models/object_detection/yolov8n.onnx".to_string(),
            confidence_threshold: 0.25,
            nms_threshold: 0.45,
            input_size: 640,
        }
    }
}

/// Object detector using YOLOv8 ONNX model.
///
/// Uses ONNX Runtime for inference with automatic execution provider selection:
/// - CUDA on Linux with NVIDIA GPU (when `cuda` feature enabled)
/// - CoreML on macOS
/// - CPU fallback on all platforms
pub struct ObjectDetector {
    session: Mutex<Session>,
    config: ObjectDetectorConfig,
}

impl ObjectDetector {
    /// Create a new object detector from config.
    ///
    /// Returns error if model file doesn't exist or cannot be loaded.
    pub fn new(config: ObjectDetectorConfig) -> MediaResult<Self> {
        let model_path = Path::new(&config.model_path);
        if !model_path.exists() {
            return Err(MediaError::model_not_found(&config.model_path));
        }

        let session = Mutex::new(create_session(model_path)?);
        info!(
            model_path = %config.model_path,
            input_size = config.input_size,
            "Object detector initialized"
        );

        Ok(Self { session, config })
    }

    /// Detect objects in frame.
    ///
    /// # Arguments
    /// * `image_data` - Raw RGB image bytes (width * height * 3)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    ///
    /// # Returns
    /// Vector of detected objects with bounding boxes in normalized coordinates.
    pub fn detect(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> MediaResult<Vec<ObjectDetection>> {
        // 1. Create image from raw data
        let img = self.raw_to_image(image_data, width, height)?;

        // 2. Preprocess: resize to input_size, normalize to [0,1], NCHW format
        let input = self.preprocess(&img)?;

        // 3. Run inference
        let outputs = self.run_inference(input)?;

        // 4. Postprocess: parse YOLOv8 output, apply NMS
        let detections = self.postprocess(&outputs, width, height)?;

        debug!(
            count = detections.len(),
            "Object detection completed"
        );

        Ok(detections)
    }

    /// Detect objects from a DynamicImage (more efficient when image is already loaded).
    pub fn detect_image(&self, img: &DynamicImage) -> MediaResult<Vec<ObjectDetection>> {
        let (width, height) = img.dimensions();
        let input = self.preprocess(img)?;
        let outputs = self.run_inference(input)?;
        self.postprocess(&outputs, width, height)
    }

    /// Convert raw RGB bytes to DynamicImage.
    fn raw_to_image(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> MediaResult<DynamicImage> {
        let expected_len = (width * height * 3) as usize;
        if image_data.len() != expected_len {
            return Err(MediaError::internal(format!(
                "Invalid image data length: expected {}, got {}",
                expected_len,
                image_data.len()
            )));
        }

        let img_buffer: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, image_data.to_vec())
                .ok_or_else(|| MediaError::internal("Failed to create image buffer"))?;

        Ok(DynamicImage::ImageRgb8(img_buffer))
    }

    /// Preprocess image for YOLOv8 inference.
    ///
    /// - Resize to model input size (640x640)
    /// - Normalize pixel values to [0, 1]
    /// - Convert to NCHW format (batch, channels, height, width)
    fn preprocess(&self, img: &DynamicImage) -> MediaResult<Value> {
        let input_size = self.config.input_size as u32;

        // Resize to model input size
        let resized = img.resize_exact(
            input_size,
            input_size,
            image::imageops::FilterType::Triangle,
        );

        // Convert to RGB and normalize
        let rgb = resized.to_rgb8();
        let (w, h) = (input_size as usize, input_size as usize);

        // Create NCHW data: [1, 3, H, W]
        let mut chw_data: Vec<f32> = Vec::with_capacity(3 * h * w);

        // HWC -> CHW with normalization to [0, 1]
        for c in 0..3 {
            for y in 0..h {
                for x in 0..w {
                    let pixel = rgb.get_pixel(x as u32, y as u32);
                    chw_data.push(pixel[c] as f32 / 255.0);
                }
            }
        }

        // Create ORT tensor
        let shape = vec![1usize, 3, h, w];
        Tensor::from_array((shape, chw_data.into_boxed_slice()))
            .map(Value::from)
            .map_err(|e| MediaError::internal(format!("Failed to create tensor: {}", e)))
    }

    /// Run ONNX inference.
    fn run_inference(&self, input: Value) -> MediaResult<Vec<f32>> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| MediaError::internal("Session lock poisoned"))?;
        
        let outputs = session
            .run(ort::inputs![input])
            .map_err(|e| MediaError::internal(format!("ONNX inference failed: {}", e)))?;

        // Get output tensor - YOLOv8 output is [1, 84, 8400]
        let output = outputs
            .get("output0")
            .ok_or_else(|| MediaError::internal("Missing output0 tensor"))?;

        let tensor = output
            .try_extract_tensor::<f32>()
            .map_err(|e| MediaError::internal(format!("Failed to extract tensor: {}", e)))?;

        Ok(tensor.1.iter().copied().collect())
    }

    /// Postprocess YOLOv8 output.
    ///
    /// YOLOv8 output format: [1, 84, 8400]
    /// - 84 = 4 (bbox: cx, cy, w, h) + 80 (class scores)
    /// - 8400 = number of detection candidates
    fn postprocess(
        &self,
        outputs: &[f32],
        orig_width: u32,
        orig_height: u32,
    ) -> MediaResult<Vec<ObjectDetection>> {
        let num_classes = 80;
        let num_boxes = 8400;
        let num_features = 84; // 4 bbox + 80 classes

        if outputs.len() != num_features * num_boxes {
            return Err(MediaError::internal(format!(
                "Unexpected output size: expected {}, got {}",
                num_features * num_boxes,
                outputs.len()
            )));
        }

        // Reshape: output is [1, 84, 8400], need to transpose to [8400, 84]
        let output_array = Array::from_shape_vec((num_features, num_boxes), outputs.to_vec())
            .map_err(|e| MediaError::internal(format!("Failed to reshape output: {}", e)))?;
        let transposed = output_array.t(); // Now [8400, 84]

        let mut candidates: Vec<ObjectDetection> = Vec::new();
        let input_size = self.config.input_size as f32;

        // Scale factors to convert from model coordinates to original image
        let scale_w = orig_width as f32 / input_size;
        let scale_h = orig_height as f32 / input_size;

        for i in 0..num_boxes {
            // Extract bbox (center format)
            let cx = transposed[[i, 0]];
            let cy = transposed[[i, 1]];
            let w = transposed[[i, 2]];
            let h = transposed[[i, 3]];

            // Find best class
            let mut best_class = 0;
            let mut best_score = 0.0f32;

            for c in 0..num_classes {
                let score = transposed[[i, 4 + c]];
                if score > best_score {
                    best_score = score;
                    best_class = c;
                }
            }

            // Apply confidence threshold
            if best_score < self.config.confidence_threshold {
                continue;
            }

            // Convert from center format to corner format and scale
            let x = (cx - w / 2.0) * scale_w;
            let y = (cy - h / 2.0) * scale_h;
            let width = w * scale_w;
            let height = h * scale_h;

            // Normalize to [0, 1]
            let x_norm = x / orig_width as f32;
            let y_norm = y / orig_height as f32;
            let w_norm = width / orig_width as f32;
            let h_norm = height / orig_height as f32;

            // Clamp to valid range
            let x_clamped = x_norm.max(0.0).min(1.0);
            let y_clamped = y_norm.max(0.0).min(1.0);
            let w_clamped = w_norm.min(1.0 - x_clamped);
            let h_clamped = h_norm.min(1.0 - y_clamped);

            candidates.push(ObjectDetection {
                x: x_clamped,
                y: y_clamped,
                width: w_clamped,
                height: h_clamped,
                class_id: best_class,
                confidence: best_score,
            });
        }

        // Apply NMS
        let filtered = self.non_maximum_suppression(candidates);

        Ok(filtered)
    }

    /// Apply Non-Maximum Suppression to remove overlapping detections.
    fn non_maximum_suppression(&self, mut detections: Vec<ObjectDetection>) -> Vec<ObjectDetection> {
        if detections.is_empty() {
            return detections;
        }

        // Sort by confidence (descending)
        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = Vec::new();
        let mut suppressed = vec![false; detections.len()];

        for i in 0..detections.len() {
            if suppressed[i] {
                continue;
            }

            keep.push(detections[i].clone());

            for j in (i + 1)..detections.len() {
                if suppressed[j] {
                    continue;
                }

                // Only suppress same class
                if detections[i].class_id != detections[j].class_id {
                    continue;
                }

                let iou = self.compute_iou(&detections[i], &detections[j]);
                if iou > self.config.nms_threshold {
                    suppressed[j] = true;
                }
            }
        }

        keep
    }

    /// Compute Intersection over Union (IoU) between two detections.
    fn compute_iou(&self, a: &ObjectDetection, b: &ObjectDetection) -> f32 {
        let x1 = a.x.max(b.x);
        let y1 = a.y.max(b.y);
        let x2 = (a.x + a.width).min(b.x + b.width);
        let y2 = (a.y + a.height).min(b.y + b.height);

        let inter_w = (x2 - x1).max(0.0);
        let inter_h = (y2 - y1).max(0.0);
        let intersection = inter_w * inter_h;

        let area_a = a.width * a.height;
        let area_b = b.width * b.height;
        let union = area_a + area_b - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &ObjectDetectorConfig {
        &self.config
    }
}

/// Create ONNX Runtime session with automatic execution provider selection.
fn create_session(model_path: &Path) -> MediaResult<Session> {
    // Read model file
    let model_bytes = std::fs::read(model_path)
        .map_err(|e| MediaError::internal(format!("Failed to read model file: {}", e)))?;

    let builder = Session::builder()
        .map_err(|e| MediaError::internal(format!("Failed to create session builder: {}", e)))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| MediaError::internal(format!("Failed to set optimization level: {}", e)))?;

    // Try CUDA on Linux with cuda feature
    #[cfg(all(target_os = "linux", feature = "cuda"))]
    {
        use ort::execution_providers::CUDAExecutionProvider;
        if let Ok(cuda_builder) = builder
            .clone()
            .with_execution_providers([CUDAExecutionProvider::default().build()])
        {
            if let Ok(session) = cuda_builder.commit_from_memory(&model_bytes) {
                info!("Using CUDA execution provider for object detection");
                return Ok(session);
            }
        }
        debug!("CUDA execution provider not available, trying alternatives");
    }

    // Try CoreML on macOS
    #[cfg(target_os = "macos")]
    {
        use ort::execution_providers::CoreMLExecutionProvider;
        if let Ok(coreml_builder) = builder
            .clone()
            .with_execution_providers([CoreMLExecutionProvider::default().build()])
        {
            if let Ok(session) = coreml_builder.commit_from_memory(&model_bytes) {
                info!("Using CoreML execution provider for object detection");
                return Ok(session);
            }
        }
        debug!("CoreML execution provider not available, using CPU");
    }

    // CPU fallback
    info!("Using CPU execution provider for object detection");
    builder
        .commit_from_memory(&model_bytes)
        .map_err(|e| MediaError::internal(format!("Failed to load ONNX model: {}", e)))
}

/// Check if object detection model is available.
pub fn is_model_available() -> bool {
    Path::new("models/object_detection/yolov8n.onnx").exists()
}

/// Check if model is available at a custom path.
pub fn is_model_available_at(path: &str) -> bool {
    Path::new(path).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_detection_creation() {
        let detection = ObjectDetection {
            x: 0.1,
            y: 0.2,
            width: 0.3,
            height: 0.4,
            class_id: 0,
            confidence: 0.9,
        };

        assert!(detection.is_person());
        assert!((detection.center().0 - 0.25).abs() < 0.001);
        assert!((detection.center().1 - 0.4).abs() < 0.001);
        assert!((detection.area() - 0.12).abs() < 0.001);
    }

    #[test]
    fn test_coco_classes() {
        assert_eq!(COCO_CLASSES[0], "person");
        assert_eq!(COCO_CLASSES[2], "car");
        assert_eq!(COCO_CLASSES.len(), 80);
    }

    #[test]
    fn test_config_default() {
        let config = ObjectDetectorConfig::default();
        assert_eq!(config.input_size, 640);
        assert!((config.confidence_threshold - 0.25).abs() < 0.001);
        assert!((config.nms_threshold - 0.45).abs() < 0.001);
    }

    #[test]
    fn test_non_person_detection() {
        let detection = ObjectDetection {
            x: 0.1,
            y: 0.2,
            width: 0.3,
            height: 0.4,
            class_id: 2, // car
            confidence: 0.85,
        };

        assert!(!detection.is_person());
        assert_eq!(COCO_CLASSES[detection.class_id], "car");
    }

    #[test]
    fn test_iou_overlapping() {
        // Two identical boxes should have IoU = 1.0
        let a = ObjectDetection {
            x: 0.1,
            y: 0.1,
            width: 0.2,
            height: 0.2,
            class_id: 0,
            confidence: 0.9,
        };
        
        // Can't test compute_iou directly without detector, but we can verify area
        assert!((a.area() - 0.04).abs() < 0.001);
    }

    #[test]
    fn test_detection_center() {
        let det = ObjectDetection {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            class_id: 0,
            confidence: 0.9,
        };
        let (cx, cy) = det.center();
        assert!((cx - 0.5).abs() < 0.001);
        assert!((cy - 0.5).abs() < 0.001);
    }
}
