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
} as const;

export type WSMessageType = typeof WS_MESSAGE_TYPES[keyof typeof WS_MESSAGE_TYPES];

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
 * Union type of all WebSocket messages
 */
export type WSMessage =
  | WSLogMessage
  | WSProgressMessage
  | WSErrorMessage
  | WSDoneMessage
  | WSClipUploadedMessage;

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

  const type = msg.type as string;
  return Object.values(WS_MESSAGE_TYPES).includes(type as WSMessageType);
}

