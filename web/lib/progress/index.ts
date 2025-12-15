/**
 * Progress tracking module
 *
 * Provides robust job progress tracking with:
 * - REST polling for reliable status updates
 * - Automatic reconnection and recovery
 * - Scene progress persistence
 */

export {
  ProgressManager,
  getProgressManager,
  resetProgressManager,
  type ProgressManagerConfig,
  type JobProgress,
  type ProgressEvent,
  type ProgressEventType,
  type ConnectionState,
  type ProgressEventHandler,
  type StatusHandler,
  type ProgressUpdateEvent,
  type LogEvent,
  type ErrorEvent,
  type DoneEvent,
  type ClipUploadedEvent,
  type ClipProgressEvent,
  type SceneStartedEvent,
  type SceneCompletedEvent,
  type StyleOmittedEvent,
  type ConnectionStateEvent,
} from "./ProgressManager";

export { useJobProgress } from "./useJobProgress";
