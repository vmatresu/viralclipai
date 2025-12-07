# Progress Reporting Architecture

## Overview

This document describes the production-grade progress reporting system implemented for video processing in ViralClip. The system provides real-time, fine-grained progress updates from backend to frontend with comprehensive error handling and observability.

## Architecture Principles

### SOLID Principles Applied

1. **Single Responsibility Principle (SRP)**

   - Each function has one clear purpose
   - `process_scene()` - Orchestrates parallel scene processing
   - `process_single_clip()` - Handles individual clip processing
   - Progress emission is separated from business logic

2. **Open/Closed Principle (OCP)**

   - Progress events are extensible via enum variants
   - New processing steps can be added without modifying existing code
   - Style processors are pluggable via registry pattern

3. **Liskov Substitution Principle (LSP)**

   - All style processors implement the same `StyleProcessor` trait
   - Progress channel can be swapped with different implementations

4. **Interface Segregation Principle (ISP)**

   - Progress channel provides focused methods for each event type
   - Clients only depend on methods they use

5. **Dependency Inversion Principle (DIP)**
   - Worker depends on abstractions (`ProgressChannel`, `StyleProcessor`)
   - Concrete implementations injected via `EnhancedProcessingContext`

### DRY (Don't Repeat Yourself)

- Helper closure `emit_progress` eliminates repetitive progress emission code
- Structured logging macros reduce boilerplate
- Error context added via `map_err` chains

### Modern Rust Patterns

- **Async/Await**: Non-blocking I/O for scalability
- **Result Type**: Explicit error handling
- **Structured Logging**: Machine-parseable logs with `tracing`
- **Semaphore**: Resource control to prevent FFmpeg process explosion
- **Repository Pattern**: Clean data access layer

## Progress Event Flow

```
┌─────────────────────────────────────────────────────────────┐
│                     Worker Process                           │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  process_scene()                                      │  │
│  │  ├─ Emit: scene_started                              │  │
│  │  ├─ Parallel processing of styles                    │  │
│  │  │  ├─ process_single_clip() [Style 1]              │  │
│  │  │  │  ├─ Emit: extracting_segment                  │  │
│  │  │  │  ├─ Emit: rendering                           │  │
│  │  │  │  ├─ Emit: render_complete                     │  │
│  │  │  │  ├─ Emit: uploading                           │  │
│  │  │  │  ├─ Emit: upload_complete                     │  │
│  │  │  │  └─ Emit: complete                            │  │
│  │  │  ├─ process_single_clip() [Style 2]              │  │
│  │  │  └─ process_single_clip() [Style N]              │  │
│  │  └─ Emit: scene_completed                            │  │
│  └──────────────────────────────────────────────────────┘  │
│                           │                                  │
└───────────────────────────┼──────────────────────────────────┘
                            │
                            ▼
                   ┌────────────────┐
                   │ ProgressChannel │
                   │  (Redis Pub/Sub)│
                   └────────────────┘
                            │
                            ▼
                   ┌────────────────┐
                   │  WebSocket API  │
                   └────────────────┘
                            │
                            ▼
                   ┌────────────────┐
                   │  Frontend UI    │
                   │  - Scene cards  │
                   │  - Style status │
                   │  - Progress bars│
                   └────────────────┘
```

## Processing Stages

### Scene-Level Events

1. **scene_started**

   - Emitted when scene processing begins
   - Includes: scene_id, title, style_count, timing
   - Used for: Creating scene progress cards in UI

2. **scene_completed**
   - Emitted when all styles for a scene finish
   - Includes: clips_completed, clips_failed
   - Used for: Updating scene status, showing success/failure

### Clip-Level Events (ClipProcessingStep)

1. **extracting_segment**

   - Parsing timestamps and preparing video segment
   - Details: Time range (e.g., "10.5s - 40.2s (29.7s)")

2. **detecting_faces** _(future)_

   - Running YuNet face detection
   - Details: Number of faces detected

3. **face_detection_complete** _(future)_

   - Face detection finished
   - Details: Face count, confidence scores

4. **computing_camera_path** _(future)_

   - Calculating intelligent crop path
   - Details: Algorithm used (basic/audio-aware/speaker-aware)

5. **camera_path_complete** _(future)_

   - Camera path computation finished
   - Details: Keyframe count

6. **computing_crop_windows** _(future)_

   - Generating crop windows for each frame
   - Details: Frame count

7. **rendering**

   - FFmpeg processing video with filters
   - Details: Style name

8. **render_complete**

   - Video rendering finished
   - Details: Output file size, duration

9. **uploading**

   - Uploading to R2 storage
   - Details: Filename

10. **upload_complete**

    - Upload finished successfully
    - Details: R2 key

11. **complete**

    - Clip fully processed and saved
    - No additional details

12. **failed**
    - Processing failed at any stage
    - Details: Error message

## Error Handling Strategy

### Fail-Fast vs. Continue-on-Error

**Fail-Fast (Critical Errors)**

- Semaphore acquisition failure → Job fails
- Storage upload failure → Job fails
- Invalid processing request → Job fails

**Continue-on-Error (Non-Critical)**

- Firestore metadata save failure → Log warning, continue
- Thumbnail upload failure → Log warning, continue
- Progress event emission failure → Log warning, continue

