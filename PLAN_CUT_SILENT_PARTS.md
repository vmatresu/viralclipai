# Implementation Plan: Cut Silent Parts Feature

## Overview

This feature adds a "Cut silent (no speech) parts" option that:
1. Appears as a checkbox in the Scene Explorer (history page) - **default: ON**
2. Has a user setting to control whether it defaults to checked or unchecked
3. Applies to **all pipelines** (Split, Full, Original, Streamer, etc.)
4. Uses Silero VAD v5 for speech detection in the backend

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         FRONTEND                                     │
├─────────────────────────────────────────────────────────────────────┤
│  Settings Page                   │  History Page (Scene Explorer)   │
│  ┌──────────────────────────┐    │  ┌───────────────────────────┐   │
│  │ Default: Cut silent parts│    │  │ StyleQualitySelector      │   │
│  │ [x] Always on            │    │  │ ...                       │   │
│  │ [ ] Always off           │    │  │ ┌─────────────────────┐   │   │
│  └──────────────────────────┘    │  │ │[x] Cut silent parts │   │   │
│                                  │  │ │    (no speech)      │   │   │
│                                  │  │ └─────────────────────┘   │   │
│                                  │  └───────────────────────────┘   │
└──────────────────────────────────┴──────────────────────────────────┘
                                   │
                                   ▼ WebSocket
┌─────────────────────────────────────────────────────────────────────┐
│                         BACKEND                                      │
├─────────────────────────────────────────────────────────────────────┤
│  ReprocessScenesJob             │  vclip-media                      │
│  ┌────────────────────────┐     │  ┌───────────────────────────┐    │
│  │ cut_silent_parts: bool │────►│  │ SilenceRemover            │    │
│  └────────────────────────┘     │  │ ├─ silero-vad-rust        │    │
│                                 │  │ └─ FFmpeg integration     │    │
│                                 │  └───────────────────────────┘    │
└─────────────────────────────────┴───────────────────────────────────┘
```

---

## UI/UX Design Decision

**Placement**: The "Cut silent parts" checkbox should be placed **outside** the Split/Full cards, similar to "Also export Original" because:
1. It applies to **all styles** (Split, Full, Original, Streamer)
2. It's a **processing modifier**, not a style selection
3. Consistent grouping with other global options

**Suggested UI Layout** (in StyleQualitySelector):
```
┌─────────────────────────────────────────────────────────────────┐
│ Output Layout & Quality                                          │
│ ┌─────────────────────┐  ┌─────────────────────┐                │
│ │    Split View       │  │     Full View       │                │
│ │    [Styles...]      │  │    [Styles...]      │                │
│ └─────────────────────┘  └─────────────────────┘                │
│                                                                  │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ [x] Cut silent parts for more dynamic scenes                │ │
│ │     Remove sections without speech (applies to all styles)  │ │
│ └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ [ ] Also export Original (no cropping)                      │ │
│ │     Optional extra output                                   │ │
│ └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Tasks

### Phase 1: Frontend - Types & State

#### 1.1 Update `types.ts`
**File**: `web/components/style-quality/types.ts`

```typescript
export type LayoutQualitySelection = {
  splitEnabled: boolean;
  splitStyle: string;
  fullEnabled: boolean;
  fullStyle: string;
  staticPosition: StaticPosition;
  includeOriginal: boolean;
  streamerSplitConfig: StreamerSplitConfig;
  topScenesEnabled: boolean;
  cutSilentParts: boolean;  // NEW
};

export const DEFAULT_SELECTION: LayoutQualitySelection = {
  // ... existing fields
  cutSilentParts: true,  // NEW - default ON
};
```

#### 1.2 Update `StyleQualitySelector.tsx`
**File**: `web/components/style-quality/StyleQualitySelector.tsx`

Add props:
```typescript
interface StyleQualitySelectorProps {
  // ... existing props
  cutSilentParts?: boolean;
  onCutSilentPartsChange?: (enabled: boolean) => void;
}
```

