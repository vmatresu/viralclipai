# Video Processing Pipeline - Production Implementation

## Overview

This document describes the production-ready video processing pipeline implemented in the ViralClip backend (Rust). The pipeline follows best practices for security, error handling, and modularity.

## Architecture

### Core Principle: highlights.json First

**Critical Workflow Rule**: `highlights.json` must exist before clips can be accessed. This file is created during AI analysis and serves as the source of truth for all clip metadata.

```
1. Download Video → Create Firestore Record (status: Processing)
2. AI Analysis → Generate highlights.json → Upload to R2
3. Generate Clips → Upload to R2 → Update Firestore (status: Completed)
4. API Access → Verify highlights.json exists → Return clip data
```

## Components

### 1. API Layer (`vclip-api`)

#### `/api/videos/{video_id}` - Get Video Info

**Security & Validation**:

- ✅ Input validation: Video ID format (alphanumeric + hyphens, 8-64 chars)
- ✅ Ownership verification via Firestore
- ✅ Status checking before returning data
- ✅ Proper error handling with appropriate HTTP status codes

**Error Handling**:

```rust
// 400 Bad Request - Invalid video ID format
if !is_valid_video_id(&video_id) {
    return Err(ApiError::bad_request("Invalid video ID format"));
}

// 404 Not Found - Video doesn't exist or user doesn't own it
if !state.user_service.user_owns_video(&user.uid, &video_id).await? {
    return Err(ApiError::not_found("Video not found"));
}

// 409 Conflict - Video still processing, highlights.json not ready
if matches!(e, StorageError::NotFound(_)) {
    return Err(ApiError::Conflict(
        "Video is still being processed. Highlights will be available once AI analysis completes."
    ));
}
```

**Response Codes**:

- `200 OK` - Video info with clips
- `400 Bad Request` - Invalid video ID format
- `404 Not Found` - Video not found or access denied
- `409 Conflict` - Video still processing (highlights.json not created yet)
- `500 Internal Server Error` - Database/storage errors (sanitized in production)

### 2. Worker Layer (`vclip-worker`)

#### Video Processing Pipeline

**Phase 1: Download & Initialize (0-15%)**

```rust
// Create work directory
let work_dir = PathBuf::from(&ctx.config.work_dir).join(job.video_id.as_str());
tokio::fs::create_dir_all(&work_dir).await?;

// Download video
let video_file = work_dir.join("source.mp4");
download_video(&job.video_url, &video_file).await?;

// Create Firestore record (status: Processing)
let video_meta = VideoMetadata::new(job.video_id, &job.user_id, &job.video_url, "Processing...");
video_repo.create(&video_meta).await?;
```

**Phase 2: AI Analysis (15-40%)**

```rust
// Analyze video to extract highlights
let highlights_data = analyze_video_highlights(
    ctx,
    &job.job_id,
    &video_file,
    &job.video_url,
    job.custom_prompt.as_deref(),
).await?;

// Validate highlights exist
if highlights_data.highlights.is_empty() {
    video_repo.fail(&job.video_id, "No highlights detected").await?;
    return Err(WorkerError::job_failed("No highlights detected in video"));
}

// Upload highlights.json to R2 (CRITICAL STEP)
ctx.storage
    .upload_highlights(&job.user_id, job.video_id.as_str(), &highlights_data)
    .await?;
```

**Phase 3: Clip Generation (40-95%)**

```rust
// Generate clip tasks from highlights
let clip_tasks = generate_clip_tasks(&highlights_data, &job.styles, &job.crop_mode, &job.target_aspect);

// Process each clip
for (idx, task) in clip_tasks.iter().enumerate() {
    match process_clip_task(ctx, &job.job_id, &job.video_id, &job.user_id,
                           &video_file, &clips_dir, task, idx, total_clips).await {
        Ok(_) => completed_clips += 1,
        Err(e) => {
            // Log error but continue processing other clips
            ctx.progress.log(&job.job_id, format!("Failed clip: {}", e)).await.ok();
        }
    }
}
```

**Phase 4: Finalization (95-100%)**

```rust
// Mark video as completed
video_repo.complete(&job.video_id, completed_clips).await?;

// Cleanup work directory
tokio::fs::remove_dir_all(&work_dir).await.ok();

// Send completion event
ctx.progress.done(&job.job_id, job.video_id.as_str()).await.ok();
```

### 3. Storage Layer (`vclip-storage`)

#### R2 Storage Structure

```
{user_id}/
  {video_id}/
    highlights.json          # MUST exist before clips are accessible
    clips/
      clip_01_1_title_split.mp4
      clip_01_1_title_split.jpg
      clip_02_2_title_original.mp4
      ...
```

#### highlights.json Schema

```json
{
  "highlights": [
    {
      "id": 1,
      "title": "Highlight 1",
      "description": "Interesting moment from 0.0s to 60.0s",
      "start": "00:00:00.000",
      "end": "00:01:00.000",
      "duration": 60,
      "hook_category": "engaging",
      "reason": "High engagement potential"
    }
  ],
  "video_url": "https://www.youtube.com/watch?v=...",
  "video_title": "YouTube Video X4gxTGDzil4",
  "custom_prompt": null
}
```

## Error Handling Strategy

### Principle: Fail Fast, Fail Gracefully

1. **Input Validation**: Reject invalid input immediately (400 Bad Request)
2. **Authorization**: Verify ownership before any operations (404 Not Found)
3. **Resource State**: Check resource state before operations (409 Conflict)
4. **Partial Failures**: Continue processing when individual clips fail
5. **Cleanup**: Always cleanup temporary files, even on failure
6. **Status Tracking**: Update Firestore status on success/failure

