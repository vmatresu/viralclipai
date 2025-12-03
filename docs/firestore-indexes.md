# Firestore Indexes Configuration

This document describes the required Firestore indexes for optimal query performance.

## Required Indexes

### 1. Videos Collection - User Videos Query

**Collection Path**: `users/{uid}/videos`

**Fields**:
- `created_at` (Descending)

**Purpose**: List user videos ordered by creation date (for history page)

**Usage**: `VideoRepository.list_videos(order_by="created_at")`

---

### 2. Videos Collection - Status Query

**Collection Path**: `users/{uid}/videos`

**Fields**:
- `status` (Ascending)
- `created_at` (Descending)

**Purpose**: Query videos by status (e.g., processing, completed, failed)

**Usage**: `VideoRepository.list_videos(status="processing")`

---

### 3. Clips Collection - Video Clips Query

**Collection Path**: `users/{uid}/videos/{video_id}/clips`

**Fields**:
- `status` (Ascending)
- `priority` (Ascending)
- `created_at` (Ascending)

**Purpose**: List clips for a video, ordered by priority

**Usage**: `ClipRepository.list_clips(status="completed", order_by="priority")`

---

### 4. Clips Collection - Style Filter Query

**Collection Path**: `users/{uid}/videos/{video_id}/clips`

**Fields**:
- `style` (Ascending)
- `created_at` (Ascending)

**Purpose**: Filter clips by style (e.g., "split", "left_focus")

**Usage**: `ClipRepository.list_clips(style="split")`

---

### 5. Clips Collection - Scene Filter Query

**Collection Path**: `users/{uid}/videos/{video_id}/clips`

**Fields**:
- `scene_id` (Ascending)
- `priority` (Ascending)

**Purpose**: Filter clips by scene ID

**Usage**: `ClipRepository.list_clips(scene_id=1)`

---

## Creating Indexes

### Option 1: Firebase Console (Recommended)

1. Go to [Firebase Console](https://console.firebase.google.com/)
2. Select your project
3. Navigate to **Firestore Database** → **Indexes**
4. Click **Create Index**
5. Enter collection path and fields as specified above
6. Click **Create**

### Option 2: Firebase CLI

Create a `firestore.indexes.json` file in your project root:

```json
{
  "indexes": [
    {
      "collectionGroup": "videos",
      "queryScope": "COLLECTION",
      "fields": [
        {
          "fieldPath": "created_at",
          "order": "DESCENDING"
        }
      ]
    },
    {
      "collectionGroup": "videos",
      "queryScope": "COLLECTION",
      "fields": [
        {
          "fieldPath": "status",
          "order": "ASCENDING"
        },
        {
          "fieldPath": "created_at",
          "order": "DESCENDING"
        }
      ]
    },
    {
      "collectionGroup": "clips",
      "queryScope": "COLLECTION",
      "fields": [
        {
          "fieldPath": "status",
          "order": "ASCENDING"
        },
        {
          "fieldPath": "priority",
          "order": "ASCENDING"
        },
        {
          "fieldPath": "created_at",
          "order": "ASCENDING"
        }
      ]
    },
    {
      "collectionGroup": "clips",
      "queryScope": "COLLECTION",
      "fields": [
        {
          "fieldPath": "style",
          "order": "ASCENDING"
        },
        {
          "fieldPath": "created_at",
          "order": "ASCENDING"
        }
      ]
    },
    {
      "collectionGroup": "clips",
      "queryScope": "COLLECTION",
      "fields": [
        {
          "fieldPath": "scene_id",
          "order": "ASCENDING"
        },
        {
          "fieldPath": "priority",
          "order": "ASCENDING"
        }
      ]
    }
  ],
  "fieldOverrides": []
}
```

Then deploy:

```bash
firebase deploy --only firestore:indexes
```

## Index Status

After creating indexes, they will be in **Building** status. This typically takes a few minutes for small collections, but can take longer for large collections.

You can check index status in the Firebase Console under **Firestore Database** → **Indexes**.

## Performance Notes

- **Without indexes**: Queries will fail with an error asking you to create the index
- **With indexes**: Queries are fast (typically 50-200ms for most queries)
- **Index building**: Large collections may take time to build indexes initially
- **Index maintenance**: Firestore automatically maintains indexes as data changes

## Troubleshooting

### Error: "The query requires an index"

If you see this error:
1. Check the error message - it will include a link to create the index
2. Click the link or manually create the index as specified above
3. Wait for the index to finish building
4. Retry the query

### Slow Queries

If queries are slow:
1. Verify indexes are built (not in "Building" status)
2. Check query complexity - simpler queries are faster
3. Consider adding limits to queries
4. Review document size - keep documents under 1MB

## Monitoring

Monitor index usage in Firebase Console:
- **Firestore Database** → **Usage** → **Index Operations**
- Track read/write operations
- Monitor index build times

