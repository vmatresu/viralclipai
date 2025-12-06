# Rust Style Processing Architecture

## Overview

This document explains how video clip styles are processed in the Rust refactor and how it maps to the original Python implementation.

## Architecture Pattern: Style Routing

### Python Implementation (Reference)

**File**: `python-backup/app/core/clipper.py`

The Python implementation used a unified entry point:

```python
def run_ffmpeg_clip_with_crop(
    start_str: str,
    end_str: str,
    out_path: Path,
    style: str,
    video_file: Path,
    crop_mode: str = "none",
    target_aspect: str = "9:16",
    ...
):
    # Line 729-772: Style routing logic
    if style == "original":
        run_ffmpeg_clip(...)
    elif style == "intelligent_split":
        run_intelligent_split_crop(...)  # Multi-step pipeline
    elif crop_mode == "intelligent":
        run_intelligent_crop(...)
    else:
        run_ffmpeg_clip(...)  # Traditional styles
```

### Rust Implementation

**Files**:

- `backend/crates/vclip-worker/src/processor.rs` (routing logic)
- `backend/crates/vclip-media/src/clip.rs` (clip creation)

The Rust implementation splits this into two layers:

#### Layer 1: Processor (Routing)

```rust
// processor.rs: process_clip_task()
if task.style == Style::IntelligentSplit {
    create_intelligent_split_clip(...)
} else {
    create_clip(...)
}
```

#### Layer 2: Media Library (Execution)

```rust
// clip.rs: create_clip()
match (task.style, task.crop_mode) {
    (Style::Original, _) => create_basic_clip(...),
    (Style::IntelligentSplit, _) => Err(...), // Should never reach here
    (_, CropMode::Intelligent) => Err(...),   // Not yet implemented
    _ => create_basic_clip(...)                // Traditional styles
}
```

## Style Processing Details

### Traditional Styles (Split, LeftFocus, RightFocus)

**Processing**: Single-pass FFmpeg with video filters

**Flow**:

1. Processor calls `create_clip()`
2. `create_clip()` builds FFmpeg filter via `build_video_filter()`
3. `create_basic_clip()` executes FFmpeg command
4. Thumbnail generated

**FFmpeg Filters** (defined in `filters.rs`):

- **Split**: `scale=1920:-2,split=2[full][full2];[full]crop=910:1080:0:0[left];[full2]crop=960:1080:960:0[right];[left]scale=1080:-2,crop=1080:960[left_scaled];[right]scale=1080:-2,crop=1080:960[right_scaled];[left_scaled][right_scaled]vstack=inputs=2`
- **LeftFocus**: `scale=1920:-2,crop=910:1080:0:0,scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2`
- **RightFocus**: `scale=1920:-2,crop=960:1080:960:0,scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2`

### IntelligentSplit Style

**Processing**: Multi-step pipeline with temporary files

**Flow**:

1. Processor detects `Style::IntelligentSplit`
2. Processor calls `create_intelligent_split_clip()`
3. Multi-step pipeline:
   ```
   a. Extract left half:  crop=iw/2:ih:0:0
   b. Extract right half: crop=iw/2:ih:iw/2:0
   c. Scale left half to 1080x960 (9:8 aspect)
   d. Scale right half to 1080x960 (9:8 aspect)
   e. Stack vertically: [left][right]vstack
   ```
4. Thumbnail generated

**Why Multi-Step?**

- Future ML integration: Each half will be processed through face-tracking ML model
- Currently uses placeholder scaling (lines 186-204 in `clip.rs`)
- Matches Python's `run_intelligent_split_crop()` architecture (lines 526-630)

### Original Style

**Processing**: No filters, preserves original format

**Flow**:

1. Processor calls `create_clip()`
2. `create_clip()` calls `create_basic_clip()` with `filter=None`
3. FFmpeg copies video segment without transformation
4. Thumbnail generated

## Error Handling

### Design Philosophy

**Fail Fast at the Right Layer**:

- Processor layer: Route to correct function (business logic)
- Media layer: Validate inputs and execute (technical logic)

### Error Cases

1. **IntelligentSplit reaches `create_clip()`**:

   - Error: "IntelligentSplit must be processed using create_intelligent_split_clip - this is a caller error"
   - Why: This indicates a bug in the processor routing logic
   - Fix: Update processor to route correctly

2. **Intelligent CropMode**:
   - Error: "Intelligent crop mode requires ML client integration (not yet implemented)"
   - Why: ML integration not yet complete in Rust
   - Future: Will integrate face-tracking ML model