Add UI (between styles and "Also export Original"):
```tsx
{/* Cut Silent Parts checkbox */}
<div className="space-y-3 rounded-xl border border-white/10 bg-slate-900/60 p-4">
  <label
    className="flex items-start gap-3 text-sm text-white"
    htmlFor="cut-silent-parts"
  >
    <Checkbox
      checked={cutSilentParts ?? selection.cutSilentParts}
      onCheckedChange={(checked) => {
        onCutSilentPartsChange?.(Boolean(checked));
        updateSelection({ cutSilentParts: Boolean(checked) });
      }}
      id="cut-silent-parts"
    />
    <div className="space-y-0.5">
      <div className="font-medium">Cut silent parts for more dynamic scenes</div>
      <p className="text-xs text-muted-foreground">
        Remove sections without speech (applies to all styles)
      </p>
    </div>
  </label>
</div>
```

### Phase 2: Frontend - Settings Page

#### 2.1 Add User Setting
**File**: `web/app/settings/page.tsx`

Add new state and UI:
```typescript
// State
const [cutSilentPartsDefault, setCutSilentPartsDefault] = useState<boolean>(true);

// Load from settings
useEffect(() => {
  // In the existing load function:
  setCutSilentPartsDefault(res.settings?.cut_silent_parts_default ?? true);
}, []);

// Save to settings
const payload = {
  settings: {
    // ... existing settings
    cut_silent_parts_default: cutSilentPartsDefault,
  },
};
```

Add UI section (after TikTok Integration):
```tsx
<section className="glass rounded-2xl p-6 space-y-4">
  <h2 className="text-xl font-semibold text-foreground">Processing Defaults</h2>
  <p className="text-sm text-muted-foreground">
    Configure default settings for video processing.
  </p>

  <div className="space-y-4">
    <label className="flex items-start gap-3 text-sm">
      <input
        type="checkbox"
        checked={cutSilentPartsDefault}
        onChange={(e) => setCutSilentPartsDefault(e.target.checked)}
        className="mt-0.5"
      />
      <div className="space-y-0.5">
        <div className="font-medium text-foreground">Cut silent parts by default</div>
        <p className="text-xs text-muted-foreground">
          When enabled, "Cut silent parts" will be checked by default in the scene processor
        </p>
      </div>
    </label>
  </div>
</section>
```

### Phase 3: Frontend - History Page Integration

#### 3.1 Add State
**File**: `web/app/history/[id]/page.tsx`

```typescript
// Add state
const [cutSilentParts, setCutSilentParts] = useState<boolean>(true);

// Load from user settings
useEffect(() => {
  if (userSettings?.settings?.cut_silent_parts_default !== undefined) {
    setCutSilentParts(userSettings.settings.cut_silent_parts_default as boolean);
  }
}, [userSettings]);

// Pass to StyleQualitySelector
<StyleQualitySelector
  // ... existing props
  cutSilentParts={cutSilentParts}
  onCutSilentPartsChange={setCutSilentParts}
/>
```

#### 3.2 Update Reprocessing Call
**File**: `web/app/history/[id]/page.tsx`

```typescript
// In startReprocess function
await reprocess(
  sceneIdsToProcess,
  plan.styles,
  hasCinematic && enableObjectDetection,
  overwrite,
  streamerParams,
  isTopScenesCompilation,
  cutSilentParts  // NEW parameter
);
```

### Phase 4: Frontend - WebSocket & Hook

#### 4.1 Update WebSocket Client
**File**: `web/lib/websocket/reprocess-client.ts`

```typescript
export interface ReprocessOptions {
  // ... existing fields
  /** Cut silent parts from clips (default: true) */
  cutSilentParts?: boolean;
}

// In reprocessScenesWebSocket function
const {
  // ... existing destructuring
  cutSilentParts = true,
} = options;

// In ws.onopen send
ws.send(
  JSON.stringify({
    // ... existing fields
    cut_silent_parts: cutSilentParts,
  })
);
```

#### 4.2 Update useReprocessing Hook
**File**: `web/hooks/useReprocessing.ts`

```typescript
const reprocess = useCallback(
  async (
    sceneIds: number[],
    styles: string[],
    enableObjectDetection: boolean = false,
    overwrite: boolean = false,
    streamerSplitParams?: StreamerSplitParams,
    topScenesCompilation: boolean = false,
    cutSilentParts: boolean = true  // NEW parameter
  ) => {
    // ...
    wsRef.current = reprocessScenesWebSocket(
      {
        // ... existing fields
        cutSilentParts,
      },
      callbacks
    );
  },
  [/* dependencies */]
);
```

### Phase 5: Backend - Job Definition

