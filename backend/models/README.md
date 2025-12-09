## Face Detection and Face Mesh Models

### YuNet face detector
- Location (checked automatically): `backend/models/face_detection/yunet/face_detection_yunet_2023mar*.onnx`
- If missing, run the existing YuNet download script or place the model under the path above.

### MediaPipe Face Mesh (landmarks)
- Expected path: `backend/models/face_mesh/face_landmark_with_attention.onnx`
- The code resolves relative to `CARGO_MANIFEST_DIR`, so in Docker/runtime use `/app/backend/models/face_mesh/face_landmark_with_attention.onnx`.

### Debug overlay for face activity
- Set env var to enable debug renders of the face-mesh crop and lip landmarks:
  - `DEBUG_RENDER_FACE_ACTIVITY=1`
- Output: annotated frames are written to `/tmp/face_mesh_debug/` (crop box in blue, lip landmarks in green, mouth openness text). Useful for tuning thresholds without affecting production behavior.
# Machine Learning Models Directory

This directory contains pre-trained machine learning models used by ViralClip AI.

## Directory Structure

```
models/
├── face_detection/
│   ├── yunet/
│   │   ├── face_detection_yunet_2023mar.onnx          # Primary: float32 (requires OpenCV 4.8+)
│   │   ├── face_detection_yunet_2023mar_int8.onnx     # Int8 quantized (requires OpenCV 4.8+)
│   │   ├── face_detection_yunet_2023mar_int8bq.onnx   # Block-quantized (requires OpenCV 4.8+)
│   │   ├── face_detection_yunet_2022mar.onnx          # Fallback: compatible with OpenCV 4.5+
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

1. `face_detection_yunet_2023mar.onnx` - Primary (float32, requires OpenCV 4.8+)
2. `face_detection_yunet_2023mar_int8.onnx` - Faster (int8, requires OpenCV 4.8+)
3. `face_detection_yunet_2023mar_int8bq.onnx` - Fastest (block-quantized, requires OpenCV 4.8+)
4. `face_detection_yunet_2022mar.onnx` - Fallback (compatible with OpenCV 4.5+)

### OpenCV Compatibility

**IMPORTANT**: The 2023mar models require OpenCV 4.8+ due to new ONNX operators.

| Model Version | OpenCV Required | Notes                                        |
| ------------- | --------------- | -------------------------------------------- |
| 2023mar       | 4.8+            | Better accuracy (0.88 AP), new architecture  |
| 2022mar       | 4.5+            | Good accuracy (0.83 AP), wider compatibility |

**Known Issue**: OpenCV 4.6.0 and 4.7.0 will fail with error:

```
Layer with requested id=-1 not found in function 'getLayerData'
```

The Rust code automatically detects this error and falls back to the 2022mar model.

### Performance Comparison

| Model              | Precision | Speed       | Accuracy (AP_easy) | OpenCV Version |
| ------------------ | --------- | ----------- | ------------------ | -------------- |
| 2023mar            | float32   | ~25ms/frame | 0.8844             | 4.8+           |
| 2023mar Int8       | int8      | ~8ms/frame  | 0.8810             | 4.8+           |
| 2023mar Int8-BQ    | int8      | ~6ms/frame  | 0.8845             | 4.8+           |
| 2022mar (fallback) | float32   | ~30ms/frame | 0.8340             | 4.5+           |

### Version Control

Models are committed to version control for:

- Reproducible builds
- Offline development
- CI/CD reliability
- Version consistency across environments
