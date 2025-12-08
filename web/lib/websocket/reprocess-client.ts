/**
 * WebSocket client for scene reprocessing with progress tracking.
 *
 * Provides a clean, type-safe interface for reprocessing scenes via WebSocket.
 */

import { invalidateClipsCache } from "@/lib/cache";
import { frontendLogger } from "@/lib/logger";

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

export type ReprocessMessage =
  | ReprocessProgressMessage
  | ReprocessLogMessage
  | ReprocessDoneMessage
  | ReprocessErrorMessage;

export interface ReprocessCallbacks {
  onProgress?: (value: number) => void;
  onLog?: (message: string) => void;
  onDone?: (videoId: string) => void;
  onError?: (message: string, details?: string) => void;
  onClose?: () => void;
}

export interface ReprocessOptions {
  videoId: string;
  sceneIds: number[];
  styles: string[];
  token: string;
  cropMode?: string;
  targetAspect?: string;
}

const MAX_MESSAGE_SIZE = 1024 * 1024; // 1MB
const DEFAULT_TIMEOUT = 300000; // 5 minutes

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

      case "log":
        callbacks.onLog?.(data.message);
        break;

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