#### 5.1 Update ReprocessScenesJob
**File**: `backend/crates/vclip-queue/src/job.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReprocessScenesJob {
    // ... existing fields

    /// Cut silent parts from all clips using VAD (default: true)
    #[serde(default = "default_cut_silent_parts")]
    pub cut_silent_parts: bool,
}

fn default_cut_silent_parts() -> bool {
    true
}

impl ReprocessScenesJob {
    pub fn new(/* ... */) -> Self {
        Self {
            // ... existing fields
            cut_silent_parts: true,
        }
    }

    /// Set cut silent parts option.
    pub fn with_cut_silent_parts(mut self, enabled: bool) -> Self {
        self.cut_silent_parts = enabled;
        self
    }
}
```

#### 5.2 Update ClipTask
**File**: `backend/crates/vclip-models/src/clip_task.rs` (or equivalent)

```rust
pub struct ClipTask {
    // ... existing fields

    /// Whether to cut silent parts using VAD
    pub cut_silent_parts: bool,
}
```

### Phase 6: Backend - WebSocket Handler

**File**: `backend/crates/vclip-api/src/handlers/ws_reprocess.rs` (or equivalent)

```rust
#[derive(Deserialize)]
struct ReprocessRequest {
    // ... existing fields
    #[serde(default = "default_true")]
    cut_silent_parts: bool,
}

fn default_true() -> bool {
    true
}

// When creating the job
let job = ReprocessScenesJob::new(/* ... */)
    // ... existing options
    .with_cut_silent_parts(request.cut_silent_parts);
```

### Phase 7: Backend - User Settings

#### 7.1 Update Settings Service
**File**: `backend/crates/vclip-api/src/services/user.rs`

The `extra: HashMap<String, serde_json::Value>` already supports arbitrary settings.
Just add validation/defaults:

```rust
impl UserSettings {
    pub fn cut_silent_parts_default(&self) -> bool {
        self.extra
            .get("cut_silent_parts_default")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }
}
```

### Phase 8: Backend - Silence Detection (Layer A)

#### 8.1 Add Dependencies
**File**: `backend/crates/vclip-media/Cargo.toml`

```toml
[dependencies]
silero-vad-rust = "0.1"  # Or silero-vad-rs
```

#### 8.2 Create Silence Remover Module
**File**: `backend/crates/vclip-media/src/silence_removal/mod.rs`

```rust
//! Silence removal using Silero VAD v5.
//!
//! This module implements "Layer A: The Meat Cleaver" - detecting
//! and removing segments without speech, even in presence of
//! music/game audio.

mod vad;
mod segmenter;
mod config;

pub use config::SilenceRemovalConfig;
pub use segmenter::{Segment, SegmentLabel, SilenceRemover};

/// Default configuration for silence removal.
pub fn default_config() -> SilenceRemovalConfig {
    SilenceRemovalConfig {
        vad_threshold: 0.5,
        min_silence_ms: 1000,
        pre_speech_padding_ms: 200,
        post_speech_padding_ms: 200,
    }
}
```

#### 8.3 VAD Wrapper
**File**: `backend/crates/vclip-media/src/silence_removal/vad.rs`

```rust
use silero_vad_rust::{VadConfig, Vad};

pub struct SileroVad {
    vad: Vad,
}

impl SileroVad {
    pub fn new() -> anyhow::Result<Self> {
        let config = VadConfig::default();
        let vad = Vad::new(config)?;
        Ok(Self { vad })
    }

    /// Analyze a single frame of audio and return speech probability.
    /// Input: 16kHz mono f32 PCM samples.
    pub fn analyze_frame(&mut self, pcm: &[f32]) -> f32 {
        self.vad.calc_level(pcm)
    }

    pub fn reset(&mut self) {
        self.vad.reset();
    }
}
```

#### 8.4 Segmenter (State Machine)
**File**: `backend/crates/vclip-media/src/silence_removal/segmenter.rs`

