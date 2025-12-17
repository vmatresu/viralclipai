/**
 * WebSocket client for scene reprocessing with progress tracking.
 *
 * Provides a clean, type-safe interface for reprocessing scenes via WebSocket.
 */

import { invalidateClipsCache } from "@/lib/cache";
import { frontendLogger } from "@/lib/logger";

import { type ClipProcessingStep } from "./types";

export interface ReprocessProgressMessage {
  type: "progress";
  value: number;
}

export interface ReprocessLogMessage {
  type: "log";
  message: string;
}

export interface ReprocessDoneMessage {
  type: "done";
  videoId: string;
}

export interface ReprocessErrorMessage {
  type: "error";
  message: string;
  details?: string;
  timestamp?: string;
}

export interface ReprocessSceneStartedMessage {
  type: "scene_started";
  sceneId: number;
  sceneTitle: string;
  styleCount: number;
  startSec: number;
  durationSec: number;
}

export interface ReprocessSceneCompletedMessage {
  type: "scene_completed";
  sceneId: number;
  clipsCompleted: number;
  clipsFailed: number;
}

export interface ReprocessClipProgressMessage {
  type: "clip_progress";
  sceneId: number;
  style: string;
  step: ClipProcessingStep;
  details?: string;
}

export interface ReprocessClipUploadedMessage {
  type: "clip_uploaded";
  videoId: string;
  clipCount: number;
  totalClips: number;
}

export interface ReprocessStyleOmittedMessage {
  type: "style_omitted";
  sceneId: number;
  style: string;
  reason: string;
}

export type ReprocessMessage =
  | ReprocessProgressMessage
  | ReprocessLogMessage
  | ReprocessDoneMessage
  | ReprocessErrorMessage
  | ReprocessSceneStartedMessage
  | ReprocessSceneCompletedMessage
  | ReprocessClipProgressMessage
  | ReprocessClipUploadedMessage
  | ReprocessStyleOmittedMessage;

export interface ReprocessCallbacks {
  onProgress?: (value: number) => void;
  onLog?: (message: string, timestamp?: string) => void;
  onDone?: (videoId: string) => void;
  onError?: (message: string, details?: string) => void;
  onClose?: () => void;
  onSceneStarted?: (
    sceneId: number,
    sceneTitle: string,
    styleCount: number,
    startSec: number,
    durationSec: number
  ) => void;
  onSceneCompleted?: (
    sceneId: number,
    clipsCompleted: number,
    clipsFailed: number
  ) => void;
  onClipProgress?: (
    sceneId: number,
    style: string,
    step: ClipProcessingStep,
    details?: string
  ) => void;
  onClipUploaded?: (videoId: string, clipCount: number, totalClips: number) => void;
  onStyleOmitted?: (sceneId: number, style: string, reason: string) => void;
}

/** StreamerSplit configuration for user-controlled crop */
export interface StreamerSplitParams {
  position_x: "left" | "center" | "right";
  position_y: "top" | "middle" | "bottom";
  zoom: number;
  static_image_url?: string;
}

export interface ReprocessOptions {
  videoId: string;
  sceneIds: number[];
  styles: string[];
  token: string;
  cropMode?: string;
  targetAspect?: string;
  /** Enable object detection for Cinematic style (default: false) */
  enableObjectDetection?: boolean;
  /** When true, overwrite existing clips instead of skipping them (default: false) */
  overwrite?: boolean;
  /** StreamerSplit parameters for user-controlled crop position/zoom */
  streamerSplitParams?: StreamerSplitParams;
  /** Enable Top Scenes compilation (creates single video with countdown overlay) */
  topScenesCompilation?: boolean;
  /** Cut silent parts from clips using VAD (default: true) */
  cutSilentParts?: boolean;
}

const MAX_MESSAGE_SIZE = 1024 * 1024; // 1MB
const DEFAULT_TIMEOUT = 1800000; // 30 minutes

/**
 * Create WebSocket URL for reprocessing endpoint.
 */
function getReprocessWebSocketUrl(): string {
  const apiBase = process.env.NEXT_PUBLIC_API_BASE_URL ?? window.location.origin;

  // Security: Validate and sanitize URL
  let baseUrl: URL;
  try {
    baseUrl = new URL(apiBase);
    if (baseUrl.protocol !== "http:" && baseUrl.protocol !== "https:") {
      throw new Error("Invalid API protocol");
    }
  } catch {
    throw new Error("Invalid API base URL configuration");
  }

  // Build WebSocket URL securely
  const wsProtocol = baseUrl.protocol === "https:" ? "wss:" : "ws:";
  const wsUrl = `${wsProtocol}//${baseUrl.host}/ws/reprocess`;

  // Validate WebSocket URL
  if (!wsUrl.startsWith("ws://") && !wsUrl.startsWith("wss://")) {
    throw new Error("Invalid WebSocket URL");
  }

  return wsUrl;
}