### Error Context Propagation

```rust
// Before: Silent error
let result = processor.process(request, ctx).await?;

// After: Structured error with context
let result = processor.process(request, ctx).await
    .map_err(|e| {
        tracing::error!(
            scene_id = scene_id,
            style = %style_name,
            error = %e,
            "Style processor failed"
        );
        // Emit failure event for frontend
        let _ = ctx.progress.clip_progress(
            job_id,
            scene_id,
            &style_name,
            ClipProcessingStep::Failed,
            Some(format!("Rendering failed: {}", e)),
        );
        e
    })?;
```

## Structured Logging

### Log Levels

- **ERROR**: Failures that prevent clip/scene completion
- **WARN**: Non-critical failures (metadata save, progress emission)
- **INFO**: Major milestones (scene start/complete, clip start/complete)
- **DEBUG**: Detailed progress (individual clip success)

### Structured Fields

All logs include contextual fields for filtering and analysis:

```rust
tracing::error!(
    scene_id = scene_id,        // Numeric scene identifier
    style = %style_name,         // Style name (formatted)
    clip_index = idx,            // Clip position in batch
    error = %e,                  // Error message (formatted)
    "Clip processing failed"     // Human-readable message
);
```

### Benefits

1. **Machine-Parseable**: JSON output for log aggregation (Datadog, CloudWatch)
2. **Filterable**: Query by scene_id, style, error type
3. **Traceable**: Follow a single clip through entire pipeline
4. **Debuggable**: Rich context for troubleshooting

## Performance Optimizations

### Parallel Processing

- Styles for a scene processed in parallel using `join_all`
- Bounded by FFmpeg semaphore to prevent resource exhaustion
- Typical speedup: 3-4x for scenes with 4+ styles

### Resource Control

```rust
// Semaphore limits concurrent FFmpeg processes
let _permit = ctx.ffmpeg_semaphore.acquire().await?;
```

- Default limit: 4 concurrent FFmpeg processes
- Prevents CPU/memory exhaustion
- Automatic backpressure when limit reached

### Non-Blocking I/O

- All progress events sent asynchronously
- Redis Pub/Sub for efficient message distribution
- WebSocket for real-time frontend updates

## Security Considerations

### Input Validation

- Timestamps validated with defensive defaults
- File paths sanitized via `SecurityContext`
- User IDs validated before storage operations

### Error Message Sanitization

- Internal errors logged with full details
- User-facing errors sanitized to prevent information leakage
- Stack traces only in development mode

### Resource Limits

- Semaphore prevents DoS via excessive FFmpeg processes
- Redis connection pooling prevents connection exhaustion
- Firestore batch limits respected

## Observability

### Metrics Collected

1. **Processing Duration**: Per-clip, per-scene, per-job
2. **Success Rate**: Clips completed vs. failed
3. **Error Types**: Categorized by failure stage
4. **Resource Usage**: FFmpeg semaphore utilization

### Tracing Integration

- Distributed tracing via `tracing` crate
- Correlation IDs for request tracking
- Span hierarchy: Job → Scene → Clip

### Monitoring Queries

```sql
-- Find slow clips
SELECT scene_id, style, duration_sec
FROM logs
WHERE message = 'Clip processing completed successfully'
  AND duration_sec > 60;

-- Error rate by style
SELECT style, COUNT(*) as failures
FROM logs
WHERE message = 'Clip processing failed'
GROUP BY style
ORDER BY failures DESC;
```

## Frontend Integration

### State Management

```typescript
interface SceneProgress {
  sceneId: number;
  sceneTitle: string;
  styleCount: number;
  status: "pending" | "processing" | "completed" | "failed";
  clipsCompleted: number;
  clipsFailed: number;
  currentSteps: Map<string, { step: ClipProcessingStep; details?: string }>;
}
```

### UI Components

1. **Scene Progress Card**

   - Shows scene title, timing, status icon
   - Progress bar for style completion
   - Per-style processing step display

2. **Processing Status**
   - Overall progress bar
   - Scene cards grid
   - Log console

### Real-Time Updates

- WebSocket connection for live updates
- Optimistic UI updates on event receipt
- Graceful degradation if WebSocket fails

## Testing Strategy

### Unit Tests

- Mock `ProgressChannel` for testing event emission
- Test error handling paths
- Verify structured logging output

### Integration Tests

- End-to-end scene processing
- Parallel style processing
- Error recovery scenarios

### Performance Tests

- Measure overhead of progress events
- Verify semaphore behavior under load
- Test Redis Pub/Sub throughput

## Future Enhancements

### Planned Features

1. **Retry Logic**: Automatic retry for transient failures
2. **Progress Estimation**: ML-based time-to-completion prediction
3. **Cancellation**: User-initiated job cancellation
4. **Priority Queues**: VIP users get faster processing

### Monitoring Improvements

1. **Alerting**: Slack/PagerDuty on high error rates
2. **Dashboards**: Grafana dashboards for real-time metrics
3. **Anomaly Detection**: ML-based outlier detection

## References

- [SOLID Principles](https://en.wikipedia.org/wiki/SOLID)
- [Rust Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Structured Logging with Tracing](https://docs.rs/tracing/latest/tracing/)
- [Redis Pub/Sub](https://redis.io/docs/manual/pubsub/)
