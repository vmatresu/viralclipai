/**
 * WebSocket Message Handler
 *
 * Handles WebSocket messages for video processing with proper validation
 * and type safety.
 */

import { invalidateClipsCache } from "@/lib/cache";
import { frontendLogger } from "@/lib/logger";

import {
  CLIP_PROCESSING_STEPS,
  isWSMessageType,
  validateWSMessage,
  WS_MESSAGE_TYPES,
  type ClipProcessingStep,
  type WSClipProgressMessage,
  type WSClipUploadedMessage,
  type WSDoneMessage,
  type WSErrorMessage,
  type WSJobStartedMessage,
  type WSLogMessage,
  type WSProgressMessage,
  type WSSceneCompletedMessage,
  type WSSceneStartedMessage,
} from "./types";

export interface MessageHandlerCallbacks {
  onLog: (message: string, timestamp?: string) => void;
  onProgress: (value: number) => void;
  onError: (message: string, details?: string) => void;
  onDone: (videoId: string) => void;
  onClipUploaded: (videoId: string, clipCount: number, totalClips: number) => void;
  // Job tracking callback for polling fallback
  onJobStarted?: (jobId: string, videoId: string) => void;
  // New detailed progress callbacks (optional for backward compatibility)
  onClipProgress?: (
    sceneId: number,
    style: string,
    step: ClipProcessingStep,
    details?: string
  ) => void;
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
}

/**
 * Get human-readable label for a processing step
 */
export function getStepLabel(step: ClipProcessingStep): string {
  switch (step) {
    case CLIP_PROCESSING_STEPS.EXTRACTING_SEGMENT:
      return "Extracting segment";
    case CLIP_PROCESSING_STEPS.DETECTING_FACES:
      return "Detecting faces";
    case CLIP_PROCESSING_STEPS.FACE_DETECTION_COMPLETE:
      return "Face detection complete";
    case CLIP_PROCESSING_STEPS.COMPUTING_CAMERA_PATH:
      return "Computing camera path";
    case CLIP_PROCESSING_STEPS.CAMERA_PATH_COMPLETE:
      return "Camera path complete";
    case CLIP_PROCESSING_STEPS.COMPUTING_CROP_WINDOWS:
      return "Computing crop windows";
    case CLIP_PROCESSING_STEPS.RENDERING:
      return "Rendering";
    case CLIP_PROCESSING_STEPS.RENDER_COMPLETE:
      return "Render complete";
    case CLIP_PROCESSING_STEPS.UPLOADING:
      return "Uploading";
    case CLIP_PROCESSING_STEPS.UPLOAD_COMPLETE:
      return "Upload complete";
    case CLIP_PROCESSING_STEPS.COMPLETE:
      return "Complete";
    case CLIP_PROCESSING_STEPS.FAILED:
      return "Failed";
    default:
      return step;
  }
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
      isWSMessageType<WSClipUploadedMessage>(message, WS_MESSAGE_TYPES.CLIP_UPLOADED)
    ) {
      const videoId = typeof message.videoId === "string" ? message.videoId : null;
      const clipCount = typeof message.clipCount === "number" ? message.clipCount : 0;
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

    // Handle detailed clip progress messages
    if (
      isWSMessageType<WSClipProgressMessage>(message, WS_MESSAGE_TYPES.CLIP_PROGRESS)
    ) {
      if (callbacks.onClipProgress) {
        const sceneId = typeof message.sceneId === "number" ? message.sceneId : 0;
        const style = typeof message.style === "string" ? message.style : "unknown";
        const step = message.step;
        const details =
          typeof message.details === "string" ? message.details : undefined;
        callbacks.onClipProgress(sceneId, style, step, details);
      }
      return true;
    }

    // Handle scene started messages
    if (
      isWSMessageType<WSSceneStartedMessage>(message, WS_MESSAGE_TYPES.SCENE_STARTED)
    ) {
      if (callbacks.onSceneStarted) {
        const sceneId = typeof message.sceneId === "number" ? message.sceneId : 0;
        const sceneTitle =
          typeof message.sceneTitle === "string" ? message.sceneTitle : "";
        const styleCount =
          typeof message.styleCount === "number" ? message.styleCount : 0;
        const startSec = typeof message.startSec === "number" ? message.startSec : 0;
        const durationSec =
          typeof message.durationSec === "number" ? message.durationSec : 0;
        callbacks.onSceneStarted(
          sceneId,
          sceneTitle,
          styleCount,
          startSec,
          durationSec
        );
      }
      return true;
    }

    // Handle scene completed messages
    if (
      isWSMessageType<WSSceneCompletedMessage>(
        message,
        WS_MESSAGE_TYPES.SCENE_COMPLETED
      )
    ) {
      if (callbacks.onSceneCompleted) {
        const sceneId = typeof message.sceneId === "number" ? message.sceneId : 0;
        const clipsCompleted =
          typeof message.clipsCompleted === "number" ? message.clipsCompleted : 0;
        const clipsFailed =
          typeof message.clipsFailed === "number" ? message.clipsFailed : 0;
        callbacks.onSceneCompleted(sceneId, clipsCompleted, clipsFailed);
      }
      return true;
    }

    // Handle job started messages (for polling fallback)
    if (isWSMessageType<WSJobStartedMessage>(message, WS_MESSAGE_TYPES.JOB_STARTED)) {
      if (callbacks.onJobStarted) {
        const jobId = typeof message.jobId === "string" ? message.jobId : "";
        const videoId = typeof message.videoId === "string" ? message.videoId : "";
        if (jobId && videoId) {
          callbacks.onJobStarted(jobId, videoId);
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
