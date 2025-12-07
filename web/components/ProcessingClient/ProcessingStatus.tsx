/**
 * Processing Status Component
 *
 * Displays processing progress and logs with detailed scene-level progress.
 */

import { CheckCircle2, Clock, Info, Loader2, XCircle } from "lucide-react";
import Link from "next/link";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { getStepLabel } from "@/lib/websocket/messageHandler";

import { type SceneProgress } from "./hooks";

interface ProcessingStatusProps {
  progress: number;
  logs: string[];
  sceneProgress?: Map<number, SceneProgress>;
}

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
function SceneProgressCard({ scene }: { scene: SceneProgress }) {
  const statusIcon = {
    pending: <Clock className="h-4 w-4 text-muted-foreground" />,
    processing: <Loader2 className="h-4 w-4 animate-spin text-primary" />,
    completed: <CheckCircle2 className="h-4 w-4 text-green-500" />,
    failed: <XCircle className="h-4 w-4 text-red-500" />,
  }[scene.status];

  const completedStyles = Array.from(scene.currentSteps.entries()).filter(
    ([, v]) => v.step === "complete"
  ).length;

  return (
    <div className="border rounded-lg p-3 bg-background/50">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          {statusIcon}
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

export function ProcessingStatus({
  progress,
  logs,
  sceneProgress,
}: ProcessingStatusProps) {
  const scenes = sceneProgress ? Array.from(sceneProgress.values()) : [];
  const hasSceneProgress = scenes.length > 0;

  return (
    <section className="space-y-6">
      <Card className="glass border-l-4 border-l-primary">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
            Processing Video...
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="w-full bg-muted rounded-full h-4 overflow-hidden">
            <div
              className="bg-gradient-to-r from-brand-500 to-brand-700 h-4 rounded-full transition-all duration-500 ease-out"
              style={{ width: `${progress}%` }}
            />
          </div>

          <div className="bg-primary/10 border border-primary/20 rounded-lg p-4 flex items-start gap-3">
            <Info className="h-5 w-5 text-primary mt-0.5 flex-shrink-0" />
            <div className="space-y-1 text-sm">
              <p className="font-semibold text-foreground">
                You can safely leave this page
              </p>
              <p className="text-muted-foreground">
                Your video is being processed in the background. You can navigate away
                and check your{" "}
                <Link
                  href="/history"
                  className="text-primary hover:underline font-medium"
                >
                  history page
                </Link>{" "}
                to see progress. Processing will continue even if you close this tab.
              </p>
            </div>
          </div>

          {/* Scene Progress Section */}
          {hasSceneProgress && (
            <div className="space-y-3">
              <h4 className="text-sm font-medium text-foreground">Scene Progress</h4>
              <div className="grid gap-2 max-h-48 overflow-y-auto">
                {scenes.map((scene) => (
                  <SceneProgressCard key={scene.sceneId} scene={scene} />
                ))}
              </div>
            </div>
          )}

          {/* Logs Section */}
          <div className="bg-muted/50 rounded-xl p-4 font-mono text-sm h-48 overflow-y-auto border space-y-1">
            {logs.length === 0 ? (
              <div className="text-muted-foreground italic">Waiting for task...</div>
            ) : (
              logs.map((l, idx) => (
                <div key={`log-${idx}`} className="text-foreground">
                  {l}
                </div>
              ))
            )}
          </div>
        </CardContent>
      </Card>
    </section>
  );
}
