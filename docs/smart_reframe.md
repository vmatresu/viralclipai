# Smart Reframe - Intelligent Video Cropping

Smart Reframe is an intelligent video reframing module that automatically crops horizontal/widescreen videos into vertical/portrait formats (9:16, 4:5, 1:1) while keeping important subjects well-framed.

## Features

- **Face/Person Detection**: Uses MediaPipe for lightweight, CPU-friendly face detection
- **Shot Detection**: Automatic scene change detection using histogram analysis
- **Smooth Camera Motion**: Temporal smoothing to eliminate jitter
- **Multiple Aspect Ratios**: Support for 9:16 (TikTok/Reels), 4:5 (Instagram), 1:1 (Square)
- **Configurable**: Extensive configuration options for different use cases
- **CLI & API**: Both command-line and Python API interfaces

## Installation

The required dependencies are included in `requirements.txt`:

```bash
pip install opencv-python-headless mediapipe numpy
```

## Quick Start

### Python API

```python
from app.core.smart_reframe import Reframer, AspectRatio, IntelligentCropConfig

# Basic usage
reframer = Reframer(
    target_aspect_ratios=[AspectRatio(9, 16)],
)

# Analyze and render in one call
output_paths = reframer.analyze_and_render(
    input_path="input.mp4",
    output_prefix="output_portrait"
)

print(output_paths)  # {"9:16": "output_portrait_9x16.mp4"}
```

### Separate Analysis and Rendering

```python
# Step 1: Analyze video (can be cached)
crop_plan = reframer.analyze(input_path="input.mp4")

# Save crop plan for later use
crop_plan.to_json_file("cropplan.json")

# Step 2: Render (can be done later or with different settings)
output_paths = reframer.render(
    input_path="input.mp4",
    crop_plan=crop_plan,
    output_prefix="output"
)
```

### Process a Time Range

```python
# Only process 10-40 seconds
output_paths = reframer.analyze_and_render(
    input_path="input.mp4",
    output_prefix="output",
    time_range=(10.0, 40.0)
)
```

### Multiple Aspect Ratios

```python
reframer = Reframer(
    target_aspect_ratios=[
        AspectRatio(9, 16),  # TikTok/Reels
        AspectRatio(4, 5),   # Instagram Feed
        AspectRatio(1, 1),   # Square
    ]
)

output_paths = reframer.analyze_and_render(
    input_path="input.mp4",
    output_prefix="output"
)
# {"9:16": "output_9x16.mp4", "4:5": "output_4x5.mp4", "1:1": "output_1x1.mp4"}
```

## CLI Usage

### Basic

```bash
python -m app.core.smart_reframe.cli --input video.mp4 --aspect 9:16
```

### With Options

```bash
python -m app.core.smart_reframe.cli \
  --input video.mp4 \
  --aspect 9:16 \
  --aspect 4:5 \
  --output-prefix output \
  --time-start 10 \
  --time-end 60 \
  --preset tiktok \
  --dump-crop-plan cropplan.json
```

### CLI Options

| Option                | Description                                                  |
| --------------------- | ------------------------------------------------------------ |
| `--input, -i`         | Input video file (required)                                  |
| `--output-prefix, -o` | Prefix for output files                                      |
| `--aspect, -a`        | Target aspect ratio (can be repeated)                        |
| `--time-start, -ss`   | Start time in seconds or HH:MM:SS                            |
| `--time-end, -to`     | End time in seconds or HH:MM:SS                              |
| `--preset`            | Configuration preset: `default`, `fast`, `quality`, `tiktok` |
| `--fps-sample`        | Analysis sample rate in FPS                                  |
| `--max-pan-speed`     | Maximum virtual camera speed (pixels/second)                 |
| `--detector`          | Detection backend: `mediapipe` or `yolo`                     |
| `--dump-crop-plan`    | Save crop plan to JSON file                                  |
| `--load-crop-plan`    | Load crop plan from JSON file                                |

## Configuration

### IntelligentCropConfig

```python
from app.core.smart_reframe import IntelligentCropConfig

config = IntelligentCropConfig(
    # Analysis settings
    fps_sample=3.0,           # Frames per second to analyze
    analysis_resolution=480,   # Height for analysis (lower = faster)

    # Shot detection
    shot_threshold=0.4,        # Sensitivity for scene change detection
    min_shot_duration=0.5,     # Minimum shot length in seconds

    # Face detection
    detector_backend="mediapipe",  # or "yolo" (optional)
    min_detection_confidence=0.5,
    min_face_size=0.02,        # Min face size as fraction of frame

    # Composition
    headroom_ratio=0.15,       # Target headroom above head
    subject_padding=0.2,       # Padding around subject
    safe_margin=0.05,          # Min margin from crop edge

    # Camera smoothing
    max_pan_speed=200.0,       # Max virtual camera speed
    smoothing_window=0.5,      # Smoothing window in seconds

    # Zoom limits
    max_zoom_factor=3.0,       # Maximum zoom
    min_zoom_factor=1.0,       # Minimum zoom

    # Fallback behavior
    fallback_policy="upper_center",  # When no faces detected

    # Rendering
    render_preset="veryfast",  # FFmpeg preset
    render_crf=20,             # Quality (lower = better)
)
```

