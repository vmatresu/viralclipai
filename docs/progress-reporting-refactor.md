# Progress Reporting System Refactor

## Technical Specification Document

**Version:** 1.0
**Date:** December 2024
**Status:** Implementation Ready

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current Architecture Analysis](#2-current-architecture-analysis)
3. [Problem Statement](#3-problem-statement)
4. [Solution Architecture](#4-solution-architecture)
5. [Detailed Implementation Plan](#5-detailed-implementation-plan)
6. [API Specifications](#6-api-specifications)
7. [Data Models](#7-data-models)
8. [Frontend Architecture](#8-frontend-architecture)
9. [Migration Strategy](#9-migration-strategy)
10. [Testing Plan](#10-testing-plan)

---

## 1. Executive Summary

### Objective
Refactor the video processing progress reporting system to be resilient, recoverable, and production-ready. The new system will handle worker crashes, network disconnections, page refreshes, and stale job states gracefully.

### Key Improvements
- **Heartbeat-based health monitoring** for worker processes
- **Persistent progress history** in Redis for recovery
- **Hybrid WebSocket + polling** for reliable updates
- **Automatic stale job detection** and recovery
- **Robust frontend state management** with reconnection logic

### Success Criteria
- No stuck "processing" states in Firebase
- Progress survives page refresh
- Automatic recovery from worker crashes within 60 seconds
- Users can always see current job status via polling fallback

---

## 2. Current Architecture Analysis

### Current Data Flow
```
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│ Frontend │────►│   API    │────►│  Redis   │────►│  Worker  │
│   (WS)   │◄────│  (WS)    │◄────│ Pub/Sub  │◄────│          │
└──────────┘     └──────────┘     └──────────┘     └──────────┘
                       │
                       ▼
                 ┌──────────┐
                 │ Firebase │
                 │(status)  │
                 └──────────┘
```

### Current Components

#### Backend (`vclip-api/src/ws.rs`)
- WebSocket handlers for `/ws/process` and `/ws/reprocess`
- Rate limiting per user (3 concurrent, 5s interval)
- Subscribes to Redis Pub/Sub channel `progress:{job_id}`
- 30-second heartbeat ping/pong for connection keep-alive
- Falls back to Firebase status check if `Done` message not received

#### Backend (`vclip-queue/src/progress.rs`)
- `ProgressChannel` using Redis Pub/Sub
- Publishes: `log`, `progress`, `clip_uploaded`, `done`, `error`, `clip_progress`, `scene_started`, `scene_completed`, `style_omitted`
- **No message persistence** - messages lost if no subscriber

#### Frontend (`web/lib/processing-context.tsx`)
- React context for global processing state
- Persists to localStorage with 24-hour expiry
- Tracks: `status`, `progress`, `logs`, `clipsCompleted`, `totalClips`
- **Scene progress not persisted** - lost on refresh

#### Frontend (`web/components/ProcessingClient/`)
- Creates WebSocket connection on form submit
- Handles messages via `messageHandler.ts`
- No reconnection logic
- No polling fallback

### Current Limitations

| Issue | Impact | Root Cause |
|-------|--------|------------|
| Stuck "processing" status | Cannot start new jobs | No timeout/heartbeat detection |
| Lost progress on refresh | Poor UX, user confusion | Scene progress in memory only |
| WebSocket disconnect = blind | No status updates | No polling fallback |
| Worker crash = eternal wait | Users wait forever | No heartbeat mechanism |
| Race condition on reconnect | Duplicate `Done` messages | No event deduplication |

---

## 3. Problem Statement

### Primary Issues

#### 3.1 No Worker Health Monitoring
```
Worker crashes during processing
    ↓
Redis Pub/Sub subscription times out
    ↓
API WebSocket handler exits without sending Done/Error
    ↓
Firebase status remains "processing"
    ↓
User cannot initiate new jobs (blocked by is_video_processing check)
```

#### 3.2 Ephemeral Progress Events
```
Client disconnects momentarily
    ↓
Worker publishes progress events
    ↓
No subscriber = events lost forever
    ↓
Client reconnects
    ↓
Client has no way to get missed events
```

#### 3.3 No Recovery Mechanism
```
Page refresh during processing
    ↓
WebSocket connection closed
    ↓
New page load has no knowledge of active job
    ↓
Processing context shows stale/no data
    ↓
User sees blank state, thinks processing failed
```

---

## 4. Solution Architecture

### New Data Flow
```
┌─────────────────────────────────────────────────────────────────────────┐
│                              FRONTEND                                    │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐ │
│  │ProgressMgr  │──►│ WebSocket   │   │ Polling     │   │ Processing  │ │
│  │ (orchestr.) │   │ (primary)   │   │ (fallback)  │   │ Context     │ │
│  └─────────────┘   └─────────────┘   └─────────────┘   └─────────────┘ │
│         │                │                 │                  │         │
│         └────────────────┴─────────────────┴──────────────────┘         │
│                                    │                                     │
│                           localStorage + IndexedDB                       │
└─────────────────────────────────────────────────────────────────────────┘
                                     │
                    ┌────────────────┴────────────────┐
                    ▼                                 ▼
            ┌─────────────┐                   ┌─────────────┐
            │ WebSocket   │                   │ REST API    │
            │ /ws/process │                   │ /jobs/:id   │
            └─────────────┘                   └─────────────┘
                    │                                 │
                    └────────────────┬────────────────┘
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                              BACKEND API                                 │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐                   │
│  │ WS Handler  │   │ Job Status  │   │ Stale Job   │                   │
│  │             │   │ Handler     │   │ Detector    │                   │
│  └─────────────┘   └─────────────┘   └─────────────┘                   │
│         │                 │                 │                           │
│         └─────────────────┴─────────────────┘                           │
│                           │                                              │
│                           ▼                                              │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                           REDIS                                    │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │  │
│  │  │ Pub/Sub    │  │ Progress   │  │ Heartbeat  │  │ Job Status │  │  │
│  │  │ progress:* │  │ History    │  │ Keys       │  │ Cache      │  │  │
│  │  │ (realtime) │  │ (ZSET)     │  │ (STRING)   │  │ (HASH)     │  │  │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                              WORKER                                      │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐                   │
│  │ Job         │──►│ Progress    │──►│ Heartbeat   │                   │
│  │ Processor   │   │ Publisher   │   │ Emitter     │                   │
│  └─────────────┘   └─────────────┘   └─────────────┘                   │
│                                            │                            │
│                                     Every 10 seconds                    │
└─────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

#### 4.1 Dual-Write Progress Events
Every progress event is:
1. Published to Redis Pub/Sub (real-time delivery)
2. Persisted to Redis Sorted Set (recovery/history)

#### 4.2 Worker Heartbeat Pattern
```
Worker Processing Loop:
┌─────────────────────────────────────────┐
│  while processing:                       │
│    do_work_chunk()                       │
│    if time_since_last_heartbeat > 10s:  │
│      publish_heartbeat()                 │
│      SET heartbeat:{job_id} NOW EX 60   │
└─────────────────────────────────────────┘
```

#### 4.3 Stale Job Detection
```
Background Task (every 30s):
┌─────────────────────────────────────────┐
│  for job in active_processing_jobs:     │
│    heartbeat = GET heartbeat:{job_id}   │
│    if heartbeat is None or expired:     │
│      mark_job_failed("Worker timeout")  │
│      update_firebase_status("failed")   │
│      publish_error_event()              │
└─────────────────────────────────────────┘
```

#### 4.4 Frontend Hybrid Strategy
```
Connection State Machine:
                    ┌──────────────┐
                    │   INITIAL    │
                    └──────┬───────┘
                           │ connect()
                           ▼
                    ┌──────────────┐
         ┌─────────│  CONNECTING  │─────────┐
         │         └──────────────┘         │
         │ success              │ failure   │
         ▼                      ▼           │
  ┌──────────────┐      ┌──────────────┐   │
  │  CONNECTED   │      │   POLLING    │◄──┘
  │  (WebSocket) │      │  (fallback)  │
  └──────┬───────┘      └──────┬───────┘
         │ disconnect          │ ws available
         └──────────┬──────────┘
                    ▼
             ┌──────────────┐
             │ RECONNECTING │
             │ (exp backoff)│
             └──────────────┘
```

---

## 5. Detailed Implementation Plan

### Phase 1: Backend Infrastructure (Priority: Critical)

#### 5.1 New Redis Data Structures

**File: `backend/crates/vclip-queue/src/progress.rs`**

```rust
// New Redis key patterns
const HEARTBEAT_KEY_PREFIX: &str = "heartbeat:";      // heartbeat:{job_id}
const PROGRESS_HISTORY_PREFIX: &str = "progress:history:"; // progress:history:{job_id}
const JOB_STATUS_PREFIX: &str = "job:status:";        // job:status:{job_id}

// TTLs
const HEARTBEAT_TTL_SECS: u64 = 60;           // Job considered dead after 60s no heartbeat
const PROGRESS_HISTORY_TTL_SECS: u64 = 3600;  // Keep progress history for 1 hour
const JOB_STATUS_TTL_SECS: u64 = 86400;       // Keep job status cache for 24 hours
```

#### 5.2 Enhanced ProgressChannel

```rust
impl ProgressChannel {
    /// Publish progress event with persistence
    pub async fn publish_with_history(&self, event: &ProgressEvent) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let channel = Self::channel_name(&event.job_id);
        let history_key = format!("{}{}", PROGRESS_HISTORY_PREFIX, event.job_id);
        let payload = serde_json::to_string(event)?;
        let timestamp = chrono::Utc::now().timestamp_millis() as f64;

        // Dual-write: Pub/Sub + Sorted Set
        redis::pipe()
            .publish(&channel, &payload)
            .zadd(&history_key, &payload, timestamp)
            .expire(&history_key, PROGRESS_HISTORY_TTL_SECS as i64)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Update worker heartbeat
    pub async fn heartbeat(&self, job_id: &JobId) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", HEARTBEAT_KEY_PREFIX, job_id);
        let now = chrono::Utc::now().timestamp();

        conn.set_ex(&key, now, HEARTBEAT_TTL_SECS).await?;
        Ok(())
    }

    /// Check if job has active heartbeat
    pub async fn is_alive(&self, job_id: &JobId) -> QueueResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", HEARTBEAT_KEY_PREFIX, job_id);

        let exists: bool = conn.exists(&key).await?;
        Ok(exists)
    }

    /// Get progress history since a given timestamp
    pub async fn get_history_since(
        &self,
        job_id: &JobId,
        since_ms: i64,
    ) -> QueueResult<Vec<ProgressEvent>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", PROGRESS_HISTORY_PREFIX, job_id);

        let events: Vec<String> = conn
            .zrangebyscore(&key, since_ms as f64, "+inf")
            .await?;

        let parsed: Vec<ProgressEvent> = events
            .into_iter()
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect();

        Ok(parsed)
    }

    /// Update job status cache
    pub async fn update_job_status(&self, job_id: &JobId, status: &JobStatusCache) -> QueueResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", JOB_STATUS_PREFIX, job_id);
        let payload = serde_json::to_string(status)?;

        conn.set_ex(&key, payload, JOB_STATUS_TTL_SECS).await?;
        Ok(())
    }

    /// Get cached job status
    pub async fn get_job_status(&self, job_id: &JobId) -> QueueResult<Option<JobStatusCache>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}{}", JOB_STATUS_PREFIX, job_id);

        let value: Option<String> = conn.get(&key).await?;
        Ok(value.and_then(|s| serde_json::from_str(&s).ok()))
    }
}
```

#### 5.3 Job Status Cache Model

**File: `backend/crates/vclip-models/src/job_status.rs`** (new file)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Cached job status for fast polling queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusCache {
    pub job_id: String,
    pub video_id: String,
    pub user_id: String,
    pub status: JobStatus,
    pub progress: u8,
    pub clips_completed: u32,
    pub clips_total: u32,
    pub current_step: Option<String>,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Sequence number for event ordering
    pub event_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Processing,
    Completed,
    Failed,
    Stale,  // New status: worker stopped responding
}

impl JobStatusCache {
    pub fn new(job_id: &str, video_id: &str, user_id: &str) -> Self {
        let now = Utc::now();
        Self {
            job_id: job_id.to_string(),
            video_id: video_id.to_string(),
            user_id: user_id.to_string(),
            status: JobStatus::Queued,
            progress: 0,
            clips_completed: 0,
            clips_total: 0,
            current_step: None,
            error_message: None,
            started_at: now,
            updated_at: now,
            last_heartbeat: None,
            event_seq: 0,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, JobStatus::Completed | JobStatus::Failed)
    }
}
```

### Phase 2: Worker Heartbeat Integration

#### 5.4 Worker Heartbeat Emission

**File: `backend/crates/vclip-worker/src/processor.rs`**

Add heartbeat emission to the main processing loop:

```rust
use std::time::{Duration, Instant};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

impl JobProcessor {
    pub async fn process_with_heartbeat<F, Fut>(
        &self,
        job_id: &JobId,
        process_fn: F,
    ) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut last_heartbeat = Instant::now();

        // Initial heartbeat
        self.progress.heartbeat(job_id).await.ok();

        // Spawn heartbeat task
        let progress = self.progress.clone();
        let job_id_clone = job_id.clone();
        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
            loop {
                interval.tick().await;
                if progress.heartbeat(&job_id_clone).await.is_err() {
                    break;
                }
            }
        });

        // Run the actual processing
        let result = process_fn().await;

        // Stop heartbeat task
        heartbeat_handle.abort();

        result
    }
}
```

### Phase 3: REST API for Job Status

#### 5.5 Job Status Endpoint

**File: `backend/crates/vclip-api/src/handlers/jobs.rs`** (new file)

```rust
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct GetJobStatusQuery {
    /// Get events since this timestamp (milliseconds)
    #[serde(default)]
    pub since: Option<i64>,
    /// Include full event history
    #[serde(default)]
    pub include_history: bool,
}

#[derive(Debug, Serialize)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub video_id: String,
    pub status: String,
    pub progress: u8,
    pub clips_completed: u32,
    pub clips_total: u32,
    pub current_step: Option<String>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    pub last_heartbeat: Option<String>,
    pub is_stale: bool,
    /// Recent progress events (if include_history=true or since provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<serde_json::Value>>,
    /// Event sequence for client sync
    pub event_seq: u64,
}

/// GET /api/jobs/:job_id/status
pub async fn get_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<GetJobStatusQuery>,
    user: AuthUser,
) -> ApiResult<Json<JobStatusResponse>> {
    // Get cached status from Redis
    let status = state
        .progress
        .get_job_status(&job_id.into())
        .await?
        .ok_or_else(|| ApiError::NotFound("Job not found".into()))?;

    // Verify ownership
    if status.user_id != user.uid {
        return Err(ApiError::Forbidden("Access denied".into()));
    }

    // Check if stale (no heartbeat for > 60s and not terminal)
    let is_stale = !status.is_terminal() && {
        match status.last_heartbeat {
            Some(hb) => (Utc::now() - hb).num_seconds() > 60,
            None => (Utc::now() - status.started_at).num_seconds() > 120, // Grace period for startup
        }
    };

    // Get event history if requested
    let events = if query.include_history || query.since.is_some() {
        let since = query.since.unwrap_or(0);
        let history = state.progress.get_history_since(&job_id.into(), since).await?;
        Some(history.into_iter().map(|e| serde_json::to_value(&e.message).unwrap()).collect())
    } else {
        None
    };

    Ok(Json(JobStatusResponse {
        job_id: status.job_id,
        video_id: status.video_id,
        status: format!("{:?}", status.status).to_lowercase(),
        progress: status.progress,
        clips_completed: status.clips_completed,
        clips_total: status.clips_total,
        current_step: status.current_step,
        error_message: status.error_message,
        started_at: status.started_at.to_rfc3339(),
        updated_at: status.updated_at.to_rfc3339(),
        last_heartbeat: status.last_heartbeat.map(|h| h.to_rfc3339()),
        is_stale,
        events,
        event_seq: status.event_seq,
    }))
}
```

### Phase 4: Stale Job Detection

#### 5.6 Background Stale Job Detector

**File: `backend/crates/vclip-api/src/services/stale_job_detector.rs`** (new file)

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn, error};

use vclip_firestore::VideoRepository;
use vclip_models::{VideoStatus, JobStatus};
use vclip_queue::ProgressChannel;

const DETECTION_INTERVAL: Duration = Duration::from_secs(30);
const STALE_THRESHOLD_SECS: i64 = 60;

pub struct StaleJobDetector {
    progress: Arc<ProgressChannel>,
    firestore: Arc<vclip_firestore::FirestoreClient>,
}

impl StaleJobDetector {
    pub fn new(progress: Arc<ProgressChannel>, firestore: Arc<vclip_firestore::FirestoreClient>) -> Self {
        Self { progress, firestore }
    }

    /// Start the background detection loop
    pub async fn run(&self) {
        let mut ticker = interval(DETECTION_INTERVAL);

        loop {
            ticker.tick().await;
            if let Err(e) = self.detect_and_recover().await {
                error!("Stale job detection error: {}", e);
            }
        }
    }

    async fn detect_and_recover(&self) -> anyhow::Result<()> {
        // Get all active jobs from Redis (keys matching job:status:*)
        let active_jobs = self.progress.get_active_jobs().await?;

        for job_status in active_jobs {
            if job_status.is_terminal() {
                continue;
            }

            // Check heartbeat
            let is_alive = self.progress.is_alive(&job_status.job_id.clone().into()).await?;

            if !is_alive {
                let age_secs = (chrono::Utc::now() - job_status.started_at).num_seconds();

                // Grace period: don't mark as stale if job just started
                if age_secs < 120 {
                    continue;
                }

                warn!(
                    job_id = %job_status.job_id,
                    video_id = %job_status.video_id,
                    "Detected stale job (no heartbeat), marking as failed"
                );

                // Update job status cache
                let mut updated = job_status.clone();
                updated.status = JobStatus::Failed;
                updated.error_message = Some("Processing timed out. The worker may have crashed. Please try again.".into());
                updated.updated_at = chrono::Utc::now();
                self.progress.update_job_status(&job_status.job_id.clone().into(), &updated).await?;

                // Publish error event so any connected clients get notified
                self.progress.error(
                    &job_status.job_id.clone().into(),
                    "Processing timed out. Please try again.",
                ).await.ok();

                // Update Firebase status
                let video_repo = VideoRepository::new(
                    (*self.firestore).clone(),
                    &job_status.user_id,
                );
                if let Err(e) = video_repo.update_status(
                    &job_status.video_id.clone().into(),
                    VideoStatus::Failed,
                ).await {
                    error!(
                        video_id = %job_status.video_id,
                        "Failed to update Firebase status: {}", e
                    );
                }

                info!(
                    job_id = %job_status.job_id,
                    video_id = %job_status.video_id,
                    "Stale job recovered"
                );
            }
        }

        Ok(())
    }
}
```

### Phase 5: Frontend Refactoring

#### 5.7 Progress Manager (New Core Module)

**File: `web/lib/progress/ProgressManager.ts`** (new file)

```typescript
/**
 * ProgressManager - Robust job progress tracking with hybrid WebSocket + polling
 *
 * Features:
 * - Primary: WebSocket for real-time updates
 * - Fallback: REST polling when WebSocket unavailable
 * - Recovery: Fetch missed events on reconnect
 * - Persistence: Scene progress survives refresh
 */

import { EventEmitter } from 'events';

export interface ProgressManagerConfig {
  apiBaseUrl: string;
  wsBaseUrl: string;
  pollIntervalMs: number;       // Default: 3000
  reconnectDelayMs: number;     // Initial reconnect delay: 1000
  maxReconnectDelayMs: number;  // Max reconnect delay: 30000
  staleThresholdMs: number;     // Consider stale after: 60000
}

export interface JobProgress {
  jobId: string;
  videoId: string;
  status: 'queued' | 'processing' | 'completed' | 'failed' | 'stale';
  progress: number;
  clipsCompleted: number;
  clipsTotal: number;
  currentStep?: string;
  errorMessage?: string;
  lastUpdate: number;
  lastHeartbeat?: number;
  eventSeq: number;
}

export type ProgressEvent =
  | { type: 'progress'; value: number }
  | { type: 'log'; message: string }
  | { type: 'error'; message: string; details?: string }
  | { type: 'done'; videoId: string }
  | { type: 'clip_uploaded'; videoId: string; clipCount: number; totalClips: number }
  | { type: 'clip_progress'; sceneId: number; style: string; step: string; details?: string }
  | { type: 'scene_started'; sceneId: number; sceneTitle: string; styleCount: number }
  | { type: 'scene_completed'; sceneId: number; clipsCompleted: number; clipsFailed: number }
  | { type: 'connection_state'; state: ConnectionState };

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'polling' | 'reconnecting';

export class ProgressManager extends EventEmitter {
  private config: ProgressManagerConfig;
  private ws: WebSocket | null = null;
  private pollInterval: NodeJS.Timeout | null = null;
  private reconnectTimeout: NodeJS.Timeout | null = null;
  private reconnectDelay: number;
  private connectionState: ConnectionState = 'disconnected';
  private activeJobId: string | null = null;
  private lastEventSeq: number = 0;
  private token: string | null = null;

  constructor(config: Partial<ProgressManagerConfig> = {}) {
    super();
    this.config = {
      apiBaseUrl: process.env.NEXT_PUBLIC_API_BASE_URL || '',
      wsBaseUrl: '',
      pollIntervalMs: 3000,
      reconnectDelayMs: 1000,
      maxReconnectDelayMs: 30000,
      staleThresholdMs: 60000,
      ...config,
    };
    this.reconnectDelay = this.config.reconnectDelayMs;
  }

  /**
   * Start tracking a job with WebSocket + polling fallback
   */
  async startTracking(jobId: string, token: string): Promise<void> {
    this.activeJobId = jobId;
    this.token = token;
    this.lastEventSeq = 0;

    // Try WebSocket first
    this.connectWebSocket();
  }

  /**
   * Stop tracking and cleanup
   */
  stopTracking(): void {
    this.activeJobId = null;
    this.token = null;
    this.cleanupWebSocket();
    this.stopPolling();
    this.setConnectionState('disconnected');
  }

  /**
   * Get current job status via REST API
   */
  async fetchStatus(includeHistory = false): Promise<JobProgress | null> {
    if (!this.activeJobId || !this.token) return null;

    try {
      const url = new URL(`${this.config.apiBaseUrl}/api/jobs/${this.activeJobId}/status`);
      if (includeHistory) {
        url.searchParams.set('include_history', 'true');
      }
      if (this.lastEventSeq > 0) {
        url.searchParams.set('since', this.lastEventSeq.toString());
      }

      const response = await fetch(url.toString(), {
        headers: {
          'Authorization': `Bearer ${this.token}`,
        },
      });

      if (!response.ok) {
        throw new Error(`Status fetch failed: ${response.status}`);
      }

      const data = await response.json();
      this.lastEventSeq = data.event_seq;

      // Emit any new events from history
      if (data.events) {
        for (const event of data.events) {
          this.emitProgressEvent(event);
        }
      }

      return {
        jobId: data.job_id,
        videoId: data.video_id,
        status: data.is_stale ? 'stale' : data.status,
        progress: data.progress,
        clipsCompleted: data.clips_completed,
        clipsTotal: data.clips_total,
        currentStep: data.current_step,
        errorMessage: data.error_message,
        lastUpdate: new Date(data.updated_at).getTime(),
        lastHeartbeat: data.last_heartbeat ? new Date(data.last_heartbeat).getTime() : undefined,
        eventSeq: data.event_seq,
      };
    } catch (error) {
      console.error('Failed to fetch job status:', error);
      return null;
    }
  }

  private connectWebSocket(): void {
    if (!this.activeJobId || !this.token) return;

    this.setConnectionState('connecting');

    try {
      const wsUrl = this.getWebSocketUrl();
      this.ws = new WebSocket(wsUrl);

      this.ws.onopen = () => {
        this.setConnectionState('connected');
        this.reconnectDelay = this.config.reconnectDelayMs; // Reset backoff
        this.stopPolling(); // Stop polling when WS connected

        // Send auth message
        this.ws?.send(JSON.stringify({
          type: 'subscribe',
          job_id: this.activeJobId,
          token: this.token,
          last_event_seq: this.lastEventSeq,
        }));
      };

      this.ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          this.handleWebSocketMessage(data);
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e);
        }
      };

      this.ws.onclose = () => {
        this.ws = null;
        this.handleDisconnect();
      };

      this.ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        this.ws?.close();
      };
    } catch (error) {
      console.error('Failed to create WebSocket:', error);
      this.startPolling();
    }
  }

  private handleDisconnect(): void {
    // If job is complete, don't reconnect
    if (!this.activeJobId) {
      this.setConnectionState('disconnected');
      return;
    }

    this.setConnectionState('reconnecting');

    // Start polling as fallback immediately
    this.startPolling();

    // Schedule WebSocket reconnection with exponential backoff
    this.reconnectTimeout = setTimeout(() => {
      this.connectWebSocket();
      this.reconnectDelay = Math.min(
        this.reconnectDelay * 2,
        this.config.maxReconnectDelayMs
      );
    }, this.reconnectDelay);
  }

  private startPolling(): void {
    if (this.pollInterval) return;

    this.setConnectionState('polling');

    // Immediate fetch on start
    this.fetchStatus(true);

    this.pollInterval = setInterval(async () => {
      const status = await this.fetchStatus();
      if (status) {
        // Emit status update
        this.emit('status', status);

        // Check for terminal state
        if (status.status === 'completed' || status.status === 'failed') {
          this.stopTracking();
        }
      }
    }, this.config.pollIntervalMs);
  }

  private stopPolling(): void {
    if (this.pollInterval) {
      clearInterval(this.pollInterval);
      this.pollInterval = null;
    }
  }

  private handleWebSocketMessage(data: any): void {
    // Update event sequence
    if (data.event_seq) {
      this.lastEventSeq = Math.max(this.lastEventSeq, data.event_seq);
    }

    this.emitProgressEvent(data);

    // Check for terminal state
    if (data.type === 'done' || data.type === 'error') {
      this.stopTracking();
    }
  }

  private emitProgressEvent(data: any): void {
    const event = this.normalizeEvent(data);
    if (event) {
      this.emit('event', event);
    }
  }

  private normalizeEvent(data: any): ProgressEvent | null {
    switch (data.type) {
      case 'log':
        return { type: 'log', message: data.message };
      case 'progress':
        return { type: 'progress', value: data.value };
      case 'error':
        return { type: 'error', message: data.message, details: data.details };
      case 'done':
        return { type: 'done', videoId: data.video_id };
      case 'clip_uploaded':
        return {
          type: 'clip_uploaded',
          videoId: data.video_id,
          clipCount: data.clip_count,
          totalClips: data.total_clips,
        };
      case 'clip_progress':
        return {
          type: 'clip_progress',
          sceneId: data.scene_id,
          style: data.style,
          step: data.step,
          details: data.details,
        };
      case 'scene_started':
        return {
          type: 'scene_started',
          sceneId: data.scene_id,
          sceneTitle: data.scene_title,
          styleCount: data.style_count,
        };
      case 'scene_completed':
        return {
          type: 'scene_completed',
          sceneId: data.scene_id,
          clipsCompleted: data.clips_completed,
          clipsFailed: data.clips_failed,
        };
      default:
        return null;
    }
  }

  private setConnectionState(state: ConnectionState): void {
    if (this.connectionState !== state) {
      this.connectionState = state;
      this.emit('event', { type: 'connection_state', state });
    }
  }

  private cleanupWebSocket(): void {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  private getWebSocketUrl(): string {
    const base = this.config.apiBaseUrl || window.location.origin;
    const protocol = base.startsWith('https') ? 'wss' : 'ws';
    const host = base.replace(/^https?:\/\//, '');
    return `${protocol}://${host}/ws/progress`;
  }

  get currentState(): ConnectionState {
    return this.connectionState;
  }
}

// Singleton instance
let progressManager: ProgressManager | null = null;

export function getProgressManager(): ProgressManager {
  if (!progressManager) {
    progressManager = new ProgressManager();
  }
  return progressManager;
}
```

#### 5.8 Enhanced Processing Context

**File: `web/lib/processing-context.tsx`** (modifications)

```typescript
// Add to ProcessingJob interface
export interface ProcessingJob {
  // ... existing fields ...
  jobId?: string;           // Backend job ID for status polling
  lastHeartbeat?: number;   // Last heartbeat timestamp
  connectionState?: 'connected' | 'polling' | 'reconnecting' | 'disconnected';
  sceneProgress?: Map<number, SceneProgress>; // Persisted scene progress
}

// Add new context methods
interface ProcessingContextValue {
  // ... existing methods ...
  setJobId: (videoId: string, jobId: string) => void;
  updateConnectionState: (videoId: string, state: ConnectionState) => void;
  getSceneProgress: (videoId: string) => Map<number, SceneProgress> | undefined;
  updateSceneProgress: (videoId: string, sceneId: number, progress: Partial<SceneProgress>) => void;
  recoverFromStorage: (videoId: string) => ProcessingJob | undefined;
  checkAndRecoverStaleJobs: () => Promise<void>;
}

// New helper for scene progress persistence
function saveSceneProgressToStorage(videoId: string, progress: Map<number, SceneProgress>) {
  try {
    const key = `vclip_scene_progress_${videoId}`;
    const serializable = Array.from(progress.entries());
    localStorage.setItem(key, JSON.stringify(serializable));
  } catch (e) {
    console.error('Failed to save scene progress:', e);
  }
}

function loadSceneProgressFromStorage(videoId: string): Map<number, SceneProgress> {
  try {
    const key = `vclip_scene_progress_${videoId}`;
    const stored = localStorage.getItem(key);
    if (!stored) return new Map();

    const entries: [number, SceneProgress][] = JSON.parse(stored);
    return new Map(entries);
  } catch (e) {
    return new Map();
  }
}
```

---

## 6. API Specifications

### 6.1 REST Endpoints

#### GET /api/jobs/:job_id/status

**Description:** Get current job status and optionally progress history

**Authentication:** Bearer token required

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `since` | number | 0 | Get events since this timestamp (ms) |
| `include_history` | boolean | false | Include full event history |

**Response:**
```json
{
  "job_id": "abc123",
  "video_id": "def456",
  "status": "processing",
  "progress": 45,
  "clips_completed": 3,
  "clips_total": 10,
  "current_step": "Rendering scene 2",
  "error_message": null,
  "started_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:31:00Z",
  "last_heartbeat": "2024-01-15T10:31:00Z",
  "is_stale": false,
  "events": [...],
  "event_seq": 42
}
```

**Error Responses:**
- `401 Unauthorized` - Invalid/missing token
- `403 Forbidden` - Not job owner
- `404 Not Found` - Job not found

### 6.2 WebSocket Endpoints

#### WS /ws/progress

**Description:** Real-time progress stream with subscription model

**Subscription Message:**
```json
{
  "type": "subscribe",
  "job_id": "abc123",
  "token": "firebase_id_token",
  "last_event_seq": 0
}
```

**Server Messages:**
```json
// Progress update
{ "type": "progress", "value": 45, "event_seq": 10 }

// Log message
{ "type": "log", "message": "Processing scene 1...", "event_seq": 11 }

// Heartbeat (server sends every 30s)
{ "type": "heartbeat", "timestamp": 1705312260000 }

// Done
{ "type": "done", "video_id": "def456", "event_seq": 100 }

// Error
{ "type": "error", "message": "Processing failed", "details": "...", "event_seq": 100 }
```

---

## 7. Data Models

### 7.1 Redis Key Schema

| Key Pattern | Type | TTL | Description |
|-------------|------|-----|-------------|
| `heartbeat:{job_id}` | STRING | 60s | Worker heartbeat timestamp |
| `progress:history:{job_id}` | ZSET | 1h | Progress events (score=timestamp) |
| `job:status:{job_id}` | STRING | 24h | JSON job status cache |
| `progress:{job_id}` | PUBSUB | - | Real-time progress channel |

### 7.2 JobStatusCache Schema

```rust
pub struct JobStatusCache {
    pub job_id: String,
    pub video_id: String,
    pub user_id: String,
    pub status: JobStatus,          // queued|processing|completed|failed|stale
    pub progress: u8,               // 0-100
    pub clips_completed: u32,
    pub clips_total: u32,
    pub current_step: Option<String>,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub event_seq: u64,             // For client synchronization
}
```

### 7.3 ProgressEvent Schema

```rust
pub struct ProgressEvent {
    pub job_id: JobId,
    pub message: WsMessage,
    pub timestamp: DateTime<Utc>,
    pub seq: u64,                   // Sequence number for ordering
}
```

---

## 8. Frontend Architecture

### 8.1 Component Hierarchy

```
App
├── ProcessingProvider (context)
│   └── ProgressManager (singleton)
│       ├── WebSocket connection
│       └── REST polling fallback
│
├── ProcessingClient
│   ├── VideoForm
│   ├── DetailedProcessingStatus
│   │   ├── ProgressBar
│   │   ├── LogViewer
│   │   └── SceneProgressCards
│   │       └── SceneProgressCard (per scene)
│   └── Results
│
└── HistoryPage
    └── VideoCard
        └── ProcessingIndicator (mini status)
```

### 8.2 State Flow

```
User submits video
    │
    ▼
ProcessingClient.onSubmit()
    │
    ├─► ProcessingContext.startJob(videoId)
    │       └─► localStorage.setItem()
    │
    └─► ProgressManager.startTracking(jobId, token)
            │
            ├─► WebSocket.connect()
            │       │
            │       ├─► on('message') ──► emit('event')
            │       │                          │
            │       │                          ▼
            │       │                   ProcessingContext.updateJob()
            │       │                          │
            │       │                          ▼
            │       │                   DetailedProcessingStatus (re-render)
            │       │
            │       └─► on('close') ──► startPolling()
            │
            └─► REST polling (fallback)
                    │
                    └─► fetchStatus() ──► emit('event')
```

### 8.3 Recovery Flow

```
Page refresh
    │
    ▼
ProcessingProvider.useEffect()
    │
    ├─► loadJobsFromStorage()
    │       │
    │       ▼
    │   Found active jobs?
    │       │
    │       ├─► Yes: checkAndRecoverStaleJobs()
    │       │           │
    │       │           ├─► For each active job:
    │       │           │       fetchStatus(jobId)
    │       │           │           │
    │       │           │           ├─► Status: completed ──► completeJob()
    │       │           │           ├─► Status: failed ──► failJob()
    │       │           │           ├─► Status: stale ──► failJob("timeout")
    │       │           │           └─► Status: processing ──► reconnect()
    │       │           │
    │       │           └─► ProgressManager.startTracking()
    │       │
    │       └─► No: done
    │
    └─► UI renders with recovered state
```

---

## 9. Migration Strategy

### 9.1 Deployment Order

1. **Backend Phase 1:** Deploy Redis schema changes + enhanced ProgressChannel
   - Zero downtime - additive changes only
   - Old workers continue working (no heartbeat = will be marked stale after 2 min)

2. **Backend Phase 2:** Deploy worker heartbeat integration
   - Rolling deployment - new workers emit heartbeats
   - Old jobs complete normally

3. **Backend Phase 3:** Deploy REST status endpoint + stale detector
   - Enable stale detection only after all workers updated
   - Feature flag: `ENABLE_STALE_DETECTION=true`

4. **Frontend Phase:** Deploy new ProgressManager + context updates
   - Backward compatible - falls back to existing behavior if new endpoints unavailable

### 9.2 Feature Flags

```rust
// Backend
ENABLE_PROGRESS_HISTORY=true      // Enable dual-write to sorted sets
ENABLE_HEARTBEAT=true             // Enable worker heartbeats
ENABLE_STALE_DETECTION=true       // Enable background stale job detector
STALE_THRESHOLD_SECS=60           // Configurable stale threshold
```

```typescript
// Frontend
NEXT_PUBLIC_ENABLE_HYBRID_PROGRESS=true  // Enable new ProgressManager
NEXT_PUBLIC_POLL_INTERVAL_MS=3000        // Polling interval
```

### 9.3 Rollback Plan

1. **Frontend rollback:** Disable `NEXT_PUBLIC_ENABLE_HYBRID_PROGRESS`
   - Immediately reverts to old WebSocket-only behavior

2. **Backend rollback:** Disable `ENABLE_STALE_DETECTION`
   - Stops marking jobs as stale
   - Manual cleanup required for stuck jobs

3. **Full rollback:** Disable all flags
   - System behaves exactly as before
   - Progress history continues accumulating (harmless)

---

## 10. Testing Plan

### 10.1 Unit Tests

**Backend:**
- `ProgressChannel::publish_with_history` - verify dual-write
- `ProgressChannel::get_history_since` - verify range query
- `ProgressChannel::heartbeat` - verify TTL setting
- `StaleJobDetector::detect_and_recover` - verify detection logic
- `JobStatusCache` serialization/deserialization

**Frontend:**
- `ProgressManager.startTracking` - WebSocket connection
- `ProgressManager.fetchStatus` - REST fallback
- `ProgressManager` reconnection with backoff
- Event sequence handling and deduplication

### 10.2 Integration Tests

1. **Happy path:** Submit job → receive all progress → complete
2. **WebSocket disconnect:** Disconnect mid-processing → reconnect → receive missed events
3. **Page refresh:** Refresh during processing → recover state → continue tracking
4. **Worker crash:** Simulate worker death → stale detection → proper failure state
5. **Concurrent jobs:** Multiple jobs processing simultaneously

### 10.3 Load Tests

1. **Connection scaling:** 100 concurrent WebSocket connections
2. **Progress throughput:** 1000 progress events/second per job
3. **History query performance:** 10000 events in sorted set
4. **Stale detection at scale:** 1000 active jobs

### 10.4 Chaos Tests

1. **Redis restart:** Redis goes down and comes back
2. **API restart:** API server restarts mid-processing
3. **Network partition:** Temporary network split between API and worker
4. **Clock skew:** Time drift between services

---

## Appendix A: Monitoring & Alerting

### Metrics to Add

```rust
// Backend metrics
job_heartbeat_total              // Counter: heartbeats emitted
job_heartbeat_age_seconds        // Histogram: time since last heartbeat
stale_jobs_detected_total        // Counter: jobs marked stale
progress_history_size            // Gauge: events in history per job
ws_connections_active            // Gauge: active WS connections
rest_status_requests_total       // Counter: status API calls
```

```typescript
// Frontend metrics
progress_ws_connections_total    // Counter: WS connection attempts
progress_ws_disconnects_total    // Counter: WS disconnections
progress_poll_requests_total     // Counter: polling requests
progress_recovery_total          // Counter: state recoveries after refresh
```

### Alerts

| Alert | Condition | Severity |
|-------|-----------|----------|
| HighStaleJobRate | stale_jobs_detected > 10/min | Warning |
| NoHeartbeats | job_heartbeat_total = 0 for 5min | Critical |
| ProgressHistoryOverflow | progress_history_size > 50000 | Warning |
| WebSocketConnectionSpike | ws_connections_active > 1000 | Warning |

---

## Appendix B: Security Considerations

1. **Job ownership verification:** All status endpoints verify `user_id` matches token
2. **Rate limiting:** Polling endpoint rate-limited to 1 req/sec per user
3. **Event sequence validation:** Reject events with sequence < last_seen
4. **Token expiry:** Re-validate token on WebSocket reconnect
5. **Input sanitization:** All job IDs validated against UUID format
6. **Redis key isolation:** Keys prefixed with job ID, no cross-job access

---

**Document End**
