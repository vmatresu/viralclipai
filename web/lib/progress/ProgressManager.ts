/**
 * ProgressManager - Robust job progress tracking via REST polling
 *
 * Features:
 * - REST API polling for reliable status updates
 * - Automatic state management and cleanup
 * - Event history recovery after reconnect
 * - Stale job detection support
 */

import { frontendLogger } from "@/lib/logger";

// ============================================================================
// Types
// ============================================================================

export interface ProgressManagerConfig {
  apiBaseUrl: string;
  pollIntervalMs: number; // Default: 3000
  reconnectDelayMs: number; // Initial reconnect delay: 1000
  maxReconnectDelayMs: number; // Max reconnect delay: 30000
  staleThresholdMs: number; // Consider stale after: 60000
}

export interface JobProgress {
  jobId: string;
  videoId: string;
  status: "queued" | "processing" | "completed" | "failed" | "stale";
  progress: number;
  clipsCompleted: number;
  clipsTotal: number;
  currentStep?: string;
  errorMessage?: string;
  lastUpdate: number;
  lastHeartbeat?: number;
  eventSeq: number;
}

export type ProgressEventType =
  | "progress"
  | "log"
  | "error"
  | "done"
  | "clip_uploaded"
  | "clip_progress"
  | "scene_started"
  | "scene_completed"
  | "style_omitted"
  | "connection_state";

export interface ProgressEventBase {
  type: ProgressEventType;
}

export interface ProgressUpdateEvent extends ProgressEventBase {
  type: "progress";
  value: number;
}

export interface LogEvent extends ProgressEventBase {
  type: "log";
  message: string;
}

export interface ErrorEvent extends ProgressEventBase {
  type: "error";
  message: string;
  details?: string;
}

export interface DoneEvent extends ProgressEventBase {
  type: "done";
  videoId: string;
}

export interface ClipUploadedEvent extends ProgressEventBase {
  type: "clip_uploaded";
  videoId: string;
  clipCount: number;
  totalClips: number;
}

export interface ClipProgressEvent extends ProgressEventBase {
  type: "clip_progress";
  sceneId: number;
  style: string;
  step: string;
  details?: string;
}

export interface SceneStartedEvent extends ProgressEventBase {
  type: "scene_started";
  sceneId: number;
  sceneTitle: string;
  styleCount: number;
  startSec?: number;
  durationSec?: number;
}

export interface SceneCompletedEvent extends ProgressEventBase {
  type: "scene_completed";
  sceneId: number;
  clipsCompleted: number;
  clipsFailed: number;
}

export interface StyleOmittedEvent extends ProgressEventBase {
  type: "style_omitted";
  sceneId: number;
  style: string;
  reason: string;
}

export interface ConnectionStateEvent extends ProgressEventBase {
  type: "connection_state";
  state: ConnectionState;
}

export type ProgressEvent =
  | ProgressUpdateEvent
  | LogEvent
  | ErrorEvent
  | DoneEvent
  | ClipUploadedEvent
  | ClipProgressEvent
  | SceneStartedEvent
  | SceneCompletedEvent
  | StyleOmittedEvent
  | ConnectionStateEvent;

export type ConnectionState =
  | "connecting"
  | "connected"
  | "disconnected"
  | "polling"
  | "reconnecting";

export type ProgressEventHandler = (event: ProgressEvent) => void;
export type StatusHandler = (status: JobProgress) => void;

// ============================================================================
// ProgressManager Class
// ============================================================================

const DEFAULT_CONFIG: ProgressManagerConfig = {
  apiBaseUrl: process.env.NEXT_PUBLIC_API_BASE_URL ?? "",
  pollIntervalMs: 3000,
  reconnectDelayMs: 1000,
  maxReconnectDelayMs: 30000,
  staleThresholdMs: 60000,
};

export class ProgressManager {
  private config: ProgressManagerConfig;
  private pollInterval: ReturnType<typeof setInterval> | null = null;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  private reconnectDelay: number;
  private connectionState: ConnectionState = "disconnected";
  private activeJobId: string | null = null;
  private lastEventSeq: number = 0;
  private token: string | null = null;

  private eventHandlers: Set<ProgressEventHandler> = new Set();
  private statusHandlers: Set<StatusHandler> = new Set();

  constructor(config: Partial<ProgressManagerConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.reconnectDelay = this.config.reconnectDelayMs;
  }

  /**
   * Subscribe to progress events
   */
  onEvent(handler: ProgressEventHandler): () => void {
    this.eventHandlers.add(handler);
    return () => this.eventHandlers.delete(handler);
  }

  /**
   * Subscribe to status updates
   */
  onStatus(handler: StatusHandler): () => void {
    this.statusHandlers.add(handler);
    return () => this.statusHandlers.delete(handler);
  }

  /**
   * Start tracking a job with polling
   */
  startTracking(jobId: string, token: string): void {
    this.activeJobId = jobId;
    this.token = token;
    this.lastEventSeq = 0;

    frontendLogger.info(`Starting job tracking: ${jobId}`);

    // Start polling immediately
    this.startPolling();
  }

