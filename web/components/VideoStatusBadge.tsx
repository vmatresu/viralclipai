/**
 * Video Status Badge Component
 *
 * Displays the processing status of a video with appropriate styling.
 * Integrates with global processing context for real-time updates.
 */

"use client";

import { CheckCircle2, Loader2, XCircle } from "lucide-react";

import { useProcessing } from "@/lib/processing-context";
import { getStyleLabel, getStyleTier, getTierBadgeClasses } from "@/lib/styleTiers";
import { cn } from "@/lib/utils";

interface VideoStatusBadgeProps {
  videoId: string;
  status?: "processing" | "completed" | "failed";
  clipsCount?: number;
  style?: string;
  className?: string;
}

export function VideoStatusBadge({
  videoId,
  status,
  clipsCount,
  style,
  className = "",
}: VideoStatusBadgeProps) {
  const { getJob } = useProcessing();
  const job = getJob(videoId);

  // Use job status from context if available, otherwise fall back to API status
  const effectiveStatus = job?.status ?? status;
  const progress = job?.progress ?? 0;
  const styleLabel = getStyleLabel(style);
  const styleBadgeClasses = style ? getTierBadgeClasses(getStyleTier(style)?.color) : "";

  if (effectiveStatus === "completed") {
    return (
      <div
        className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors border-green-500/30 bg-green-500/10 text-green-500 ${className}`}
      >
        <CheckCircle2 className="h-3 w-3" />
        Complete
        {clipsCount !== undefined && clipsCount > 0 && (
          <span className="text-green-500/70">({clipsCount} clips)</span>
        )}
        {styleLabel && (
          <span
            className={cn(
              "ml-1 rounded-full border px-2 py-0.5 text-[10px] font-semibold",
              styleBadgeClasses
            )}
          >
            {styleLabel}
          </span>
        )}
      </div>
    );
  }

  if (effectiveStatus === "failed") {
    return (
      <div
        className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors border-destructive/30 bg-destructive/10 text-destructive ${className}`}
      >
        <XCircle className="h-3 w-3" />
        Failed
        {styleLabel && (
          <span
            className={cn(
              "ml-1 rounded-full border px-2 py-0.5 text-[10px] font-semibold",
              styleBadgeClasses
            )}
          >
            {styleLabel}
          </span>
        )}
      </div>
    );
  }

  if (effectiveStatus === "processing" || effectiveStatus === "pending") {
    return (
      <div
        className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors border-primary/30 bg-primary/10 text-primary ${className}`}
      >
        <Loader2 className="h-3 w-3 animate-spin" />
        Processing
        {progress > 0 && (
          <span className="text-primary/70">{Math.round(progress)}%</span>
        )}
        {styleLabel && (
          <span
            className={cn(
              "ml-1 rounded-full border px-2 py-0.5 text-[10px] font-semibold",
              styleBadgeClasses
            )}
          >
            {styleLabel}
          </span>
        )}
      </div>
    );
  }

  // No status or unknown status - don't show badge
  return null;
}
