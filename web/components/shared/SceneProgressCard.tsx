import { CheckCircle2, Clock, Loader2, XCircle } from "lucide-react";
import { type JSX } from "react";

import { getStepLabel } from "@/lib/websocket/messageHandler";
import { type SceneProgress } from "@/types/processing";

/**
 * Format duration in seconds to a human-readable string
 */
function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return mins > 0 ? `${mins}m ${secs}s` : `${secs}s`;
}

/**
 * Scene progress card component
 */
export function SceneProgressCard({ scene }: { scene: SceneProgress }) {
  const statusIcon: Record<SceneProgress["status"], JSX.Element> = {
    pending: <Clock className="h-4 w-4 text-muted-foreground" />,
    processing: <Loader2 className="h-4 w-4 animate-spin text-primary" />,
    completed: <CheckCircle2 className="h-4 w-4 text-green-500" />,
    failed: <XCircle className="h-4 w-4 text-red-500" />,
  };

  const completedStyles = Array.from(scene.currentSteps.values()).filter(
    (v) => v.step === "complete"
  ).length;

  return (
    <div className="border rounded-lg p-3 bg-background/50">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          {statusIcon[scene.status]}
          <span className="font-medium text-sm truncate max-w-[200px]">
            Scene {scene.sceneId}: {scene.sceneTitle}
          </span>
        </div>
        <span className="text-xs text-muted-foreground">
          {formatDuration(scene.startSec)} -{" "}
          {formatDuration(scene.startSec + scene.durationSec)}
        </span>
      </div>

      {scene.status === "processing" && scene.currentSteps.size > 0 && (
        <div className="space-y-1 mt-2">
          {Array.from(scene.currentSteps.entries()).map(
            ([style, { step, details }]) => (
              <div key={style} className="flex items-center gap-2 text-xs">
                <span className="text-muted-foreground w-24 truncate">{style}</span>
                <span className="text-foreground">{getStepLabel(step)}</span>
                {details && (
                  <span className="text-muted-foreground truncate max-w-[150px]">
                    ({details})
                  </span>
                )}
              </div>
            )
          )}
        </div>
      )}

      {scene.status === "completed" && (
        <div className="text-xs text-green-600 mt-1">
          ✓ {scene.clipsCompleted} clips completed
          {scene.clipsFailed > 0 && (
            <span className="text-red-500 ml-2">({scene.clipsFailed} failed)</span>
          )}
        </div>
      )}

      {scene.status === "processing" && (
        <div className="w-full bg-muted rounded-full h-1.5 mt-2">
          <div
            className="bg-primary h-1.5 rounded-full transition-all duration-300"
            style={{ width: `${(completedStyles / scene.styleCount) * 100}%` }}
          />
        </div>
      )}
    </div>
  );
}