  /**
   * Stop tracking and cleanup
   */
  stopTracking(): void {
    frontendLogger.info(`Stopping job tracking: ${this.activeJobId}`);
    this.activeJobId = null;
    this.token = null;
    this.stopPolling();
    this.setConnectionState("disconnected");
  }

  /**
   * Get current job status via REST API
   */
  async fetchStatus(includeHistory = false): Promise<JobProgress | null> {
    if (!this.activeJobId || !this.token) return null;

    try {
      const url = new URL(
        `${this.config.apiBaseUrl}/api/jobs/${this.activeJobId}/status`
      );
      if (includeHistory) {
        url.searchParams.set("include_history", "true");
      }
      if (this.lastEventSeq > 0) {
        url.searchParams.set("since", this.lastEventSeq.toString());
      }

      const response = await fetch(url.toString(), {
        headers: {
          Authorization: `Bearer ${this.token}`,
        },
      });

      if (!response.ok) {
        if (response.status === 404) {
          // Job not found - may not be initialized yet
          return null;
        }
        throw new Error(`Status fetch failed: ${response.status}`);
      }

      const data = await response.json();
      this.lastEventSeq = data.event_seq;

      // Emit any new events from history
      if (data.events && Array.isArray(data.events)) {
        for (const event of data.events) {
          this.emitProgressEvent(event);
        }
      }

      const status: JobProgress = {
        jobId: data.job_id,
        videoId: data.video_id,
        status: data.is_stale ? "stale" : data.status,
        progress: data.progress,
        clipsCompleted: data.clips_completed,
        clipsTotal: data.clips_total,
        currentStep: data.current_step,
        errorMessage: data.error_message,
        lastUpdate: new Date(data.updated_at).getTime(),
        lastHeartbeat: data.last_heartbeat
          ? new Date(data.last_heartbeat).getTime()
          : undefined,
        eventSeq: data.event_seq,
      };

      // Emit status to handlers
      for (const handler of this.statusHandlers) {
        handler(status);
      }

      return status;
    } catch (error) {
      frontendLogger.error("Failed to fetch job status:", error);
      return null;
    }
  }

  private startPolling(): void {
    if (this.pollInterval) return;

    this.setConnectionState("polling");

    // Immediate fetch on start
    void this.fetchStatus(true);

    this.pollInterval = setInterval(async () => {
      const status = await this.fetchStatus();
      if (status) {
        // Check for terminal state
        if (status.status === "completed" || status.status === "failed") {
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
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
  }

  private emitProgressEvent(data: Record<string, unknown>): void {
    const event = this.normalizeEvent(data);
    if (event) {
      for (const handler of this.eventHandlers) {
        handler(event);
      }
    }
  }

  private normalizeEvent(data: Record<string, unknown>): ProgressEvent | null {
    const type = data.type as string;

    switch (type) {
      case "log":
        return { type: "log", message: data.message as string };
      case "progress":
        return { type: "progress", value: data.value as number };
      case "error":
        return {
          type: "error",
          message: data.message as string,
          details: data.details as string | undefined,
        };
      case "done":
        return { type: "done", videoId: data.video_id as string };
      case "clip_uploaded":
        return {
          type: "clip_uploaded",
          videoId: data.video_id as string,
          clipCount: data.clip_count as number,
          totalClips: data.total_clips as number,
        };
      case "clip_progress":
        return {
          type: "clip_progress",
          sceneId: data.scene_id as number,
          style: data.style as string,
          step: data.step as string,
          details: data.details as string | undefined,
        };
      case "scene_started":
        return {
          type: "scene_started",
          sceneId: data.scene_id as number,
          sceneTitle: data.scene_title as string,
          styleCount: data.style_count as number,
          startSec: data.start_sec as number | undefined,
          durationSec: data.duration_sec as number | undefined,
        };
      case "scene_completed":
        return {
          type: "scene_completed",
          sceneId: data.scene_id as number,
          clipsCompleted: data.clips_completed as number,
          clipsFailed: data.clips_failed as number,
        };
      case "style_omitted":
        return {
          type: "style_omitted",
          sceneId: data.scene_id as number,
          style: data.style as string,
          reason: data.reason as string,
        };
      default:
        return null;
    }
  }

  private setConnectionState(state: ConnectionState): void {
    if (this.connectionState !== state) {
      this.connectionState = state;
      const event: ConnectionStateEvent = { type: "connection_state", state };
      for (const handler of this.eventHandlers) {
        handler(event);
      }
    }
  }

  get currentState(): ConnectionState {
    return this.connectionState;
  }

  get currentJobId(): string | null {
    return this.activeJobId;
  }
}

// ============================================================================
// Singleton Instance
// ============================================================================

let progressManagerInstance: ProgressManager | null = null;

export function getProgressManager(): ProgressManager {
  progressManagerInstance ??= new ProgressManager();
  return progressManagerInstance;
}

export function resetProgressManager(): void {
  if (progressManagerInstance) {
    progressManagerInstance.stopTracking();
    progressManagerInstance = null;
  }
}
