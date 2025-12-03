/**
 * WebSocket Message Handler
 * 
 * Handles WebSocket messages for video processing with proper validation
 * and type safety.
 */

import { frontendLogger } from "@/lib/logger";
import { invalidateClipsCache } from "@/lib/cache";
import {
  WSMessage,
  WSLogMessage,
  WSProgressMessage,
  WSErrorMessage,
  WSDoneMessage,
  WSClipUploadedMessage,
  isWSMessageType,
  validateWSMessage,
  WS_MESSAGE_TYPES,
} from "./types";

export interface MessageHandlerCallbacks {
  onLog: (message: string, timestamp?: string) => void;
  onProgress: (value: number) => void;
  onError: (message: string, details?: string) => void;
  onDone: (videoId: string) => void;
  onClipUploaded: (videoId: string, clipCount: number, totalClips: number) => void;
}

/**
 * Handle WebSocket message with validation and type safety
 */
export function handleWSMessage(
  message: unknown,
  callbacks: MessageHandlerCallbacks,
  currentVideoId?: string | null
): boolean {
  // Validate message structure
  if (!validateWSMessage(message)) {
    frontendLogger.error("Invalid WebSocket message format", { message });
    return false;
  }

  try {
    // Handle log messages
    if (isWSMessageType<WSLogMessage>(message, WS_MESSAGE_TYPES.LOG)) {
      const logMessage =
        typeof message.message === "string"
          ? message.message.substring(0, 1000)
          : "Unknown log message";
      const timestamp =
        typeof message.timestamp === "string" ? message.timestamp : undefined;
      callbacks.onLog(logMessage, timestamp);
      return true;
    }

    // Handle progress messages
    if (isWSMessageType<WSProgressMessage>(message, WS_MESSAGE_TYPES.PROGRESS)) {
      const progressValue =
        typeof message.value === "number" && message.value >= 0 && message.value <= 100
          ? message.value
          : 0;
      callbacks.onProgress(progressValue);
      return true;
    }

    // Handle error messages
    if (isWSMessageType<WSErrorMessage>(message, WS_MESSAGE_TYPES.ERROR)) {
      const errorMessage =
        typeof message.message === "string"
          ? message.message.substring(0, 500)
          : "An unexpected error occurred.";
      const errorDetails =
        typeof message.details === "string"
          ? message.details.substring(0, 200)
          : undefined;
      callbacks.onError(errorMessage, errorDetails);
      return true;
    }

    // Handle completion messages
    if (isWSMessageType<WSDoneMessage>(message, WS_MESSAGE_TYPES.DONE)) {
      const videoId =
        typeof message.videoId === "string" && message.videoId.trim() !== ""
          ? message.videoId.trim()
          : null;

      if (!videoId) {
        frontendLogger.error("Invalid video ID in done message", { message });
        callbacks.onError("Invalid video ID received");
        return false;
      }

      // Sanitize video ID
      const sanitizedId = videoId.replace(/[^a-zA-Z0-9_-]/g, "");
      if (sanitizedId !== videoId) {
        frontendLogger.warn("Video ID contained invalid characters", { id: videoId });
      }

      callbacks.onDone(sanitizedId);
      return true;
    }

    // Handle clip uploaded messages
    if (
      isWSMessageType<WSClipUploadedMessage>(
        message,
        WS_MESSAGE_TYPES.CLIP_UPLOADED
      )
    ) {
      const videoId =
        typeof message.videoId === "string" ? message.videoId : null;
      const clipCount =
        typeof message.clipCount === "number" ? message.clipCount : 0;
      const totalClips =
        typeof message.totalClips === "number" ? message.totalClips : 0;

      if (videoId) {
        // Invalidate cache so history page shows updated clips
        void invalidateClipsCache(videoId);

        // If we're currently viewing this video, reload results
        if (videoId === currentVideoId) {
          callbacks.onClipUploaded(videoId, clipCount, totalClips);
        }
      }
      return true;
    }

    // Unknown message type
    frontendLogger.warn("Unknown WebSocket message type", {
      type: (message as { type: string }).type,
    });
    return false;
  } catch (error) {
    frontendLogger.error("Error handling WebSocket message", { error, message });
    return false;
  }
}

