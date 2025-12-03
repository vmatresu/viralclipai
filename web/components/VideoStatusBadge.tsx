/**
 * Video Status Badge Component
 * 
 * Displays the processing status of a video with appropriate styling.
 */

import { Loader2 } from "lucide-react";

interface VideoStatusBadgeProps {
  status?: "processing" | "completed";
  clipsCount?: number;
  className?: string;
}

export function VideoStatusBadge({
  status,
  clipsCount,
  className = "",
}: VideoStatusBadgeProps) {
  if (status !== "processing") {
    return null;
  }

  return (
    <div
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors border-primary/30 bg-primary/10 text-primary ${className}`}
    >
      <Loader2 className="h-3 w-3 animate-spin" />
      Processing
      {clipsCount !== undefined && (
        <span className="text-primary/70">({clipsCount} clips)</span>
      )}
    </div>
  );
}

