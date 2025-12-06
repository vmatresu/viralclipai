# YuNet Face Detection Models

YuNet is a lightweight CNN-based face detector optimized for speed and accuracy.

## Model Variants

### Primary Model: `face_detection_yunet_2023mar.onnx`

- **Precision**: float32
- **Performance**: ~25ms/frame
- **Accuracy**: 0.8844 AP_easy
- **Compatibility**: ✅ Works with all OpenCV builds
- **Use Case**: Production default, most reliable

### Secondary Models (Quantized)

#### `face_detection_yunet_2023mar_int8.onnx`

- **Precision**: int8 quantized
- **Performance**: ~8ms/frame
- **Accuracy**: 0.8810 AP_easy
- **Compatibility**: ⚠️ Requires DequantizeLinear ONNX support
- **Use Case**: Performance-critical deployments with compatible OpenCV

#### `face_detection_yunet_2023mar_int8bq.onnx`

- **Precision**: int8 block-quantized
- **Performance**: ~6ms/frame
- **Accuracy**: 0.8845 AP_easy
- **Compatibility**: ⚠️ Requires DequantizeLinear ONNX support
- **Use Case**: Maximum performance with compatible OpenCV

## Technical Details

- **Input**: RGB images, variable size (scaled internally)
- **Output**: Face bounding boxes + 5 facial landmarks + confidence scores
- **Architecture**: Lightweight CNN with depth-wise separable convolutions
- **Training Data**: WIDER FACE dataset
- **Framework**: OpenCV DNN with ONNX runtime

## Usage in Code

Models are automatically discovered in this priority order:

1. Primary (float32) - most compatible
2. Int8 quantized - faster
3. Block-quantized - fastest

## Requirements

- OpenCV 4.5+ with DNN module
- ONNX runtime support (built into OpenCV DNN)
- For quantized models: DequantizeLinear operator support

## Download Sources

- **Official**: https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet
- **Mirror**: https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/

## Validation

Models can be validated by checking file sizes and attempting to load with OpenCV:

```python
import cv2
model = cv2.FaceDetectorYN.create("face_detection_yunet_2023mar.onnx", "", (320, 240))
# Success = model is not None
```
