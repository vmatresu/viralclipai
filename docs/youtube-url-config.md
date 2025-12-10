# YouTube URL Configuration Module

The `youtube_url_config` module in `vclip-models` provides comprehensive YouTube URL parsing, validation, and yt-dlp configuration generation.

## Overview

This module:

1. **Validates** user-pasted YouTube URLs as untrusted input
2. **Extracts** canonical 11-character video IDs
3. **Classifies** URL types (video, shorts, playlist item, etc.)
4. **Generates** safe yt-dlp configuration plans for:
   - Subtitle/transcript download
   - Video file download

## Security

- URLs are treated as untrusted input
- Only YouTube hosts are accepted (`youtube.com`, `youtu.be`, `youtube-nocookie.com` and their subdomains via host-based checks)
- Rejects look-alike hosts or URLs that merely contain a YouTube link in the path/query (e.g., `evil-youtube.com`, `example.com/redirect?target=https://youtube.com/...`)
- Video IDs are strictly validated (exactly 11 alphanumeric chars + `-_`)
- Non-HTTP(S) schemes are rejected
- No shell command execution or external API calls
- Conservative: ambiguous URLs are marked `unsupported`

## Usage

### Rust API

```rust
use vclip_models::{analyze_youtube_url, YoutubeUrlInput, LiveCaptureMode};

let input = YoutubeUrlInput {
    raw_url: "https://youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
    preferred_sub_langs: vec!["en".to_string()],
    allow_auto_subs: true,
    live_capture_mode: LiveCaptureMode::FromStart,
    max_expected_duration_sec: 21600,
};

let config = analyze_youtube_url(&input);

if config.validation.is_supported_youtube_url {
    println!("Video ID: {}", config.video_id.unwrap());
    println!("yt-dlp flags: {:?}", config.video_download_plan.yt_dlp_flags);
}
```

### JSON API

```rust
use vclip_models::analyze_youtube_url_json;

let input_json = r#"{
    "raw_url": "https://youtube.com/watch?v=dQw4w9WgXcQ",
    "preferred_sub_langs": ["en"],
    "allow_auto_subs": true,
    "live_capture_mode": "from_start",
    "max_expected_duration_sec": 21600
}"#;

let output_json = analyze_youtube_url_json(input_json)?;
```

## Input Schema

| Field                       | Type     | Default      | Description                                                                                                                        |
| --------------------------- | -------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `raw_url`                   | string   | required     | User-pasted URL (untrusted)                                                                                                        |
| `preferred_sub_langs`       | string[] | `["en"]`     | Preferred subtitle languages (sanitized: lowercase, alphanumeric/`-`/`_`, max 16 chars; invalid entries are ignored with warnings) |
| `allow_auto_subs`           | bool     | `true`       | Allow auto-generated subtitles                                                                                                     |
| `live_capture_mode`         | enum     | `from_start` | `from_start` or `from_now`                                                                                                         |
| `max_expected_duration_sec` | u64      | `21600`      | Max expected video duration (hint only; does not change flags today)                                                               |

## Output Schema

```json
{
  "normalized_url": "https://www.youtube.com/watch?v=VIDEO_ID",
  "video_id": "VIDEO_ID",
  "url_type": "video | shorts | playlist_item | channel_item | unsupported",
  "is_shorts": false,
  "is_live": false,                       // currently never true without external metadata
  "has_subtitles": null,
  "subtitle_plan": {
    "enabled": true,
    "preferred_languages": ["en"],
    "allow_auto_subs": true,
    "exclude_live_chat": true,
    "yt_dlp_flags": ["--skip-download", "--write-subs", ...]
  },
  "video_download_plan": {
    "enabled": true,
    "preferred_container": "mp4",
    "preferred_codecs": ["h264", "aac"],
    "is_chunked_live_capture": false,
    "yt_dlp_flags": ["-f", "bv*[ext=mp4]+ba[ext=m4a]/bv*+ba/b", ...]
  },
  "live_handling": {
    "live_capture_mode": "from_start",
    "download_sections": null
  },
  "validation": {
    "is_supported_youtube_url": true,
    "errors": []
  }
}
```

## URL Type Classification

| Type            | Description                  | Example                   |
| --------------- | ---------------------------- | ------------------------- |
| `video`         | Standard watch URL           | `youtube.com/watch?v=...` |
| `shorts`        | YouTube Shorts               | `youtube.com/shorts/...`  |
| `playlist_item` | Playlist with specific video | `...?v=...&list=...`      |
| `channel_item`  | Channel URL with video       | `/@channel?v=...`         |
| `unsupported`   | Cannot process               | Invalid/ambiguous URLs    |

`live` / `live_vod` variants remain in the enum for forward compatibility but are not emitted by this pure, metadata-free module.

## Supported URL Formats

- `https://www.youtube.com/watch?v=VIDEO_ID`
- `https://youtube.com/watch?v=VIDEO_ID&list=PLAYLIST_ID`
- `https://youtu.be/VIDEO_ID`
- `https://youtu.be/VIDEO_ID?t=30`
- `https://www.youtube.com/embed/VIDEO_ID`
- `https://www.youtube.com/v/VIDEO_ID`
- `https://www.youtube.com/shorts/VIDEO_ID`
- `https://m.youtube.com/watch?v=VIDEO_ID`
- `https://www.youtube-nocookie.com/embed/VIDEO_ID`

## Generated yt-dlp Flags

### Subtitle Plan

```bash
yt-dlp \
  --skip-download \
  --write-subs \
  --no-playlist \
  --write-auto-subs \        # if allow_auto_subs=true
  --sub-langs "en,-live_chat" \  # sanitized languages, fallback to "en" if invalid
  --convert-subs srt \
  "https://www.youtube.com/watch?v=VIDEO_ID"
```

### Video Download Plan

```bash
yt-dlp \
  -f "bv*[ext=mp4]+ba[ext=m4a]/bv*+ba/b" \
  --merge-output-format mp4 \
  --no-playlist \
  "https://www.youtube.com/watch?v=VIDEO_ID"
```

## Error Handling

When validation fails, `validation.errors` contains human-readable messages:

- `"Non-YouTube domain: example.com"`
- `"Video ID has invalid format (must be 11 alphanumeric characters)"`
- `"Playlist URL without specific video selected (v= parameter missing)"`
- `"Channel/user URL without specific video"`

## Live Content Handling

**Important**: Without an external API call, the module cannot reliably determine if a video is currently live. Conservative behavior:

- This module does **not** emit `url_type: "live"` / `"live_vod"` today; they are reserved for future metadata-aware classification.
- `is_live` is currently always `false`, so live-specific download flags are not enabled unless external metadata sets it later.
- Backend should verify live status via yt-dlp metadata before download.

## Files

- `backend/crates/vclip-models/src/youtube_url_config.rs` - Main implementation
- `backend/crates/vclip-models/src/utils.rs` - Base `extract_youtube_id()` function
- `backend/crates/vclip-models/examples/youtube_url_demo.rs` - Demo/testing

## Tests

Run tests:

```bash
cargo test -p vclip-models youtube_url_config
```

31 tests covering:

- Valid URL formats (watch, short, embed, shorts, playlist)
- Invalid URLs (wrong domain, bad video ID, ambiguous)
- Subtitle plan generation
- Video download plan generation
- JSON serialization/deserialization
- Edge cases (whitespace, case sensitivity)
