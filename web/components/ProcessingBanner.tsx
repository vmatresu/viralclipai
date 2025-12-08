"use client";

/**
 * Global Processing Banner
 *
 * Shows a persistent banner at the top of the page when videos are being processed.
 * Displays progress for all active processing jobs.
 */

import { ChevronDown, ChevronUp, Film, Loader2, X } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import { Progress } from "@/components/ui/progress";
import { useProcessing, type ProcessingJob } from "@/lib/processing-context";

function JobItem({ job, onClear }: { job: ProcessingJob; onClear: () => void }) {
  const isActive = job.status === "pending" || job.status === "processing";
  const isCompleted = job.status === "completed";
  const isFailed = job.status === "failed";

  return (
    <div className="flex items-center gap-3 py-2 border-b border-border/50 last:border-b-0">
      <div className="flex-shrink-0">
        {isActive && <Loader2 className="h-4 w-4 animate-spin text-primary" />}
        {isCompleted && <Film className="h-4 w-4 text-green-500" />}
        {isFailed && <X className="h-4 w-4 text-destructive" />}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center justify-between gap-2">
          <Link
            href={`/?id=${encodeURIComponent(job.videoId)}`}
            className="text-sm font-medium truncate hover:text-primary transition-colors"
          >
            {job.videoTitle ?? `Video ${job.videoId.slice(0, 8)}...`}
          </Link>
          {isActive && (
            <span className="text-xs font-medium text-primary flex-shrink-0">
              {Math.round(job.progress)}%
            </span>
          )}
          {isCompleted && (
            <span className="text-xs text-green-500 flex-shrink-0">Complete</span>
          )}
          {isFailed && (
            <span className="text-xs text-destructive flex-shrink-0">Failed</span>
          )}
        </div>

        {isActive && (
          <div className="mt-1">
            <Progress value={job.progress} className="h-1" />
            <div className="flex items-center justify-between mt-0.5">
              <span className="text-xs text-muted-foreground truncate">
                {job.currentStep ?? "Processing..."}
              </span>
              {job.totalClips > 0 && (
                <span className="text-xs text-muted-foreground flex-shrink-0">
                  {job.clipsCompleted}/{job.totalClips} clips
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      {(isCompleted || isFailed) && (
        <button
          onClick={onClear}
          className="p-1 text-muted-foreground hover:text-foreground transition-colors"
          title="Dismiss"
        >
          <X className="h-3 w-3" />
        </button>
      )}
    </div>
  );
}

export function ProcessingBanner() {
  const { jobs, activeJobCount, clearJob, clearAllCompleted } = useProcessing();
  const [isExpanded, setIsExpanded] = useState(false);

  const allJobs = Array.from(jobs.values());
  const activeJobs = allJobs.filter(
    (j) => j.status === "pending" || j.status === "processing"
  );
  const completedJobs = allJobs.filter(
    (j) => j.status === "completed" || j.status === "failed"
  );

  // Don't show if no jobs
  if (allJobs.length === 0) {
    return null;
  }

  // Calculate overall progress for active jobs
  const overallProgress =
    activeJobs.length > 0
      ? activeJobs.reduce((sum, j) => sum + j.progress, 0) / activeJobs.length
      : 100;

  return (
    <div className="fixed top-0 left-0 right-0 z-50 bg-background/95 backdrop-blur-sm border-b shadow-sm">
      <div className="max-w-7xl mx-auto px-4">
        {/* Collapsed view */}
        <div
          onClick={() => setIsExpanded(!isExpanded)}
          className="w-full flex items-center justify-between py-2 text-sm cursor-pointer"
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              setIsExpanded(!isExpanded);
            }
          }}
        >
          <div className="flex items-center gap-3">
            {activeJobCount > 0 ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin text-primary" />
                <span>
                  Processing {activeJobCount} video{activeJobCount > 1 ? "s" : ""}
                </span>
                <span className="text-primary font-medium">
                  {Math.round(overallProgress)}%
                </span>
              </>
            ) : (
              <>
                <Film className="h-4 w-4 text-green-500" />
                <span className="text-green-500">
                  {completedJobs.length} video{completedJobs.length > 1 ? "s" : ""}{" "}
                  ready
                </span>
              </>
            )}
          </div>
          <div className="flex items-center gap-2">
            {completedJobs.length > 0 && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  clearAllCompleted();
                }}
                className="text-xs text-muted-foreground hover:text-foreground px-2 py-1"
              >
                Clear all
              </button>
            )}
            {isExpanded ? (
              <ChevronUp className="h-4 w-4" />
            ) : (
              <ChevronDown className="h-4 w-4" />
            )}
          </div>
        </div>

        {/* Progress bar for collapsed view */}
        {!isExpanded && activeJobCount > 0 && (
          <Progress value={overallProgress} className="h-1 -mt-1" />
        )}

        {/* Expanded view */}
        {isExpanded && (
          <div className="pb-3 max-h-64 overflow-y-auto">
            {activeJobs.length > 0 && (
              <div className="mb-2">
                <h4 className="text-xs font-semibold text-muted-foreground uppercase mb-1">
                  Processing
                </h4>
                {activeJobs.map((job) => (
                  <JobItem
                    key={job.videoId}
                    job={job}
                    onClear={() => clearJob(job.videoId)}
                  />
                ))}
              </div>
            )}

            {completedJobs.length > 0 && (
              <div>
                <h4 className="text-xs font-semibold text-muted-foreground uppercase mb-1">
                  Completed
                </h4>
                {completedJobs.map((job) => (
                  <JobItem
                    key={job.videoId}
                    job={job}
                    onClear={() => clearJob(job.videoId)}
                  />
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
