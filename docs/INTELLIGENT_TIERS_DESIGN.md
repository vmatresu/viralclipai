# Intelligent Tiers Design Document

## Overview

The intelligent pipeline is now fully visual-only (audio removed due to unreliable mono/duplicated inputs) and organized into the **Clean 4** tiers. Motion tier is NN-free; SpeakerAware uses FaceMesh mouth activity (MAR) only.

## Tier Definitions

### DetectionTier::None (Static/SplitFast)
- **Detection**: Heuristic positioning only  
- **Camera behavior**: Fixed or center-weighted positioning  
- **Use case**: Fast, deterministic

### DetectionTier::MotionAware (IntelligentMotion, IntelligentSplitMotion)
- **Detection**: NN-free heuristic motion (frame differencing)  
- **Camera behavior**: Center-of-motion targeting; no faces, no audio  
- **Use case**: High-motion clips (gaming/sports)

### DetectionTier::Basic (Intelligent, IntelligentSplit)
- **Detection**: YuNet face detection  
- **Camera behavior**: Follow the most prominent face (largest × confidence)  
- **Use case**: General talking-heads

### DetectionTier::SpeakerAware (IntelligentSpeaker, IntelligentSplitSpeaker)
- **Detection**: YuNet + FaceMesh (mouth MAR), visual-only  
- **Camera behavior**: Mouth-activity-driven speaker tracking with hysteresis  
- **Switching**: Dwell ≥ 1.0s, margin 20%  
- **Use case**: Podcasts/interviews, visual active face

## Implementation

### TierAwareCameraSmoother (`tier_aware_smoother.rs`)
- **Basic**: `compute_focus_basic()` - largest face × confidence  
- **MotionAware**: Motion-only tracks (NN-free) with snaps/hysteresis  
- **SpeakerAware**: `compute_focus_speaker_aware()` - mouth MAR with hysteresis, no audio

### TierAwareIntelligentCropper (`tier_aware_cropper.rs`)
1. Face detection (Basic/SpeakerAware) or motion detection (MotionAware, NN-free)  
2. Tier-aware camera smoothing  
3. Crop planning and rendering

### TierAwareSplitProcessor (`tier_aware_split.rs`)
- **Basic**: Fixed positioning (0% left, 15% right)  
- **SpeakerAware**: Visual-only mouth activity; leftmost → top, rightmost → bottom (hard invariant)  
- **MotionAware**: Motion-guided split using NN-free motion tracks

### Integration Points
- **IntelligentProcessor** (`styles/intelligent.rs`): `create_tier_aware_intelligent_clip()` for None/Basic/MotionAware/SpeakerAware.  
- **IntelligentSplitProcessor** (`styles/intelligent_split.rs`): `create_tier_aware_split_clip()` for None/Basic/MotionAware/SpeakerAware.

## Camera Behavior Specs

### Basic
- Target: Largest face × confidence  
- Smoothing: Moving average ~0.3s  
- Max pan: ~600 px/s  
- No speaker awareness

### MotionAware
- Target: Center-of-motion heuristic  
- Hysteresis: Minimum segment duration to avoid flapping  
- Fallback: Centered box if no motion

### SpeakerAware (Visual-Only)
- Target: Face with highest mouth MAR  
- Hysteresis: Dwell 1.0s, margin 20%  
- Fallback: Basic behavior if unclear

## File Changes Summary
- `intelligent/mod.rs` - Tier-aware exports; motion detector  
- `styles/intelligent.rs` / `styles/intelligent_split.rs` - Tier-aware processors  
- `intelligent/tier_aware_cropper.rs` - Tier-aware orchestration (motion path NN-free)  
- `intelligent/tier_aware_smoother.rs` - Visual-only smoothing logic  
- `intelligent/tier_aware_split.rs` - Split invariant (left→top, right→bottom)  
- `intelligent/tests.rs` - Visual-only tests

## Preserved Fixes
1. A/V sync: Two-pass seeking in `extract_segment()`  
2. Padding: `pad_before_seconds`, `pad_after_seconds`  
3. Split framing: SpeakerAware invariant + mild hysteresis  
4. Camera responsiveness: 600 px/s max pan speed, ~0.3s smoothing (Basic/SpeakerAware)
