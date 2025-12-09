# Face Mesh Models

This directory contains ONNX models for MediaPipe Face Mesh, used by the SpeakerAware intelligent tiers.

## Required Model

- **File**: `face_landmark_with_attention.onnx`
- **Source**: MediaPipe (Google), exported to ONNX (e.g., via PINTO_model_zoo)
- **Use**: Activity analysis (mouth openness) and dense landmarks.

## Setup

To download the model automatically, run the helper script:

```bash
cd backend/models/face_mesh
./donload-face-mesh.sh
```

Or manually place the file here.

## Hardware Acceleration

The face mesh inference uses ONNX Runtime (`ort` crate) with automatic hardware detection:

### macOS (Development)
- **Default**: CPU execution (Apple Silicon/Intel)
- **CUDA**: Not supported on macOS
- The code will automatically use CPU with optimized graph (Level3)

### Ubuntu 24.04 (Production)
- **CPU-only**: Works out of the box
- **NVIDIA GPU**: Automatically detected if CUDA libraries are present
  - Requires: `libcudart.so`, `libcublas.so`, `libcudnn.so`
  - The `ort` crate is built with `cuda` feature enabled
  - Set `ORT_USE_CUDA=1` to force CUDA (optional, auto-detected)

### Docker
- **CPU**: Default configuration works everywhere
- **GPU**: Add `--gpus all` to docker run and ensure NVIDIA Container Toolkit is installed
  ```bash
  docker run --gpus all ...
  ```

The implementation automatically handles CPU/GPU fallback, so the same binary works across all platforms.

## Model Details

- **Input**: 192x192 RGB image (normalized to [-1, 1])
- **Output**: 468 facial landmarks (x, y, z coordinates)
- **Format**: ONNX float32
- **Size**: 4.7 MB

## References

- [MediaPipe Face Mesh](https://developers.google.com/mediapedia/solutions/vision/face_landmarker)
- [PINTO_model_zoo](https://github.com/PINTO0309/PINTO_model_zoo)
- [Model Source](https://github.com/PINTO0309/PINTO_model_zoo/tree/main/282_face_landmark_with_attention)
