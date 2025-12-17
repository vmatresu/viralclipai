/**
 * Clip processing step types (moved from deleted websocket/types.ts)
 */
export type ClipProcessingStep =
  | "queued"
  | "downloading"
  | "encoding"
  | "uploading"
  | "done"
  | "failed";

/**
 * Scene progress tracking
 */
export interface SceneProgress {
  sceneId: number;
  sceneTitle: string;
  styleCount: number;
  startSec: number;
  durationSec: number;
  status: "pending" | "processing" | "completed" | "failed";
  clipsCompleted: number;
  clipsFailed: number;
  currentSteps: Map<string, { step: ClipProcessingStep; details?: string }>;
}
