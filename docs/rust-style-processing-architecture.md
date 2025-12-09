# Rust Style Processing Architecture (Current)

This is the concise, up-to-date view of how styles are processed in the Rust stack. Legacy Python references are removed; the source of truth is `vclip-media` plus the worker `clip_pipeline`.

## Routing & Responsibilities

- **Factory**: `StyleProcessorFactory` builds a `StyleProcessor` per style/tier.
- **Static styles** (Original, Split, Left/Right Focus) share `run_basic_style` for DRY logging/metrics/thumbnails and FFmpeg filters defined in `filters.rs`.
- **Fast style**: `SplitFastProcessor` uses `FastSplitEngine` (heuristic split) plus thumbnails.
- **Intelligent styles**: tier-aware processors (None/Basic/MotionAware/SpeakerAware) delegate to `create_tier_aware_intelligent_clip` / `create_tier_aware_split_clip`. MotionAware is NN-free (frame-diff motion); SpeakerAware is visual-only (FaceMesh mouth MAR, no audio).
- **Encoding**: style-specific presets are selected in `clip_pipeline/clip.rs` (`EncodingConfig::for_intelligent_crop` or `for_split_view`, otherwise default).

## Modules

- `vclip-media/styles/` – individual style processors implementing `StyleProcessor`.
- `vclip-media/intelligent/` – detectors, trackers, planners, renderers, fast split, motion detector, tier-aware crop/split engines.
- `vclip-media/filters.rs` – FFmpeg filters for static crops/splits.
- `vclip-media/encoding.rs` – presets for codecs, CRF, audio bitrate, NVENC.
- `vclip-worker/clip_pipeline` – constructs tasks, fans out per scene, selects processors, uploads outputs, writes Firestore metadata.

## Style Behavior (summary)

- **original**: transcode only, no filters. Uses `run_basic_style`.
- **split / left_focus / right_focus**: single-pass FFmpeg with predefined filters; thumbnails generated; uses `run_basic_style`.
- **split_fast**: FastSplit heuristic (no AI), extracts segment then runs `FastSplitEngine`; thumbnails generated.
- **intelligent / intelligent_motion / intelligent_speaker**: tier-aware intelligent crop on a pre-cut segment; Basic uses YuNet, Motion uses NN-free motion heuristic, Speaker uses FaceMesh mouth activity (visual-only); thumbnails generated.
- **intelligent_split\* (Basic/Motion/Speaker)**: tier-aware split view; per-panel detection/positioning; Speaker split invariant left→top/right→bottom; thumbnails generated.

## Crop Modes

- Supported: `none`, `center`, `manual`, `intelligent`. Intelligent crop mode is handled via the intelligent processors; other modes are static filters or default framing.

## Data & Metadata

- Output clips: MP4 + JPG thumbnail per clip (same stem) uploaded to R2.
- `ClipMetadata` stored in Firestore with size, duration, style, scene, and R2 keys.
- Progress events emitted per clip stage (extract → render → upload → complete) via the worker progress channel.

## Extending Styles

- Add a new style by implementing `StyleProcessor`, registering in `StyleProcessorFactory`, and (if needed) adding filter or intelligent pipeline steps.
- Map the style to an encoding preset in `clip_pipeline/clip.rs` to keep bitrate/quality consistent.
- Include thumbnail generation; treat upload failures as warnings, not job-ending errors.

## Safety & Performance

- FFmpeg commands are sanitized; resource use is bounded by semaphores.
- Segment extraction is frame-accurate (input + output seeking) to avoid A/V drift.
- Intelligent pipelines use tier-aware detectors and smoothing to balance quality vs speed.
