# Intelligent Tiers Design Document

## Overview

This document describes the architecture for tier-specific behavior in ViralClipAI's intelligent video processing pipeline. The implementation makes `intelligent`, `intelligent_audio`, and `intelligent_speaker` styles produce meaningfully different outputs based on their detection tier.

## Tier Definitions

### DetectionTier::None (SplitFast, Static Styles)

- **Detection**: Heuristic positioning only
- **Camera behavior**: Fixed or center-weighted positioning
- **Use case**: Fast processing, deterministic results

### DetectionTier::Basic (Intelligent, IntelligentSplit)

- **Detection**: YuNet face detection
- **Camera behavior**: Follow the most prominent face (largest × confidence)
- **Use case**: Single-speaker content, general face tracking

### DetectionTier::AudioAware (IntelligentAudio, IntelligentSplitAudio)

- **Detection**: YuNet + audio activity detection
- **Camera behavior**: Prioritize faces on the active speaker side
- **Switching speed**: Fast (0.2-0.3s transition)
- **Use case**: Podcast-style content with clear speaker turns

### DetectionTier::SpeakerAware (IntelligentSpeaker, IntelligentSplitSpeaker)

- **Detection**: YuNet + audio + face activity (mouth movement, motion)
- **Camera behavior**: Robust speaker tracking with hysteresis
- **Switching**: Minimum dwell time (1.0s), margin threshold (20%)
- **Use case**: Multi-speaker podcasts with interruptions/overlaps

## Implementation

### New Components

#### TierAwareCameraSmoother (`tier_aware_smoother.rs`)

Camera smoother that uses speaker and activity information:

- **Basic**: `compute_focus_basic()` - largest face × confidence
- **AudioAware**: `compute_focus_audio_aware()` - prioritize speaker side
- **SpeakerAware**: `compute_focus_speaker_aware()` - full activity tracking

#### TierAwareIntelligentCropper (`tier_aware_cropper.rs`)

Orchestrates the full pipeline with tier-specific behavior:

1. Face detection
2. Speaker detection (AudioAware/SpeakerAware only)
3. Tier-aware camera smoothing
4. Crop planning and rendering

#### TierAwareSplitProcessor (`tier_aware_split.rs`)

Split view processing with tier-specific vertical positioning:

- **Basic**: Fixed positioning (0% left, 15% right)
- **AudioAware/SpeakerAware**: Face-aware positioning per panel

### Integration Points

#### IntelligentProcessor (`styles/intelligent.rs`)

Uses `create_tier_aware_intelligent_clip()` with the processor's tier.

#### IntelligentSplitProcessor (`styles/intelligent_split.rs`)

Uses `create_tier_aware_split_clip()` with the processor's tier.

## Camera Behavior Specifications

### Basic Tier

- **Target selection**: Largest face × confidence score
- **Smoothing**: Moving average with 0.3s window
- **Max pan speed**: 600 px/s
- **No speaker awareness**

### AudioAware Tier

- **Target selection**: Face on active speaker side (left/right)
- **Speaker detection**: Stereo audio balance or motion analysis
- **Switching speed**: Fast (0.2-0.3s transition)
- **Fallback**: If no clear speaker, use Basic behavior

### SpeakerAware Tier

- **Target selection**: Face with highest activity score
- **Activity components**: Visual activity + audio activity
- **Hysteresis**: Minimum dwell time 1.0s, switch margin 20%
- **Fallback**: If activity unclear, use AudioAware behavior

## File Changes Summary

### Modified Files

- `intelligent/mod.rs` - Export new tier-aware components
- `styles/intelligent.rs` - Use tier-aware intelligent clip
- `styles/intelligent_split.rs` - Use tier-aware split clip

### New Files

- `intelligent/tier_aware_cropper.rs` - Tier-aware orchestration
- `intelligent/tier_aware_smoother.rs` - Tier-specific camera logic
- `intelligent/tier_aware_split.rs` - Tier-aware split view
- `intelligent/tests.rs` - Comprehensive tests

## Preserved Fixes

1. **A/V sync**: Two-pass seeking in `extract_segment()`
2. **Padding**: `pad_before_seconds` and `pad_after_seconds`
3. **Split framing**: 15% vertical bias for bottom panel
4. **Camera responsiveness**: 600 px/s max pan speed, 0.3s smoothing