```rust
use super::config::SilenceRemovalConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentLabel {
    Keep,
    Cut,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub label: SegmentLabel,
}

enum State {
    InSpeech,
    InSilence { silence_start_ms: u64 },
}

pub struct SilenceRemover {
    config: SilenceRemovalConfig,
    state: State,
    segments: Vec<Segment>,
    current_segment_start: u64,
}

impl SilenceRemover {
    pub fn new(config: SilenceRemovalConfig) -> Self {
        Self {
            config,
            state: State::InSilence { silence_start_ms: 0 },
            segments: Vec::new(),
            current_segment_start: 0,
        }
    }

    /// Process a single frame of VAD output.
    pub fn ingest_frame(&mut self, speech_prob: f32, timestamp_ms: u64) {
        let is_speech = speech_prob >= self.config.vad_threshold;

        match (&self.state, is_speech) {
            (State::InSilence { silence_start_ms }, true) => {
                // Transition to speech
                let silence_duration = timestamp_ms.saturating_sub(*silence_start_ms);

                if silence_duration > self.config.min_silence_ms {
                    // Mark previous silence as Cut (with padding)
                    let cut_end = timestamp_ms
                        .saturating_sub(self.config.pre_speech_padding_ms);

                    if cut_end > self.current_segment_start {
                        self.segments.push(Segment {
                            start_ms: self.current_segment_start,
                            end_ms: cut_end,
                            label: SegmentLabel::Cut,
                        });
                        self.current_segment_start = cut_end;
                    }
                }

                self.state = State::InSpeech;
            }
            (State::InSpeech, false) => {
                // Transition to silence
                self.state = State::InSilence {
                    silence_start_ms: timestamp_ms,
                };
            }
            _ => {
                // No state change
            }
        }
    }

    /// Finalize and return all segments.
    pub fn finalize(mut self, total_duration_ms: u64) -> Vec<Segment> {
        // Handle final segment
        if let State::InSilence { silence_start_ms } = self.state {
            let silence_duration = total_duration_ms.saturating_sub(silence_start_ms);

            if silence_duration > self.config.min_silence_ms {
                // Final silence is Cut
                let cut_start = silence_start_ms + self.config.post_speech_padding_ms;

                if cut_start < total_duration_ms && cut_start > self.current_segment_start {
                    self.segments.push(Segment {
                        start_ms: self.current_segment_start,
                        end_ms: cut_start,
                        label: SegmentLabel::Keep,
                    });
                    self.segments.push(Segment {
                        start_ms: cut_start,
                        end_ms: total_duration_ms,
                        label: SegmentLabel::Cut,
                    });
                } else {
                    self.segments.push(Segment {
                        start_ms: self.current_segment_start,
                        end_ms: total_duration_ms,
                        label: SegmentLabel::Keep,
                    });
                }
            } else {
                self.segments.push(Segment {
                    start_ms: self.current_segment_start,
                    end_ms: total_duration_ms,
                    label: SegmentLabel::Keep,
                });
            }
        } else {
            self.segments.push(Segment {
                start_ms: self.current_segment_start,
                end_ms: total_duration_ms,
                label: SegmentLabel::Keep,
            });
        }

        self.segments
    }
}
```

#### 8.5 Config
**File**: `backend/crates/vclip-media/src/silence_removal/config.rs`

```rust
#[derive(Debug, Clone)]
pub struct SilenceRemovalConfig {
    /// VAD threshold (0.0-1.0), default 0.5
    pub vad_threshold: f32,
    /// Minimum silence duration to cut (ms), default 1000
    pub min_silence_ms: u64,
    /// Padding before speech starts (ms), default 200
    pub pre_speech_padding_ms: u64,
    /// Padding after speech ends (ms), default 200
    pub post_speech_padding_ms: u64,
}

impl Default for SilenceRemovalConfig {
    fn default() -> Self {
        Self {
            vad_threshold: 0.5,
            min_silence_ms: 1000,
            pre_speech_padding_ms: 200,
            post_speech_padding_ms: 200,
        }
    }
}
```

### Phase 9: Backend - FFmpeg Integration

#### 9.1 Detect Speech Segments
**File**: `backend/crates/vclip-media/src/silence_removal/analyze.rs`