## Comparison: Python vs Rust

| Aspect                 | Python                                        | Rust                                           |
| ---------------------- | --------------------------------------------- | ---------------------------------------------- |
| **Entry Point**        | Single function `run_ffmpeg_clip_with_crop()` | Two-layer: processor routing + media execution |
| **Style Routing**      | If/elif chain in one function                 | Pattern matching in processor                  |
| **IntelligentSplit**   | `run_intelligent_split_crop()`                | `create_intelligent_split_clip()`              |
| **Traditional Styles** | `run_ffmpeg_clip()`                           | `create_clip()` → `create_basic_clip()`        |
| **Filter Building**    | `build_vf_filter()`                           | `build_video_filter()`                         |
| **Error Handling**     | RuntimeError with string messages             | Typed errors with MediaError enum              |

## Code Locations

### Rust Implementation

```
backend/crates/
├── vclip-worker/src/
│   └── processor.rs          # Style routing (lines 300-313)
├── vclip-media/src/
│   ├── clip.rs               # Clip creation (lines 46-244)
│   ├── filters.rs            # FFmpeg filters (lines 37-45)
│   └── command.rs            # FFmpeg command builder
└── vclip-models/src/
    └── style.rs              # Style enum and parsing (lines 10-85)
```

### Python Reference

```
python-backup/app/core/
└── clipper.py
    ├── run_ffmpeg_clip_with_crop()      # Lines 699-772 (unified entry)
    ├── run_intelligent_split_crop()     # Lines 526-630 (multi-step)
    ├── run_intelligent_crop()           # Lines 303-398 (ML integration)
    └── build_vf_filter()                # Lines 214-244 (filter building)
```

## Testing the Fix

### Before Fix

```
Error: Media error: Unsupported format: IntelligentSplit should use create_intelligent_split_clip
```

### After Fix

```
[03:47:42] > Processing clip 2/16: clip_01_..._intelligent_split.mp4
[03:47:42] > Creating clip: ... (style: intelligent_split, crop: none)
[Success] Clip created and uploaded
```

### Verification Steps

1. **Check routing**: Processor detects `IntelligentSplit` and calls correct function
2. **Check execution**: `create_intelligent_split_clip()` runs multi-step pipeline
3. **Check output**: Clip has stacked halves (1080x1920 portrait)
4. **Check thumbnail**: Thumbnail generated successfully

## Future Enhancements

### ML Integration for IntelligentSplit

**Current** (lines 180-204 in `clip.rs`):

```rust
// Placeholder: Scale each half to 9:8 aspect
let scale_filter = "scale=1080:960:force_original_aspect_ratio=decrease,pad=1080:960:(ow-iw)/2:(oh-ih)/2";
```

**Future**:

```rust
// Call ML service for face-tracking crop
let left_cropped = ml_client.intelligent_crop(left_half, AspectRatio::SPLIT_VIEW).await?;
let right_cropped = ml_client.intelligent_crop(right_half, AspectRatio::SPLIT_VIEW).await?;
```

### Intelligent CropMode

**Status**: Defined but not implemented

**Future**: Will support intelligent cropping for any style:

```rust
if task.crop_mode == CropMode::Intelligent {
    // Apply ML-based crop regardless of style
    ml_client.intelligent_crop(video, task.target_aspect).await?
}
```

## Best Practices

### Adding New Styles

1. **Define enum** in `vclip-models/src/style.rs`
2. **Add filter** in `vclip-media/src/filters.rs` (if single-pass)
3. **Update routing** in `vclip-worker/src/processor.rs` (if multi-step)
4. **Add tests** for the new style
5. **Update docs** in `STYLES_AND_CROP_MODES.md`

### Modifying Existing Styles

1. **Traditional styles**: Update filter in `filters.rs`
2. **IntelligentSplit**: Update pipeline in `clip.rs::create_intelligent_split_clip()`
3. **Run integration tests** to verify no regressions
4. **Update documentation** if behavior changes

## Summary

The Rust refactor correctly implements the Python style processing logic with improved:

- **Type safety**: Enum-based styles vs string matching
- **Separation of concerns**: Routing vs execution
- **Error handling**: Typed errors vs generic RuntimeError
- **Modularity**: Clear boundaries between layers

The fix ensures `IntelligentSplit` is routed correctly at the processor layer, matching the Python implementation's `run_ffmpeg_clip_with_crop()` logic.
