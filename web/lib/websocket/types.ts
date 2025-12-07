/**
 * WebSocket Message Types and Schemas
 *
 * Centralized definitions for WebSocket message types used in video processing.
 */

export const WS_MESSAGE_TYPES = {
  LOG: "log",
  PROGRESS: "progress",
  ERROR: "error",
  DONE: "done",
  CLIP_UPLOADED: "clip_uploaded",
  CLIP_PROGRESS: "clip_progress",
  SCENE_STARTED: "scene_started",
  SCENE_COMPLETED: "scene_completed",
} as const;

export type WSMessageType = (typeof WS_MESSAGE_TYPES)[keyof typeof WS_MESSAGE_TYPES];

/**
 * Clip processing step enum (matches backend ClipProcessingStep)
 */
export const CLIP_PROCESSING_STEPS = {
  EXTRACTING_SEGMENT: "extracting_segment",
  DETECTING_FACES: "detecting_faces",
  FACE_DETECTION_COMPLETE: "face_detection_complete",
  COMPUTING_CAMERA_PATH: "computing_camera_path",
  CAMERA_PATH_COMPLETE: "camera_path_complete",
  COMPUTING_CROP_WINDOWS: "computing_crop_windows",
  RENDERING: "rendering",
  RENDER_COMPLETE: "render_complete",
  UPLOADING: "uploading",
  UPLOAD_COMPLETE: "upload_complete",
  COMPLETE: "complete",
  FAILED: "failed",
} as const;

export type ClipProcessingStep =
  (typeof CLIP_PROCESSING_STEPS)[keyof typeof CLIP_PROCESSING_STEPS];

/**
 * Base WebSocket message structure
 */
export interface BaseWSMessage {
  type: WSMessageType;
  timestamp?: string;
}

/**
 * Log message from server
 */
export interface WSLogMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.LOG;
  message: string;
}

/**
 * Progress update message
 */
export interface WSProgressMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.PROGRESS;
  value: number; // 0-100
}

/**
 * Error message from server
 */
export interface WSErrorMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.ERROR;
  message: string;
  details?: string;
}

/**
 * Processing completion message
 */
export interface WSDoneMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.DONE;
  videoId: string;
}

/**
 * Clip upload notification message
 */
export interface WSClipUploadedMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.CLIP_UPLOADED;
  videoId: string;
  clipCount: number;
  totalClips: number;
}

/**
 * Detailed clip processing progress message
 */
export interface WSClipProgressMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.CLIP_PROGRESS;
  sceneId: number;
  style: string;
  step: ClipProcessingStep;
  details?: string;
}

/**
 * Scene processing started message
 */
export interface WSSceneStartedMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.SCENE_STARTED;
  sceneId: number;
  sceneTitle: string;
  styleCount: number;
  startSec: number;
  durationSec: number;
}

/**
 * Scene processing completed message
 */
export interface WSSceneCompletedMessage extends BaseWSMessage {
  type: typeof WS_MESSAGE_TYPES.SCENE_COMPLETED;
  sceneId: number;
  clipsCompleted: number;
  clipsFailed: number;
}

/**
 * Union type of all WebSocket messages
 */
export type WSMessage =
  | WSLogMessage
  | WSProgressMessage
  | WSErrorMessage
  | WSDoneMessage
  | WSClipUploadedMessage
  | WSClipProgressMessage
  | WSSceneStartedMessage
  | WSSceneCompletedMessage;

/**
 * Type guard to check if message is a specific type
 */
export function isWSMessageType<T extends WSMessage>(
  message: unknown,
  type: T["type"]
): message is T {
  return (
    typeof message === "object" &&
    message !== null &&
    "type" in message &&
    (message as { type: string }).type === type
  );
}

/**
 * Validate WebSocket message structure
 */
export function validateWSMessage(message: unknown): message is WSMessage {
  if (!message || typeof message !== "object") {
    return false;
  }

  const msg = message as Record<string, unknown>;
  if (!("type" in msg) || typeof msg.type !== "string") {
    return false;
  }

  const type = msg.type;
  return Object.values(WS_MESSAGE_TYPES).includes(type as WSMessageType);
}