```rust
use std::path::Path;
use anyhow::Result;
use super::{SilenceRemover, Segment, SilenceRemovalConfig};
use super::vad::SileroVad;

/// Analyze audio file and return keep/cut segments.
pub async fn analyze_audio_segments(
    audio_path: &Path,
    config: SilenceRemovalConfig,
) -> Result<Vec<Segment>> {
    // Extract 16kHz mono audio for VAD
    let temp_audio = tempfile::NamedTempFile::new()?;
    extract_audio_for_vad(audio_path, temp_audio.path()).await?;

    // Load audio samples
    let samples = load_audio_samples(temp_audio.path()).await?;

    // Process through VAD
    let mut vad = SileroVad::new()?;
    let mut remover = SilenceRemover::new(config);

    // Process in 30ms frames (480 samples at 16kHz)
    const FRAME_SIZE: usize = 480;
    const FRAME_MS: u64 = 30;

    for (i, chunk) in samples.chunks(FRAME_SIZE).enumerate() {
        if chunk.len() < FRAME_SIZE {
            break;
        }

        let speech_prob = vad.analyze_frame(chunk);
        let timestamp_ms = (i as u64) * FRAME_MS;
        remover.ingest_frame(speech_prob, timestamp_ms);
    }

    let total_duration_ms = (samples.len() as u64 * 1000) / 16000;
    Ok(remover.finalize(total_duration_ms))
}

async fn extract_audio_for_vad(input: &Path, output: &Path) -> Result<()> {
    // FFmpeg: extract 16kHz mono PCM
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-i", input.to_str().unwrap(),
            "-ar", "16000",
            "-ac", "1",
            "-f", "f32le",
            "-y",
            output.to_str().unwrap(),
        ])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("FFmpeg audio extraction failed");
    }

    Ok(())
}

async fn load_audio_samples(path: &Path) -> Result<Vec<f32>> {
    let bytes = tokio::fs::read(path).await?;
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    Ok(samples)
}
```

#### 9.2 Apply Silence Removal to Video
**File**: `backend/crates/vclip-media/src/silence_removal/apply.rs`

```rust
use std::path::Path;
use anyhow::Result;
use super::{Segment, SegmentLabel};

/// Apply silence removal to video using FFmpeg.
///
/// This uses the complex filter approach to concatenate only Keep segments.
pub async fn apply_silence_removal(
    input_path: &Path,
    output_path: &Path,
    segments: &[Segment],
) -> Result<()> {
    // Collect only Keep segments
    let keep_segments: Vec<_> = segments
        .iter()
        .filter(|s| s.label == SegmentLabel::Keep)
        .collect();

    if keep_segments.is_empty() {
        anyhow::bail!("No speech segments detected - cannot create empty video");
    }

    // If only one segment covers entire video, no cutting needed
    if keep_segments.len() == 1 {
        let seg = &keep_segments[0];
        return trim_video(input_path, output_path, seg.start_ms, seg.end_ms).await;
    }

    // Build FFmpeg complex filter for multiple segments
    let filter = build_concat_filter(&keep_segments);

    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().unwrap(),
            "-filter_complex", &filter,
            "-map", "[outv]",
            "-map", "[outa]",
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            "-y",
            output_path.to_str().unwrap(),
        ])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("FFmpeg silence removal failed");
    }

    Ok(())
}

fn build_concat_filter(segments: &[&Segment]) -> String {
    let mut filter = String::new();
    let mut v_inputs = Vec::new();
    let mut a_inputs = Vec::new();

    for (i, seg) in segments.iter().enumerate() {
        let start_sec = seg.start_ms as f64 / 1000.0;
        let end_sec = seg.end_ms as f64 / 1000.0;

        // Video trim
        filter.push_str(&format!(
            "[0:v]trim=start={}:end={},setpts=PTS-STARTPTS[v{}];",
            start_sec, end_sec, i
        ));
        v_inputs.push(format!("[v{}]", i));

        // Audio trim
        filter.push_str(&format!(
            "[0:a]atrim=start={}:end={},asetpts=PTS-STARTPTS[a{}];",
            start_sec, end_sec, i
        ));
        a_inputs.push(format!("[a{}]", i));
    }

    // Concat all segments
    let n = segments.len();
    filter.push_str(&format!(
        "{}concat=n={}:v=1:a=0[outv];",
        v_inputs.join(""),
        n
    ));
    filter.push_str(&format!(
        "{}concat=n={}:v=0:a=1[outa]",
        a_inputs.join(""),
        n
    ));

    filter
}

async fn trim_video(
    input: &Path,
    output: &Path,
    start_ms: u64,
    end_ms: u64,
) -> Result<()> {
    let start_sec = start_ms as f64 / 1000.0;
    let duration_sec = (end_ms - start_ms) as f64 / 1000.0;

    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-ss", &start_sec.to_string(),
            "-i", input.to_str().unwrap(),
            "-t", &duration_sec.to_string(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            "-y",
            output.to_str().unwrap(),
        ])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("FFmpeg trim failed");
    }

    Ok(())
}
```

