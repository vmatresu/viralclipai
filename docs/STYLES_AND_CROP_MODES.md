# Styles and Crop Modes Explained

## Overview

The video processing system uses two independent concepts:

- **Style**: Determines the visual layout/composition of the output video
- **Crop Mode**: Determines how the video is cropped (if at all)

These can be combined independently to create different output variations.

---

## All Available Styles

### 1. `split`

- **Description**: Creates a split-screen view showing both sides of a landscape video
- **Output**: Portrait format (1080x1920)
- **How it works**:
  - Splits the video into left and right halves
  - Stacks them vertically (top/bottom)
  - Each half is cropped to 1080x960, then stacked
- **Use case**: Good for videos with two people or two focal points

### 2. `left_focus`

- **Description**: Focuses on the left side of the video
- **Output**: Portrait format (1080x1920)
- **How it works**:
  - Crops the left 910 pixels from a 1920px wide video
  - Scales to portrait (1080x1920) with padding if needed
- **Use case**: When the main subject is on the left side

### 3. `right_focus`

- **Description**: Focuses on the right side of the video
- **Output**: Portrait format (1080x1920)
- **How it works**:
  - Crops the right side starting at pixel 960 from a 1920px wide video
  - Scales to portrait (1080x1920) with padding if needed
- **Use case**: When the main subject is on the right side

### 4. `intelligent`

- **Description**: AI-powered smart cropping with face/subject tracking
- **Output**: Portrait format (default 9:16, configurable)
- **How it works**:
  - Uses computer vision to detect faces and subjects
  - Dynamically adjusts crop window to keep subjects centered
  - Supports multiple aspect ratios (9:16, 4:5, 1:1, etc.)
- **Use case**: Best for videos with people or moving subjects

### 5. `intelligent_split`

- **Description**: AI-powered smart cropping specifically for 9:16 portrait format
- **Output**: Portrait format (9:16)
- **How it works**:
  - Uses intelligent cropping (same as `intelligent` style)
  - Always outputs 9:16 aspect ratio
  - Uses face/subject tracking to dynamically adjust crop window
- **Use case**: When you want intelligent cropping with guaranteed 9:16 output (optimized for TikTok, Instagram Reels, YouTube Shorts)

### 6. `original`

- **Description**: Preserves the original video format without any cropping
- **Output**: Same as input (landscape stays landscape, portrait stays portrait)
- **How it works**:
  - No video filters applied
  - Original aspect ratio and resolution preserved
- **Use case**: When you want to keep the original format

### 7. `all`

- **Description**: Special keyword that generates all available styles
- **Output**: Multiple files, one for each style
- **Use case**: When you want to generate all variations at once

---

## All Available Crop Modes

### 1. `none` (Default)

- **Description**: No additional cropping beyond what the style specifies
- **Behavior**:
  - Uses the style's built-in cropping logic
  - For `split`, `left_focus`, `right_focus`: applies their specific crops
  - For `original`: no cropping at all
  - For `intelligent`: ignored (intelligent style uses its own cropping)

### 2. `center`

- **Description**: Center-based cropping (currently not fully implemented)
- **Status**: Defined but not yet implemented in the codebase
- **Planned behavior**: Would crop from the center of the frame

### 3. `manual`

- **Description**: Manual cropping with user-specified coordinates (currently not implemented)
- **Status**: Defined but not yet implemented in the codebase
- **Planned behavior**: Would allow users to specify exact crop coordinates

### 4. `intelligent`

- **Description**: AI-powered intelligent cropping
- **Behavior**:
  - Uses face detection and subject tracking
  - Dynamically adjusts crop window frame-by-frame
  - Requires `target_aspect` parameter (default: "9:16")
  - Supports multiple aspect ratios: "9:16", "4:5", "1:1", "16:9", etc.
  - Ignores the `style` parameter when active

---

## How Style and Crop Mode Interact

The system processes them in this priority order:

```
1. If style == "original":
   → Always preserves original format (crop_mode is ignored)

2. Else if crop_mode == "intelligent":
   → Uses intelligent cropping (style is ignored, except "intelligent" style)
   → Requires target_aspect parameter

3. Else:
   → Uses traditional style-based cropping
   → crop_mode "none", "center", "manual" currently all behave the same
```

### Web Interface Behavior

**Important**: The web interface currently only allows selecting **styles**, not crop modes. When you select a style:

- **No `crop_mode` is sent** from the web interface
- The backend **defaults to `crop_mode="none"`**
- Each style has **built-in cropping logic** that is automatically applied

