# Machine Learning Models Directory

This directory contains pre-trained machine learning models used by ViralClip AI.

## Directory Structure

```
models/
├── face_detection/
│   ├── yunet/
│   │   ├── face_detection_yunet_2023mar.onnx          # Primary: float32, most compatible
│   │   ├── face_detection_yunet_2023mar_int8.onnx     # Int8 quantized: faster but limited compatibility
│   │   ├── face_detection_yunet_2023mar_int8bq.onnx   # Block-quantized: fastest but limited compatibility
│   │   ├── README.md                                  # Model documentation
│   │   └── checksums.sha256                           # Integrity verification
│   └── README.md
└── README.md
```

## Model Management

### Downloading Models

Use the download script to fetch and verify models:

```bash
# From project root
./download-yunet-models.sh

# Or manually
cd backend/models/face_detection/yunet
curl -L -o face_detection_yunet_2023mar.onnx \
  "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx"
```

### Model Priority

Models are loaded in this order of preference:

1. `face_detection_yunet_2023mar.onnx` - Primary (float32, most compatible)
2. `face_detection_yunet_2023mar_int8.onnx` - Fallback (int8 quantized)
3. `face_detection_yunet_2023mar_int8bq.onnx` - Last resort (block-quantized)

### Compatibility Notes

- **Float32 models**: Work with all OpenCV builds
- **Quantized models**: Require `DequantizeLinear` ONNX operator support
- Some OpenCV installations (especially in containers) may not support quantized models

### Performance Comparison

| Model           | Precision | Speed       | Accuracy (AP_easy) | Compatibility |
| --------------- | --------- | ----------- | ------------------ | ------------- |
| Original        | float32   | ~25ms/frame | 0.8844             | ✅ Universal  |
| Int8 Quantized  | int8      | ~8ms/frame  | 0.8810             | ⚠️ Limited    |
| Block-Quantized | int8      | ~6ms/frame  | 0.8845             | ⚠️ Limited    |

### Version Control

Models are committed to version control for:

- Reproducible builds
- Offline development
- CI/CD reliability
- Version consistency across environments
