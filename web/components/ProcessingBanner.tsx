"use client";

/**
 * Global Processing Banner (Simplified)
 *
 * Shows a simple banner when videos are being processed in the current session.
 * Since we no longer have real-time progress updates, this just shows a count
 * of videos being processed and a link to the history page to check progress.
 */

import { Loader2, X } from "lucide-react";
import Link from "next/link";

import { useProcessing } from "@/lib/processing-context";

export function ProcessingBanner() {
  const { activeJobCount, processingVideos, stopProcessing } = useProcessing();

  // Don't show if no jobs initiated this session
  if (activeJobCount === 0) {
    return null;
  }

  const videoIds = Array.from(processingVideos);
  const singleVideoId = videoIds.length === 1 ? videoIds[0] : null;

  return (
    <div className="fixed top-0 left-0 right-0 z-50 bg-background/95 backdrop-blur-sm border-b shadow-sm">
      <div className="max-w-7xl mx-auto px-4">
        <div className="flex items-center justify-between py-2 text-sm">
          <div className="flex items-center gap-3">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>
              Processing {activeJobCount} video{activeJobCount > 1 ? "s" : ""}...
            </span>
            <Link href="/history" className="text-primary hover:underline font-medium">
              View in History
            </Link>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">
              Refresh the page to check status
            </span>
            {singleVideoId && (
              <button
                onClick={() => stopProcessing(singleVideoId)}
                className="p-1 text-muted-foreground hover:text-foreground transition-colors"
                title="Dismiss"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
