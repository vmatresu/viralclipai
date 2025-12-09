# Styles and Crop Modes Explained

## Overview

The video processing system uses two independent concepts:

- **Style**: Determines the visual layout/composition of the output video
- **Crop Mode**: Determines how the video is cropped (if at all)
- **Detection Tier**: Controls which AI detection providers are used (for intelligent styles)

---

## Detection Tiers (Clean 4)

| Tier           | Providers                         | Speed       | Description                                             |
| -------------- | --------------------------------- | ----------- | ------------------------------------------------------- |
| `None`         | —                                 | ⚡ Fastest  | Static/heuristic only                                   |
| `MotionAware`  | Heuristic motion (frame diff)     | 🏃 Active   | NN-free center-of-motion heuristic, no faces, no audio |
| `Basic`        | YuNet faces                       | 🧠 Standard | Face detection for subject tracking                     |
| `SpeakerAware` | YuNet + FaceMesh (visual-only)    | 🎯 Premium  | Mouth-open activity (MAR) based speaker following       |

---

## All Available Styles (12 Total)

### Static/Fast Styles (No AI)

#### `split`

- **Detection Tier**: None
- **Output**: 1080x1920 portrait
- **Description**: Static split-screen showing left/right halves stacked vertically
- **Use case**: Videos with two people or focal points

#### `split_fast`

- **Detection Tier**: None
- **Output**: 1080x1920 portrait
- **Description**: Heuristic-only split (45% from each side, no overlap)
- **Use case**: Fast processing when AI detection not needed

#### `left_focus`

- **Detection Tier**: None
- **Output**: 1080x1920 portrait
- **Description**: Crops left portion of landscape video
- **Use case**: Main subject on left side

#### `center_focus`

- **Detection Tier**: None
- **Output**: 1080x1920 portrait
- **Description**: Crops the centered 9:16 slice of a landscape video
- **Use case**: Main subject near the middle; deterministic framing

#### `right_focus`

- **Detection Tier**: None
- **Output**: 1080x1920 portrait
- **Description**: Crops right portion of landscape video
- **Use case**: Main subject on right side

#### `original`

- **Detection Tier**: None
- **Output**: Same as input
- **Description**: No processing, preserves original format
- **Use case**: Keep original aspect ratio

---

### Intelligent Single-View Styles

#### `intelligent`

- **Detection Tier**: Basic (YuNet)
- **Output**: 9:16 portrait
- **Description**: AI face tracking with dynamic crop window
- **Use case**: Videos with moving subjects

#### `intelligent_motion`

- **Detection Tier**: MotionAware (heuristic motion)
- **Output**: 9:16 portrait
- **Description**: NN-free motion-following using frame differencing
- **Use case**: High-motion clips, gaming/sports

#### `intelligent_speaker`

- **Detection Tier**: SpeakerAware (visual-only)
- **Output**: 9:16 portrait
- **Description**: YuNet + FaceMesh mouth activity (no audio)
- **Use case**: Premium speaker tracking for podcasts/interviews

---

### Intelligent Split-View Styles

#### `intelligent_split`

- **Detection Tier**: Basic (YuNet)
- **Output**: 1080x1920 portrait (stacked halves)
- **Description**: Split view with face-centered crop on each half
- **Use case**: Podcast-style dual subjects

#### `intelligent_split_motion`

- **Detection Tier**: MotionAware (heuristic motion)
- **Output**: 1080x1920 portrait
- **Description**: Split view steered by motion centers (NN-free)
- **Use case**: High-motion, dual-subject clips

#### `intelligent_split_speaker`

- **Detection Tier**: SpeakerAware (visual-only)
- **Output**: 1080x1920 portrait
- **Description**: Split view with FaceMesh mouth activity; left=top, right=bottom invariant
- **Use case**: Premium dual-subject podcasts

---

### Special

#### `all`

- **Description**: Generates multiple styles at once
- **Expands to**: split, split_fast, left_focus, center_focus, right_focus, intelligent, intelligent_split

### Deleted / Legacy

- `intelligent_basic`, `intelligent_split_basic`, `intelligent_activity`, `intelligent_split_activity` are removed in Clean 4.

---

## Frontend Style Selector

The UI displays styles in a 4-column grid with speed indicators:

| Indicator       | Meaning              |
| --------------- | -------------------- |
| ⚡ Fast/Fastest | No AI detection      |
| 🧠 Standard     | Basic face detection |
| 🎯 Premium      | Full detection stack |

---

## Style → Backend Mapping

| Frontend Value              | Backend Style                    | Detection Tier |
| --------------------------- | -------------------------------- | -------------- |
| `split`                     | `Style::Split`                   | None           |
| `split_fast`                | `Style::SplitFast`               | None           |
| `left_focus`                | `Style::LeftFocus`               | None           |
| `center_focus`              | `Style::CenterFocus`             | None           |
| `right_focus`               | `Style::RightFocus`              | None           |
| `original`                  | `Style::Original`                | None           |
| `intelligent`               | `Style::Intelligent`             | Basic          |
| `intelligent_motion`        | `Style::IntelligentMotion`       | MotionAware    |
| `intelligent_speaker`       | `Style::IntelligentSpeaker`      | SpeakerAware   |
| `intelligent_split`         | `Style::IntelligentSplit`        | Basic          |
| `intelligent_split_motion`  | `Style::IntelligentSplitMotion`  | MotionAware    |
| `intelligent_split_speaker` | `Style::IntelligentSplitSpeaker` | SpeakerAware   |

---

## Implementation Status

| Feature                                | Status                                       |
| -------------------------------------- | -------------------------------------------- |
| Static styles                          | ✅ Implemented                               |
| `SplitFast` with `FastSplitEngine`     | ✅ Implemented                               |
| Intelligent styles (Basic tier)        | ✅ Implemented                               |
| Intelligent styles (Motion tier)       | ✅ NN-free motion heuristic wired            |
| Intelligent styles (SpeakerAware tier) | ✅ Visual-only FaceMesh mouth activity       |
| Detection pipeline module              | ✅ Implemented                               |
| Tier-aware `StyleProcessorFactory`     | ✅ Implemented (Clean 4 tiers)               |

---

## Recommendations

1. **For speed**: Use `split_fast` for quickest processing
2. **For motion-heavy**: Use `intelligent_motion` or `intelligent_split_motion`
3. **For quality**: Use `intelligent_speaker` / `intelligent_split_speaker` (visual-only)
4. **For podcasts**: `intelligent_split` for speed, `intelligent_split_speaker` for quality
5. **For all variations**: Use `all` keyword