/**
 * Reprocess scenes via WebSocket with progress tracking.
 *
 * @param options Reprocessing options
 * @param callbacks Event callbacks
 * @returns WebSocket instance for manual control if needed
 */
export function reprocessScenesWebSocket(
  options: ReprocessOptions,
  callbacks: ReprocessCallbacks
): WebSocket {
  const {
    videoId,
    sceneIds,
    styles,
    token,
    cropMode = "none",
    targetAspect = "9:16",
    enableObjectDetection = false,
    overwrite = false,
    streamerSplitParams,
    topScenesCompilation = false,
    cutSilentParts = false,
  } = options;

  // Validation
  if (!videoId || !token) {
    throw new Error("Video ID and token are required");
  }

  if (!sceneIds || sceneIds.length === 0) {
    throw new Error("At least one scene ID is required");
  }

  if (!styles || styles.length === 0) {
    throw new Error("At least one style is required");
  }

  // Security: Validate scene IDs
  if (sceneIds.length > 50) {
    throw new Error("Too many scene IDs. Maximum is 50.");
  }

  for (const id of sceneIds) {
    if (!Number.isInteger(id) || id < 1 || id > 10000) {
      throw new Error(`Invalid scene ID: ${id}`);
    }
  }

  const wsUrl = getReprocessWebSocketUrl();
  const ws = new WebSocket(wsUrl);

  // Set up timeout
  const timeoutId = setTimeout(() => {
    if (ws.readyState === WebSocket.CONNECTING || ws.readyState === WebSocket.OPEN) {
      frontendLogger.warn("WebSocket reprocessing timeout");
      ws.close();
      callbacks.onError?.("Reprocessing timeout. Please try again.");
    }
  }, DEFAULT_TIMEOUT);

  ws.onopen = () => {
    try {
      // Send reprocessing request
      ws.send(
        JSON.stringify({
          token,
          video_id: videoId,
          scene_ids: sceneIds,
          styles,
          crop_mode: cropMode,
          target_aspect: targetAspect,
          enable_object_detection: enableObjectDetection,
          overwrite,
          streamer_split_params: streamerSplitParams,
          top_scenes_compilation: topScenesCompilation,
          cut_silent_parts: cutSilentParts,
        })
      );
    } catch (error) {
      frontendLogger.error("Failed to send reprocessing request", error);
      callbacks.onError?.("Failed to start reprocessing");
      ws.close();
    }
  };

  ws.onmessage = (event) => {
    // Security: Limit message size to prevent DoS
    if (event.data.length > MAX_MESSAGE_SIZE) {
      frontendLogger.error("WebSocket message too large", {
        size: event.data.length,
      });
      ws.close();
      callbacks.onError?.("Message too large");
      return;
    }

    let data: ReprocessMessage;
    try {
      data = JSON.parse(event.data) as ReprocessMessage;
    } catch (error) {
      frontendLogger.error("Failed to parse WebSocket message", error);
      callbacks.onError?.("Invalid server response");
      return;
    }

    // Handle different message types
    switch (data.type) {
      case "progress":
        callbacks.onProgress?.(data.value);
        break;

      case "log": {
        // Format timestamp for display if present
        const logTimestamp = (data as { timestamp?: string }).timestamp;
        const formattedTs = logTimestamp
          ? new Date(logTimestamp).toLocaleTimeString("en-US", {
              hour: "2-digit",
              minute: "2-digit",
              second: "2-digit",
              hour12: false,
            })
          : undefined;
        callbacks.onLog?.(data.message, formattedTs);
        break;
      }

      case "done":
        clearTimeout(timeoutId);
        // Invalidate cache so the new clips are fetched fresh
        void invalidateClipsCache(data.videoId);
        callbacks.onDone?.(data.videoId);
        ws.close();
        break;

      case "error":
        clearTimeout(timeoutId);
        callbacks.onError?.(data.message, data.details);
        ws.close();
        break;

      case "scene_started":
        callbacks.onSceneStarted?.(
          data.sceneId,
          data.sceneTitle,
          data.styleCount,
          data.startSec,
          data.durationSec
        );
        break;

      case "scene_completed":
        callbacks.onSceneCompleted?.(
          data.sceneId,
          data.clipsCompleted,
          data.clipsFailed
        );
        break;

      case "clip_progress":
        callbacks.onClipProgress?.(data.sceneId, data.style, data.step, data.details);
        break;

      case "clip_uploaded":
        callbacks.onClipUploaded?.(data.videoId, data.clipCount, data.totalClips);
        break;

      case "style_omitted":
        callbacks.onStyleOmitted?.(data.sceneId, data.style, data.reason);
        break;

      default:
        frontendLogger.warn("Unknown WebSocket message type", data);
    }
  };

  ws.onerror = (error) => {
    clearTimeout(timeoutId);
    frontendLogger.error("WebSocket reprocessing error", error);
    callbacks.onError?.("Connection error. Please try again.");
  };

  ws.onclose = () => {
    clearTimeout(timeoutId);
    callbacks.onClose?.();
  };

  return ws;
}