**Example: When you select "Split View" style:**

1. Web sends: `styles: ["split"]` (no `crop_mode` field)
2. Backend defaults: `crop_mode="none"`
3. System uses: `build_vf_filter("split")` which returns the split-screen FFmpeg filter
4. Result: The split-screen cropping is **built into the style itself**

The cropping for each style is **hardcoded** in the `build_vf_filter()` function:

- `split`: Splits video into left/right halves and stacks them vertically
- `left_focus`: Crops left 910px from 1920px wide video
- `right_focus`: Crops right side starting at pixel 960
- `original`: No cropping (returns `None`)
- `intelligent`: Uses AI-powered cropping (requires intelligent crop mode)
- `intelligent_split`: Uses AI-powered cropping with guaranteed 9:16 output (requires intelligent crop mode)

### Decision Flow

```python
if style == "original":
    # Preserve original format, ignore crop_mode
    use_original_format()

elif style in ["intelligent", "intelligent_split"] or crop_mode == "intelligent":
    # Use intelligent cropping
    # intelligent_split always uses 9:16 aspect ratio
    target_aspect = "9:16" if style == "intelligent_split" else target_aspect
    use_intelligent_crop(target_aspect)

else:
    # Use style-based cropping
    use_style_cropping(style)
```

---

## Examples of Style + Crop Mode Combinations

### Example 1: `style="split"` + `crop_mode="none"`

- **Result**: Split-screen view (left/right stacked vertically)
- **Output**: 1080x1920 portrait video

### Example 2: `style="left_focus"` + `crop_mode="intelligent"`

- **Result**: Intelligent cropping is used (style is ignored)
- **Output**: 9:16 portrait with AI tracking

### Example 3: `style="original"` + `crop_mode="intelligent"`

- **Result**: Original format preserved (crop_mode is ignored)
- **Output**: Same as input (e.g., 1920x1080 landscape stays landscape)

### Example 4: `style="intelligent"` + `crop_mode="none"`

- **Result**: Intelligent cropping is used (style overrides crop_mode)
- **Output**: 9:16 portrait with AI tracking

### Example 5: `style="split"` + `crop_mode="intelligent"`

- **Result**: Intelligent cropping is used (crop_mode overrides style)
- **Output**: 9:16 portrait with AI tracking (split style is ignored)

### Example 6: `style="intelligent_split"` + `crop_mode="none"`

- **Result**: Intelligent cropping is used (style forces intelligent crop mode)
- **Output**: 9:16 portrait with AI tracking (guaranteed 9:16 aspect ratio)

---

## Current Implementation Status

| Feature                     | Status                         | Notes                                       |
| --------------------------- | ------------------------------ | ------------------------------------------- |
| `style="split"`             | ✅ Implemented                 | Works with crop_mode="none"                 |
| `style="left_focus"`        | ✅ Implemented                 | Works with crop_mode="none"                 |
| `style="right_focus"`       | ✅ Implemented                 | Works with crop_mode="none"                 |
| `style="intelligent"`       | ✅ Implemented                 | Forces crop_mode="intelligent"              |
| `style="intelligent_split"` | ✅ Implemented                 | Forces crop_mode="intelligent", 9:16 output |
| `style="original"`          | ✅ Implemented                 | Ignores crop_mode                           |
| `crop_mode="none"`          | ✅ Implemented                 | Default behavior                            |
| `crop_mode="intelligent"`   | ✅ Implemented                 | Requires target_aspect                      |
| `crop_mode="center"`        | ⚠️ Defined but not implemented | Currently behaves like "none"               |
| `crop_mode="manual"`        | ⚠️ Defined but not implemented | Currently behaves like "none"               |

---

## Recommendations

1. **For portrait videos**: Use `style="intelligent"` or `crop_mode="intelligent"` with `target_aspect="9:16"`
2. **For square videos**: Use `crop_mode="intelligent"` with `target_aspect="1:1"`
3. **For original format**: Use `style="original"` (crop_mode is ignored)
4. **For split-screen**: Use `style="split"` with `crop_mode="none"`
5. **For side-focused**: Use `style="left_focus"` or `style="right_focus"` with `crop_mode="none"`

---

## Future Enhancements

- Implement `crop_mode="center"` for center-biased cropping
- Implement `crop_mode="manual"` for user-specified crop coordinates
- Allow combining traditional styles with intelligent cropping
- Support more aspect ratios and custom resolutions