### Error Recovery

```rust
// On AI analysis failure
Err(e) => {
    video_repo.fail(&job.video_id, &e.to_string()).await.ok();
    return Err(e);
}

// On clip processing failure (continue with others)
Err(e) => {
    ctx.progress.log(&job.job_id, format!("Failed clip: {}", e)).await.ok();
    // Don't return error - continue processing
}

// Always cleanup
if work_dir.exists() {
    tokio::fs::remove_dir_all(&work_dir).await.ok();
}
```

## Security Best Practices

### 1. Input Validation

- ✅ Video ID format validation (alphanumeric + hyphens only)
- ✅ Clip name validation (no path traversal: `..`, `/`, `\`)
- ✅ Scene ID validation (must exist in highlights)
- ✅ Style validation (must be in allowed list)

### 2. Authorization

- ✅ Ownership verification on all video operations
- ✅ Firebase Auth token validation
- ✅ User ID from verified JWT claims

### 3. Error Sanitization

- ✅ Generic error messages in production
- ✅ Detailed errors only in development
- ✅ No stack traces exposed to clients

### 4. Resource Limits

- ✅ Max 100 videos in bulk delete
- ✅ Max 50 scenes in reprocess
- ✅ Max 10 styles per request
- ✅ Plan-based clip limits

## Performance Optimizations

### 1. Concurrent Processing

```rust
// FFmpeg semaphore limits concurrent processes
let ffmpeg_semaphore = Arc::new(Semaphore::new(config.max_ffmpeg_processes));

// Acquire permit before processing
let _permit = ctx.ffmpeg_semaphore.acquire().await.unwrap();
```

### 2. Progress Tracking

```rust
// Granular progress updates (0-100%)
ctx.progress.progress(&job.job_id, 5).await.ok();   // Download started
ctx.progress.progress(&job.job_id, 40).await.ok();  // AI complete
ctx.progress.progress(&job.job_id, 95).await.ok();  // Clips complete
```

### 3. Cleanup Strategy

```rust
// Remove work directory after processing
if work_dir.exists() {
    tokio::fs::remove_dir_all(&work_dir).await.ok();
}
```

## Testing Strategy

### Unit Tests

- Input validation functions
- Timestamp formatting
- Filename sanitization
- Video ID extraction

### Integration Tests

- Full video processing pipeline
- Error recovery scenarios
- Partial failure handling
- Storage operations

### Load Tests

- Concurrent job processing
- FFmpeg semaphore limits
- Redis queue performance

## Monitoring & Observability

### Structured Logging

```rust
info!("Processing video job: {}", job.job_id);
warn!("Highlights not found for video {}, status: {:?}", video_id, video_meta.status);
```

### Metrics (Prometheus)

- Job completion rate
- Processing duration
- Error rates by type
- Active workers

### Progress Events (WebSocket)

- Real-time progress updates
- Clip upload notifications
- Error notifications

## Future Enhancements

### AI Integration

**Gemini AI Integration** (`backend/crates/vclip-worker/src/gemini.rs`):

The system now uses **Google Gemini AI** for real highlight extraction:

1. **Transcript Extraction**: Uses `yt-dlp` to download video captions (VTT format)
2. **Transcript Parsing**: Converts VTT to timestamped text format
3. **AI Analysis**: Sends transcript + prompt to Gemini API
4. **Highlight Extraction**: Gemini returns 3-10 viral segments (20-90s each)

**Models with Fallback**:

- `gemini-2.0-flash-exp` (primary)
- `gemini-1.5-flash` (fallback 1)
- `gemini-1.5-pro` (fallback 2)

**Environment Variable Required**:

```bash
GEMINI_API_KEY=your_api_key_here
```

**Prompt Customization**:

1. User's custom prompt (highest priority)
2. `prompt.txt` file in worker directory
3. Built-in default prompt (fallback)

### Intelligent Cropping

Integration point for smart reframe module:

```rust
// In process_clip_task(), before create_clip()
if task.crop_mode == CropMode::Intelligent {
    // Use smart_reframe module to analyze and crop
    let crop_plan = analyze_crop_plan(video_file, task).await?;
    create_clip_with_intelligent_crop(video_file, output_path, task, crop_plan).await?;
}
```

## Deployment Checklist

- [ ] Set `ENVIRONMENT=production` in production
- [ ] Configure R2 credentials
- [ ] Configure Firebase credentials
- [ ] Set Redis URL
- [ ] Configure worker concurrency limits
- [ ] Set up monitoring/alerting
- [ ] Enable structured logging
- [ ] Configure CORS for frontend domain
- [ ] Set up CDN for R2 public URLs
- [ ] Configure backup/retention policies

## Troubleshooting

### "Video is still being processed" (409 Conflict)

**Cause**: Frontend polling too early, highlights.json not created yet  
**Solution**: Wait for WebSocket `done` event before fetching video info

### "Video not found" (404)

**Cause**: Video doesn't exist or user doesn't own it  
**Solution**: Verify video ID and user authentication

### "No highlights detected in video"

**Cause**: AI analysis failed to find engaging moments  
**Solution**: Check video content, adjust custom prompt, verify video duration

### Clips missing after processing

**Cause**: Individual clip processing failed  
**Solution**: Check worker logs, verify FFmpeg availability, check disk space
