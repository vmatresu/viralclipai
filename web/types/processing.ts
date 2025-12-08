import { type ClipProcessingStep } from "@/lib/websocket/types";

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
