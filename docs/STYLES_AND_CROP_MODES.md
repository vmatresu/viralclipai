# Styles and Crop Modes Explained

## Overview

The video processing system uses two independent concepts:

- **Style**: Determines the visual layout/composition of the output video
- **Crop Mode**: Determines how the video is cropped (if at all)
- **Detection Tier**: Controls which AI detection providers are used (for intelligent styles)

---

## Detection Tiers

Intelligent styles use progressive detection tiers that control quality vs speed:

| Tier | Providers | Speed | Description |
|------|-----------|-------|-------------|
| `None` | — | ⚡ Fastest | Heuristic positioning only |
| `Basic` | YuNet faces | 🧠 Standard | Face detection for subject tracking |
| `AudioAware` | YuNet + Audio | 🎧 Enhanced | Face detection + speaker audio activity |
| `SpeakerAware` | YuNet + Audio + Activity | 🎯 Premium | Full detection with mouth movement analysis |

---

## All Available Styles (13 Total)

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

#### `intelligent` / `intelligent_basic`
- **Detection Tier**: Basic (YuNet)
- **Output**: 9:16 portrait
- **Description**: AI face tracking with dynamic crop window
- **Use case**: Videos with moving subjects

#### `intelligent_audio`
- **Detection Tier**: AudioAware (YuNet + Speaker Detection)
- **Output**: 9:16 portrait
- **Description**: Face tracking biased toward active speaker
- **Use case**: Podcast/interview format

#### `intelligent_speaker`
- **Detection Tier**: SpeakerAware (Full stack)
- **Output**: 9:16 portrait
- **Description**: Face + audio + mouth movement analysis
- **Use case**: Highest quality speaker tracking

---

### Intelligent Split-View Styles

#### `intelligent_split` / `intelligent_split_basic`
- **Detection Tier**: Basic (YuNet)
- **Output**: 1080x1920 portrait (stacked halves)
- **Description**: Split view with face-centered crop on each half
- **Use case**: Podcast-style dual subjects

#### `intelligent_split_audio`
- **Detection Tier**: AudioAware
- **Output**: 1080x1920 portrait
- **Description**: Split view with speaker-aware emphasis
- **Use case**: Enhanced podcast format

#### `intelligent_split_speaker`
- **Detection Tier**: SpeakerAware
- **Output**: 1080x1920 portrait
- **Description**: Split view with full speaker detection
- **Use case**: Premium dual-subject videos

---

### Special

#### `all`
- **Description**: Generates multiple styles at once
- **Expands to**: split, split_fast, left_focus, right_focus, intelligent, intelligent_split

---

## Frontend Style Selector

The UI displays styles in a 4-column grid with speed indicators:

| Indicator | Meaning |
|-----------|---------|
| ⚡ Fast/Fastest | No AI detection |
| 🧠 Standard | Basic face detection |
| 🎧 Enhanced | Face + audio detection |
| 🎯 Premium | Full detection stack |

---

## Style → Backend Mapping

| Frontend Value | Backend Style | Detection Tier |
|----------------|---------------|----------------|
| `split` | `Style::Split` | None |
| `split_fast` | `Style::SplitFast` | None |
| `left_focus` | `Style::LeftFocus` | None |
| `right_focus` | `Style::RightFocus` | None |
| `original` | `Style::Original` | None |
| `intelligent` | `Style::Intelligent` | Basic |
| `intelligent_audio` | `Style::IntelligentAudio` | AudioAware |
| `intelligent_speaker` | `Style::IntelligentSpeaker` | SpeakerAware |
| `intelligent_split` | `Style::IntelligentSplit` | Basic |
| `intelligent_split_audio` | `Style::IntelligentSplitAudio` | AudioAware |
| `intelligent_split_speaker` | `Style::IntelligentSplitSpeaker` | SpeakerAware |

---

## Implementation Status

| Feature | Status |
|---------|--------|
| Static styles | ✅ Implemented |
| `SplitFast` with `FastSplitEngine` | ✅ Implemented |
| Intelligent styles (Basic tier) | ✅ Implemented |
| Intelligent styles (AudioAware tier) | ✅ Wired to `SpeakerDetector` |
| Intelligent styles (SpeakerAware tier) | ✅ Wired to `FaceActivityAnalyzer` |
| Detection pipeline module | ✅ Implemented |
| Tier-aware `StyleProcessorFactory` | ✅ Implemented |

---

## Recommendations

1. **For speed**: Use `split_fast` for quickest processing
2. **For quality**: Use `intelligent_speaker` for best speaker tracking
3. **For podcasts**: Use `intelligent_split_audio` as a good balance
4. **For all variations**: Use `all` keyword
