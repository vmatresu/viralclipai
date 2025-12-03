# Firestore Database Schema

This document describes the Firestore database schema for Viral Clip AI, optimized for fast queries and efficient data access.

## Architecture Principles

- **Firestore for Metadata**: Fast queries, indexing, real-time updates
- **R2 for Files**: Large file storage, presigned URLs for secure access
- **Separation of Concerns**: Metadata separate from file storage

## Collection Structure

```
users/{uid}/
  ├── videos/{video_id}          # Video metadata
  └── clips/{clip_id}            # Clip metadata (subcollection)
```

## Schema Definitions

### Video Document

**Path**: `users/{uid}/videos/{video_id}`

```typescript
{
  // Identifiers
  video_id: string;              // Unique video ID (run_id)
  user_id: string;              // Owner user ID

  // Video Information
  video_url: string;            // Source YouTube URL
  video_title: string;           // Video title
  youtube_id: string;            // Extracted YouTube ID

  // Processing Status
  status: "processing" | "completed" | "failed";
  created_at: Timestamp;
  completed_at?: Timestamp;
  failed_at?: Timestamp;
  error_message?: string;

  // Highlights Metadata (summary)
  highlights_count: number;    // Number of scenes detected
  highlights_summary?: {        // Quick summary (optional, for UI)
    total_duration: number;     // Total duration in seconds
    categories: string[];        // Unique hook categories
  };

  // Processing Configuration
  custom_prompt?: string;        // Custom prompt used
  styles_processed: string[];   // Styles that were processed
  crop_mode: string;            // Crop mode used
  target_aspect: string;        // Target aspect ratio

  // Statistics
  clips_count: number;          // Total clips generated
  clips_by_style: {             // Clips grouped by style
    [style: string]: number;
  };

  // Storage References
  highlights_json_key: string;  // R2 key: "{uid}/{video_id}/highlights.json"

  // Metadata
  created_by: string;           // User ID
  updated_at: Timestamp;
}
```

### Clip Document

**Path**: `users/{uid}/videos/{video_id}/clips/{clip_id}`

```typescript
{
  // Identifiers
  clip_id: string;              // Unique clip ID (filename without extension)
  video_id: string;             // Parent video ID
  user_id: string;              // Owner user ID

  // Scene Information
  scene_id: number;             // Highlight/scene ID from highlights.json
  scene_title: string;          // Scene title
  scene_description?: string;   // Scene description

  // Clip Metadata
  filename: string;             // R2 filename: "clip_XX_XX_title_style.mp4"
  style: string;                // Style: "split", "left_focus", etc.
  priority: number;             // Processing priority

  // Timing Information
  start_time: string;           // "HH:MM:SS" format
  end_time: string;             // "HH:MM:SS" format
  duration_seconds: number;      // Clip duration

  // File Information
  file_size_bytes: number;       // File size in bytes
  file_size_mb: number;          // File size in MB (for display)
  has_thumbnail: boolean;        // Whether thumbnail exists

  // Storage References
  r2_key: string;               // R2 key: "{uid}/{video_id}/clips/{filename}"
  thumbnail_r2_key?: string;    // R2 key for thumbnail

  // Status
  status: "processing" | "completed" | "failed";
  created_at: Timestamp;
  completed_at?: Timestamp;

  // Metadata
  created_by: string;           // User ID
}
```

## Indexes Required

### Video Collection Indexes

1. **User Videos Query** (for history page)

   - Collection: `users/{uid}/videos`
   - Fields: `created_at` (descending)
   - Used by: `list_user_videos()`

2. **Status Query** (for processing status)
   - Collection: `users/{uid}/videos`
   - Fields: `status`, `created_at` (descending)
   - Used by: Status polling

### Clip Collection Indexes

1. **Video Clips Query** (for clip grid)

   - Collection: `users/{uid}/videos/{video_id}/clips`
   - Fields: `created_at` (ascending) or `priority` (ascending)
   - Used by: `list_video_clips()`

2. **Style Filter Query** (for filtering by style)

   - Collection: `users/{uid}/videos/{video_id}/clips`
   - Fields: `style`, `created_at` (ascending)
   - Used by: Style filtering in UI

3. **Scene Filter Query** (for filtering by scene)
   - Collection: `users/{uid}/videos/{video_id}/clips`
   - Fields: `scene_id`, `created_at` (ascending)
   - Used by: Scene-based filtering

## Performance Benefits

### Before (R2-based)

- **List Clips**: ~500-2000ms (paginated R2 list + parse filenames)
- **Get Video Info**: ~1000-3000ms (load highlights.json + list clips)
- **Filter by Style**: ~1000-3000ms (list all + filter in memory)
- **Count Clips**: ~500-2000ms (list all + count)

### After (Firestore-based)

- **List Clips**: ~50-200ms (single Firestore query, indexed)
- **Get Video Info**: ~100-300ms (Firestore query + highlights.json only if needed)
- **Filter by Style**: ~50-200ms (indexed Firestore query)
- **Count Clips**: ~10-50ms (Firestore count query or cached field)

**Expected Performance Improvement: 5-10x faster**

## Migration Strategy

1. **Phase 1**: Write to both Firestore and R2 (dual-write)
2. **Phase 2**: Read from Firestore, fallback to R2 if missing
3. **Phase 3**: Migrate existing data to Firestore
4. **Phase 4**: Remove R2 listing logic (keep only file operations)

## Data Consistency

- **Write Path**: Write to Firestore first, then upload to R2
- **Read Path**: Read from Firestore, generate presigned URLs from R2
- **Error Handling**: If Firestore write fails, retry; if R2 upload fails, mark clip as failed in Firestore
- **Cleanup**: When deleting video, delete Firestore docs first, then R2 files
- **Transactions**: Use Firestore transactions for atomic operations (e.g., creating video + initial clips)

## Architecture Improvements

### Type Safety

- **Pydantic Models**: `ClipMetadata` and `VideoMetadata` models provide runtime validation
- **Type Hints**: Full type annotations throughout repository layer
- **Validation**: Input validation at repository boundaries

### Error Handling

- **Custom Exceptions**: Hierarchical exception structure (`RepositoryError` → `ClipRepositoryError`, etc.)
- **Error Context**: Detailed error messages with context
- **Logging**: Comprehensive logging at appropriate levels

### Performance

- **Batch Operations**: Efficient batch writes (up to 500 operations)
- **Transactions**: Atomic operations for consistency
- **Query Optimization**: Indexed queries with proper ordering
- **Count Queries**: Efficient counting (can use Firestore count API when available)

### Security

- **Input Validation**: All inputs validated before database operations
- **User Isolation**: User ID validation ensures data isolation
- **Size Limits**: Field length limits prevent DoS attacks
- **Sanitization**: Data sanitization before storage

### Testability

- **Repository Pattern**: Clean interfaces for easy mocking
- **Dependency Injection**: Repositories can be injected for testing
- **Transaction Support**: Testable transaction behavior