### Phase 10: Backend - Pipeline Integration

#### 10.1 Update Clip Processing
**File**: `backend/crates/vclip-media/src/clip.rs` (or style processors)

```rust
use crate::silence_removal::{analyze_audio_segments, apply_silence_removal, default_config};

pub async fn process_clip(
    input: &Path,
    output: &Path,
    cut_silent_parts: bool,
    // ... other params
) -> Result<()> {
    if cut_silent_parts {
        // Analyze and apply silence removal
        let segments = analyze_audio_segments(input, default_config()).await?;

        // Check if there are significant silent parts to cut
        let total_keep: u64 = segments
            .iter()
            .filter(|s| s.label == SegmentLabel::Keep)
            .map(|s| s.end_ms - s.start_ms)
            .sum();

        let total_duration: u64 = segments.last().map(|s| s.end_ms).unwrap_or(0);

        // Only apply if we're cutting at least 10% of the video
        if total_keep < total_duration * 90 / 100 {
            let temp_output = tempfile::NamedTempFile::new()?;
            apply_silence_removal(input, temp_output.path(), &segments).await?;

            // Continue processing with silence-removed video
            // ... style processing with temp_output as input
        }
    }

    // ... rest of style processing
}
```

---

## File Change Summary

### Frontend Files
| File | Change |
|------|--------|
| `web/components/style-quality/types.ts` | Add `cutSilentParts` field |
| `web/components/style-quality/StyleQualitySelector.tsx` | Add checkbox UI |
| `web/app/settings/page.tsx` | Add default setting toggle |
| `web/app/history/[id]/page.tsx` | Add state and pass to reprocess |
| `web/lib/websocket/reprocess-client.ts` | Add `cutSilentParts` option |
| `web/hooks/useReprocessing.ts` | Add parameter to `reprocess()` |

### Backend Files
| File | Change |
|------|--------|
| `backend/crates/vclip-queue/src/job.rs` | Add `cut_silent_parts` to job |
| `backend/crates/vclip-api/src/handlers/ws_reprocess.rs` | Parse new field |
| `backend/crates/vclip-api/src/services/user.rs` | Add helper for setting |
| `backend/crates/vclip-media/Cargo.toml` | Add silero-vad dependency |
| `backend/crates/vclip-media/src/lib.rs` | Export silence_removal module |
| `backend/crates/vclip-media/src/silence_removal/mod.rs` | NEW: Module root |
| `backend/crates/vclip-media/src/silence_removal/config.rs` | NEW: Configuration |
| `backend/crates/vclip-media/src/silence_removal/vad.rs` | NEW: VAD wrapper |
| `backend/crates/vclip-media/src/silence_removal/segmenter.rs` | NEW: State machine |
| `backend/crates/vclip-media/src/silence_removal/analyze.rs` | NEW: Audio analysis |
| `backend/crates/vclip-media/src/silence_removal/apply.rs` | NEW: FFmpeg integration |
| `backend/crates/vclip-worker/src/reprocessing.rs` | Pass option through pipeline |

---

## Testing Plan

1. **Unit Tests**
   - Test `SilenceRemover` state machine with mock VAD output
   - Test segment merging and padding logic
   - Test FFmpeg filter string generation

2. **Integration Tests**
   - Test silence detection on sample audio files
   - Test end-to-end silence removal on sample videos

3. **UI Tests**
   - Verify checkbox state persistence
   - Verify user setting loading/saving
   - Verify WebSocket message includes flag

4. **Manual Tests**
   - Process a video with music-only sections
   - Process a video with long silent pauses
   - Verify clips are shorter with feature enabled
   - Verify quality is maintained

---

## Future Enhancements

1. **User-configurable thresholds** (Phase 2)
   - Expose `min_silence_ms` and `vad_threshold` in UI
   - Allow per-style configuration

2. **Preview mode** (Phase 3)
   - Show detected segments before processing
   - Allow user to adjust before final render

3. **Filler word removal** (Layer B)
   - Extend to remove "uh", "um", etc.
   - Requires ASR integration
