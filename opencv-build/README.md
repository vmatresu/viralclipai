# OpenCV Build with OpenVINO Integration

Production-grade OpenCV build optimized for YuNet face detection with OpenVINO DNN backend.

## Overview

This directory contains build infrastructure for creating optimized OpenCV artifacts with:

- **OpenVINO Integration**: Hardware-accelerated DNN inference
- **TBB Threading**: Parallel processing via Intel Threading Building Blocks
- **IPP Acceleration**: Intel Performance Primitives for image operations
- **Configurable ISA Profiles**: Portable (AVX2) or Tuned (AVX-512)

## ISA Profiles

### Portable (Default)

```bash
./build-portable.sh
```

- **CPU Baseline**: AVX2
- **Target CPUs**: All modern x86_64 (Intel Haswell 2013+, AMD Zen 2017+)
- **Use Case**: Default production image, works everywhere
- **Cloud Compatibility**: AWS, GCP, Azure - all instance types

### Tuned

```bash
./build-tuned.sh
```

- **CPU Baseline**: AVX2 with AVX-512 dispatch
- **Target CPUs**: Intel Skylake-X, Ice Lake, Sapphire Rapids; AMD EPYC Zen 2/3/4
- **Use Case**: Pinned fleets with known AVX-512 support
- **IMPORTANT**: Requires runtime CPU verification

## Build Requirements

- Docker 20.10+
- Docker Buildx
- ~10GB disk space
- ~60 minutes build time

## Manual Build

```bash
# Build with Docker directly
docker buildx build \
    --target export \
    --build-arg ISA_PROFILE=portable \
    --build-arg OPENCV_VERSION=4.12.0 \
    --build-arg OPENVINO_VERSION=2024.4 \
    -f Dockerfile.openvino \
    -o type=local,dest=./artifacts \
    .
```

## Auto Profile Selection

On the build machine, you can auto-select the best profile based on CPU flags:

```bash
./build-auto.sh
```

Print the recommended profile without building:

```bash
./which-profile.sh
```

Build both profiles (tuned will be skipped if AVX-512 is not available):

```bash
./build-auto.sh both
```

Force a specific profile:

```bash
./build-auto.sh portable
./build-auto.sh tuned
```

## Build Artifacts

After building, artifacts are in `./artifacts/`:

| File                                     | Description                  |
| ---------------------------------------- | ---------------------------- |
| `opencv-4.12.0-openvino-portable.tar.gz` | OpenCV libraries and headers |
| `opencv-build-info.txt`                  | Build configuration details  |

## Verification

After building, verify the artifacts:

```bash
# Extract artifacts
sudo tar -xzf artifacts/opencv-4.12.0-openvino-portable.tar.gz -C /usr/local
sudo ldconfig

# Verify OpenVINO is enabled
python3 -c "
import cv2
info = cv2.getBuildInformation()
print('OpenVINO enabled:', 'YES' in [l for l in info.split('\n') if 'OpenVINO' in l][0])
print('CPU Baseline:', [l for l in info.split('\n') if 'CPU_BASELINE' in l][0])
"
```

## Integration

### Dockerfile Usage

```dockerfile
# Copy pre-built OpenCV artifacts
COPY opencv-artifacts/opencv-4.12.0-openvino-portable.tar.gz /tmp/opencv.tar.gz
RUN cd /usr/local && \
    tar -xzf /tmp/opencv.tar.gz && \
    rm /tmp/opencv.tar.gz && \
    ldconfig
```

### Tuned Build Startup Guard

When using the tuned (AVX-512) OpenCV artifacts, enable the runtime guard to
fail fast if the CPU lacks AVX-512 support:

```bash
VCLIP_TUNED_BUILD=1
```

Leave it unset for portable builds.

### Runtime CPU Verification (Tuned Profile)

When using the tuned profile, verify CPU features at startup:

```rust
use std::arch::is_x86_feature_detected;

fn verify_cpu_features() -> Result<(), &'static str> {
    if !is_x86_feature_detected!("avx512f") {
        return Err("AVX-512 required but not available");
    }
    Ok(())
}
```

## CI/CD

Builds are automated via GitHub Actions:

- **opencv-build.yml**: Builds artifacts on changes or weekly
- **opencv-build-verification.yml**: Validates build configuration

## Modules Included

### Core Modules

- `core`, `imgproc`, `imgcodecs`, `videoio`
- `dnn` (neural network inference)
- `objdetect` (FaceDetectorYN)
- `calib3d`, `features2d`, `flann`

### Contrib Modules

- `face` (face recognition)
- `tracking` (object tracking)
- `optflow` (optical flow)
- `ximgproc`, `xobjdetect`

### Excluded (for minimal size)

- GUI modules (gtk, qt, opengl)
- Python bindings
- CUDA/GPU support
- Tests and examples

## Performance Comparison

| Backend                 | 1080p YuNet Inference | Notes              |
| ----------------------- | --------------------- | ------------------ |
| OpenCV DNN (SSE3)       | ~25ms                 | Default fallback   |
| OpenCV DNN (AVX2)       | ~12ms                 | Portable profile   |
| OpenVINO (AVX2)         | ~6ms                  | With OpenVINO      |
| OpenVINO (AVX-512 VNNI) | ~2ms                  | Tuned + INT8 model |

## Troubleshooting

### OpenVINO not found during build

Ensure the OpenVINO apt repository is configured:

```bash
curl -fsSL https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB \
    | gpg --dearmor -o /usr/share/keyrings/intel-openvino.gpg
echo "deb [signed-by=/usr/share/keyrings/intel-openvino.gpg] https://apt.repos.intel.com/openvino/2024 ubuntu24 main" \
    > /etc/apt/sources.list.d/intel-openvino.list
```

### SIGILL on AVX-512 tuned build

The tuned profile requires AVX-512 capable CPUs. Use the portable profile for general deployment, or implement runtime CPU verification.

### YuNet model compatibility

- YuNet 2023mar models require OpenCV 4.8+
- Use 2022mar model as fallback for older OpenCV versions
