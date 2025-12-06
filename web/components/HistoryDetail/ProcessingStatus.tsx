"use client";

import { CheckCircle2, Clock, Film, Loader2, XCircle } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";

interface ProcessingStatusProps {
  videoId: string;
  videoTitle?: string;
  progress?: number;
  currentStep?: string;
  clipsCompleted?: number;
  totalClips?: number;
  status?: "pending" | "processing" | "completed" | "failed";
  error?: string;
  startedAt?: number;
  onDismiss?: () => void;
}

function formatElapsedTime(startedAt: number): string {
  const elapsed = Math.floor((Date.now() - startedAt) / 1000);
  if (elapsed < 60) return `${elapsed}s`;
  if (elapsed < 3600) return `${Math.floor(elapsed / 60)}m ${elapsed % 60}s`;
  return `${Math.floor(elapsed / 3600)}h ${Math.floor((elapsed % 3600) / 60)}m`;
}

export function ProcessingStatus({
  videoId,
  videoTitle,
  progress = 0,
  currentStep,
  clipsCompleted = 0,
  totalClips = 0,
  status = "processing",
  error,
  startedAt,
  onDismiss,
}: ProcessingStatusProps) {
  const [elapsedTime, setElapsedTime] = useState("");

  // Update elapsed time every second
  useEffect(() => {
    if (!startedAt || status === "completed" || status === "failed") return;

    const updateTime = () => setElapsedTime(formatElapsedTime(startedAt));
    updateTime();

    const interval = setInterval(updateTime, 1000);
    return () => clearInterval(interval);
  }, [startedAt, status]);

  const isActive = status === "pending" || status === "processing";
  const isCompleted = status === "completed";
  const isFailed = status === "failed";

  const borderColor = isCompleted
    ? "border-l-green-500"
    : isFailed
      ? "border-l-destructive"
      : "border-l-primary";

  const StatusIcon = isCompleted ? CheckCircle2 : isFailed ? XCircle : Loader2;

  const iconColor = isCompleted
    ? "text-green-500"
    : isFailed
      ? "text-destructive"
      : "text-primary";

  const title = isCompleted
    ? "Processing Complete!"
    : isFailed
      ? "Processing Failed"
      : "Processing in Background";

  return (
    <Card className={`glass border-l-4 ${borderColor}`}>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <StatusIcon
              className={`h-5 w-5 ${iconColor} ${isActive ? "animate-spin" : ""}`}
            />
            <span>{title}</span>
          </div>
          {onDismiss && (isCompleted || isFailed) && (
            <button
              onClick={onDismiss}
              className="text-muted-foreground hover:text-foreground text-sm"
            >
              Dismiss
            </button>
          )}
        </CardTitle>
        {videoTitle && (
          <p className="text-sm text-muted-foreground truncate">{videoTitle}</p>
        )}
      </CardHeader>
      <CardContent className="space-y-3">
        {isActive && (
          <>
            <div className="space-y-1">
              <div className="flex justify-between text-sm">
                <span className="text-muted-foreground">
                  {currentStep ?? "Processing..."}
                </span>
                <span className="font-medium">{Math.round(progress)}%</span>
              </div>
              <Progress value={progress} className="h-2" />
            </div>

            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <div className="flex items-center gap-4">
                {totalClips > 0 && (
                  <div className="flex items-center gap-1">
                    <Film className="h-3 w-3" />
                    <span>
                      {clipsCompleted}/{totalClips} clips
                    </span>
                  </div>
                )}
                {elapsedTime && (
                  <div className="flex items-center gap-1">
                    <Clock className="h-3 w-3" />
                    <span>{elapsedTime}</span>
                  </div>
                )}
              </div>
            </div>
          </>
        )}

        {isCompleted && (
          <div className="flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              {totalClips > 0
                ? `${totalClips} clips generated successfully!`
                : "Your clips are ready to view."}
            </p>
            <Link
              href={`/?id=${encodeURIComponent(videoId)}`}
              className="text-sm font-medium text-primary hover:underline"
            >
              View Clips →
            </Link>
          </div>
        )}

        {isFailed && (
          <div className="space-y-2">
            <p className="text-sm text-destructive">
              {error ?? "An error occurred during processing."}
            </p>
            <p className="text-xs text-muted-foreground">
              Please try again or contact support if the issue persists.
            </p>
          </div>
        )}

        {isActive && (
          <p className="text-xs text-muted-foreground">
            You can{" "}
            <Link
              href={`/?id=${encodeURIComponent(videoId)}`}
              className="text-primary hover:underline"
            >
              view progress here
            </Link>{" "}
            or continue browsing. Processing will continue even if you leave this page.
          </p>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * Compact processing indicator for use in lists
 */
export function ProcessingIndicator({
  progress = 0,
  clipsCompleted = 0,
  totalClips = 0,
  currentStep,
}: {
  progress?: number;
  clipsCompleted?: number;
  totalClips?: number;
  currentStep?: string;
}) {
  return (
    <div className="flex items-center gap-3 bg-primary/5 border border-primary/20 rounded-lg px-3 py-2">
      <Loader2 className="h-4 w-4 animate-spin text-primary flex-shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="flex items-center justify-between text-xs mb-1">
          <span className="text-muted-foreground truncate">
            {currentStep ?? "Processing..."}
          </span>
          <span className="font-medium text-primary ml-2">{Math.round(progress)}%</span>
        </div>
        <Progress value={progress} className="h-1.5" />
        {totalClips > 0 && (
          <p className="text-xs text-muted-foreground mt-1">
            {clipsCompleted}/{totalClips} clips
          </p>
        )}
      </div>
    </div>
  );
}
