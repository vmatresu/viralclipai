/**
 * Detailed Processing Status Component
 *
 * Displays processing progress and logs with detailed scene-level progress.
 * Shared between Homepage and History Detail page.
 */

import { Info, Loader2 } from "lucide-react";
import Link from "next/link";

import { SceneProgressCard } from "@/components/shared/SceneProgressCard";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { type SceneProgress } from "@/types/processing";

export interface DetailedProcessingStatusProps {
  progress: number;
  logs: string[];
  sceneProgress?: Map<number, SceneProgress>;
  isResuming?: boolean; // New prop to indicate we are resuming/monitoring an existing job
}

export function DetailedProcessingStatus({
  progress,
  logs,
  sceneProgress,
  isResuming = false,
}: DetailedProcessingStatusProps) {
  const scenes = sceneProgress ? Array.from(sceneProgress.values()) : [];
  const hasSceneProgress = scenes.length > 0;

  return (
    <section className="space-y-6">
      <Card className="glass border-l-4 border-l-primary">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
            {isResuming ? "Monitoring Processing..." : "Processing Video..."}
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
          {!hasSceneProgress && isResuming && (
            <div className="text-sm text-muted-foreground italic">
              Connecting to processing stream or waiting for updates...
            </div>
          )}

          {/* Logs Section */}
          <div className="bg-muted/50 rounded-xl p-4 font-mono text-sm h-48 overflow-y-auto border space-y-1">
            {logs.length === 0 && (
              <div className="text-muted-foreground italic">
                {isResuming ? "Retrieving logs..." : "Waiting for task..."}
              </div>
            )}
            {/* Log entries don't have unique IDs, using content+index as key */}
            {logs.length > 0 &&
              logs.map((l, idx) => (
                // eslint-disable-next-line react/no-array-index-key
                <div key={`log-${idx}`} className="text-foreground">
                  {l}
                </div>
              ))}
          </div>
        </CardContent>
      </Card>
    </section>
  );
}