### Preset Configurations

```python
from app.core.smart_reframe.config import FAST_CONFIG, QUALITY_CONFIG, TIKTOK_CONFIG

# Fast processing for previews
reframer = Reframer(config=FAST_CONFIG)

# High quality for final renders
reframer = Reframer(config=QUALITY_CONFIG)

# Optimized for TikTok-style content
reframer = Reframer(config=TIKTOK_CONFIG)
```

## Integration with ViralClipAI

The intelligent crop mode is integrated into the existing clipper:

```python
from app.core.clipper import run_ffmpeg_clip_with_crop

# Use intelligent cropping
run_ffmpeg_clip_with_crop(
    start_str="00:00:10",
    end_str="00:00:40",
    out_path=Path("output.mp4"),
    style="split",  # Ignored when crop_mode="intelligent"
    video_file=Path("source.mp4"),
    crop_mode="intelligent",
    target_aspect="9:16",
)
```

## Architecture

### Pipeline Overview

```
Input Video
    │
    ▼
┌─────────────────┐
│ Shot Detection  │  ← Histogram-based scene detection
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│Content Analysis │  ← MediaPipe face detection + tracking
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Camera Planning │  ← Motion smoothing + constraints
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Crop Planning   │  ← Aspect ratio fitting + composition
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Rendering     │  ← FFmpeg with dynamic cropping
└────────┬────────┘
         │
         ▼
Output Video(s)
```

### Module Structure

```
app/core/smart_reframe/
├── __init__.py          # Public API exports
├── models.py            # Pydantic data models
├── config.py            # Configuration options
├── shot_detector.py     # Scene change detection
├── content_analyzer.py  # Face/person detection
├── saliency.py          # Fallback saliency estimation
├── smoother.py          # Camera path smoothing
├── crop_planner.py      # Crop window computation
├── renderer.py          # FFmpeg rendering
├── reframer.py          # Main orchestration class
└── cli.py               # Command-line interface
```

### Data Flow

1. **Shot Detection** → `list[Shot]`
2. **Content Analysis** → `list[ShotDetections]`
3. **Camera Planning** → `list[ShotCameraPlan]`
4. **Crop Planning** → `list[ShotCropPlan]`
5. **All Combined** → `CropPlan` (serializable to JSON)
6. **Rendering** → Output video files

## Extending

### Adding a New Detection Backend

```python
# In content_analyzer.py, add to _detect_faces method:
if self.config.detector_backend == DetectorBackend.CUSTOM:
    return self._detect_faces_custom(frame, scale, orig_width, orig_height)
```

### Re-rendering at Different Resolutions

```python
# Load existing crop plan
crop_plan = CropPlan.from_json_file("cropplan.json")

# Render at 1080x1920
reframer.render(
    input_path="input.mp4",
    crop_plan=crop_plan,
    output_prefix="output_1080",
    output_resolution=(1080, 1920)
)

# Render at 720x1280
reframer.render(
    input_path="input.mp4",
    crop_plan=crop_plan,
    output_prefix="output_720",
    output_resolution=(720, 1280)
)
```

### Using YOLO Backend (Optional)

```bash
pip install ultralytics
```

```python
config = IntelligentCropConfig(
    detector_backend="yolo"
)
```

## Performance

### Typical Processing Times (CPU-only)

| Video Duration | Analysis Time | Render Time | Total |
| -------------- | ------------- | ----------- | ----- |
| 30 seconds     | ~5s           | ~10s        | ~15s  |
| 1 minute       | ~10s          | ~20s        | ~30s  |
| 5 minutes      | ~45s          | ~90s        | ~2.5m |

### Memory Usage

- Analysis: ~200-400 MB (depending on resolution)
- MediaPipe model: ~50 MB
- Peak during rendering: ~500 MB

### Optimization Tips

1. Lower `analysis_resolution` for faster processing
2. Reduce `fps_sample` (2-3 fps is usually sufficient)
3. Use `render_preset="ultrafast"` for previews
4. Use `render_preset="slow"` only for final exports

## Troubleshooting

### No Faces Detected

The system falls back to saliency-based or center-biased cropping when no faces are detected. Configure the fallback behavior:

```python
config = IntelligentCropConfig(
    fallback_policy="upper_center"  # Good for talking-head videos
)
```

### Jerky Camera Motion

Increase smoothing:

```python
config = IntelligentCropConfig(
    smoothing_window=1.0,     # Longer smoothing
    max_pan_speed=150,        # Slower max speed
)
```

### Over-Cropping

Reduce zoom and increase margins:

```python
config = IntelligentCropConfig(
    max_zoom_factor=2.0,      # Less zoom
    subject_padding=0.3,      # More padding
)
```
