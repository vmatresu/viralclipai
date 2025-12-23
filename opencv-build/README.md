# OpenCV Build System

One-command build for **OpenCV 4.12.0** with **OpenVINO** integration for accelerated DNN inference.

## Quick Start

```bash
# On your worker server:
./build.sh --native
```

That's it. The script will:
1. Auto-detect your Ubuntu version and install OpenVINO if needed
2. Auto-detect your CPU and choose the optimal ISA profile
3. Clone OpenCV source, configure with CMake, and build with Ninja

## Commands

| Command | Description |
|---------|-------------|
| `./build.sh` | Build with Docker (auto-detect profile) |
| `./build.sh --native` | Build directly on host (recommended for servers) |
| `./build.sh info` | Show CPU capabilities and OpenVINO status |
| `./build.sh install-openvino` | Install OpenVINO only |
| `./build.sh --help` | Show all options |

## Options

| Option | Description |
|--------|-------------|
| `--native` | Build on host instead of Docker |
| `--profile portable` | Force AVX2-only build (compatible with all modern CPUs) |
| `--profile tuned` | Force AVX-512 build (fastest, requires AVX-512 CPU) |

## ISA Profiles

| Profile | CPU Baseline | Dispatch | Compatible CPUs |
|---------|--------------|----------|-----------------|
| `portable` | AVX2 | None | Intel Haswell+ (2013), AMD Zen+ (2017) |
| `tuned` | AVX2 | AVX-512 | Intel Skylake-X+, AMD EPYC 7002+ |

The script auto-detects your CPU and selects the appropriate profile.

## Directory Structure

```
opencv-build/
├── build.sh              # Main entry point (run this!)
├── lib/
│   ├── cpu.sh            # CPU feature detection
│   ├── openvino.sh       # OpenVINO auto-install
│   └── cmake-config.sh   # CMake configuration
├── Dockerfile.openvino   # Multi-stage Docker build
└── README.md             # This file
```

## Output

**Native build** (`--native`):
- OpenCV installed to `/usr/local`
- Build info in `./build/opencv-build-info.txt`

**Docker build** (default):
- Tarball in `./artifacts/opencv-4.12.0-openvino-<profile>.tar.gz`

## Requirements

- Ubuntu 20.04, 22.04, or 24.04
- Root access (for `apt-get` and `ninja install`)
- ~10GB disk space
- 15-30 minutes build time

## Environment Variables

```bash
OPENCV_VERSION=4.12.0       # Override OpenCV version
OPENVINO_VERSION=2024.4     # Override OpenVINO version
```
