/**
 * Detailed Processing Status Component
 *
 * Refactored: Compact, premium status bar with collapsible details.
 * Optimized for "set it and forget it" workflow.
 */

import { CheckCircle2, ChevronDown, Loader2, Sparkles } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import { SceneProgressCard } from "@/components/shared/SceneProgressCard";
import { cn } from "@/lib/utils";
import { type SceneProgress } from "@/types/processing";

export interface DetailedProcessingStatusProps {
  progress: number;
  logs: string[];
  sceneProgress?: Map<number, SceneProgress>;
  isResuming?: boolean;
}

export function DetailedProcessingStatus({
  progress,
  logs,
  sceneProgress,
  isResuming = false,
}: DetailedProcessingStatusProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const scenes = sceneProgress ? Array.from(sceneProgress.values()) : [];
  const hasSceneProgress = scenes.length > 0;

  // Determine current stage from logs or default
  const lastLog = logs.at(-1);
  const statusText = isResuming
    ? "Monitoring processing..."
    : (lastLog ?? "Processing video in background...");

  const isComplete = progress >= 100;

  return (
    <section className="w-full animate-in fade-in slide-in-from-top-4 duration-500">
      {/* Main Compact Bar */}
      <div
        className={cn(
          "relative overflow-hidden rounded-xl border border-white/10 bg-slate-950/60 backdrop-blur-md transition-all duration-300",
          isExpanded ? "ring-1 ring-white/10" : "hover:bg-slate-950/70"
        )}
      >
        {/* Background Progress Fill (Subtle) */}
        <div
          className="absolute inset-y-0 left-0 bg-indigo-500/5 transition-all duration-1000 ease-out"
          style={{ width: `${progress}%` }}
        />

        {/* Content Row */}
        <div className="relative flex items-center justify-between gap-4 p-4">
          <div className="flex items-center gap-3 min-w-0">
            {/* Status Icon */}
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-indigo-500/10 ring-1 ring-indigo-500/20">
              {isComplete ? (
                <CheckCircle2 className="h-5 w-5 text-emerald-400" />
              ) : (
                <Loader2 className="h-5 w-5 animate-spin text-indigo-400" />
              )}
            </div>

            {/* Text & Progress */}
            <div className="flex flex-col min-w-0">
              <div className="flex items-center gap-2">
                <span className="font-medium text-white truncate">
                  {isComplete
                    ? "Processing Complete"
                    : "Processing Video in Background"}
                </span>
                <span className="inline-flex items-center rounded-full bg-indigo-500/10 px-2 py-0.5 text-[10px] font-medium text-indigo-300 ring-1 ring-inset ring-indigo-500/20">
                  {Math.round(progress)}%
                </span>
              </div>
              <p className="text-xs text-muted-foreground truncate max-w-[300px] sm:max-w-md">
                {statusText}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-3 shrink-0">
            {/* Safe to leave hint (Desktop only) */}
            <div className="hidden sm:flex items-center gap-1.5 text-[10px] text-muted-foreground/70 bg-white/5 px-2 py-1 rounded-md">
              <Sparkles className="h-3 w-3" />
              <span>Safe to leave page</span>
            </div>

            {/* Expand Toggle */}
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="group flex items-center gap-1.5 rounded-lg border border-white/5 bg-white/5 px-3 py-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-white/10 hover:text-white"
            >
              <span className="hidden sm:inline">
                {isExpanded ? "Hide Details" : "View Details"}
              </span>
              <ChevronDown
                className={cn(
                  "h-3.5 w-3.5 transition-transform duration-200",
                  isExpanded && "rotate-180"
                )}
              />
            </button>
          </div>
        </div>

        {/* Progress Line (Bottom) */}
        {!isComplete && (
          <div className="h-[2px] w-full bg-slate-800">
            <div
              className="h-full bg-gradient-to-r from-indigo-500 to-purple-500 shadow-[0_0_10px_rgba(99,102,241,0.5)] transition-all duration-500 ease-out"
              style={{ width: `${progress}%` }}
            />
          </div>
        )}
      </div>

      {/* Expanded Details Section */}
      {isExpanded && (
        <div className="mt-2 animate-in slide-in-from-top-2 duration-300">
          <div className="rounded-xl border border-white/5 bg-black/40 p-4 space-y-4 backdrop-blur-sm">
            {/* Safe to leave notice (Expanded) */}
            <div className="flex items-start gap-3 rounded-lg bg-indigo-500/5 p-3 border border-indigo-500/10">
              <Sparkles className="h-4 w-4 text-indigo-400 mt-0.5 shrink-0" />
              <div className="space-y-1">
                <p className="text-sm font-medium text-indigo-100">
                  Background Processing Active
                </p>
                <p className="text-xs text-indigo-200/60">
                  You can safely navigate away. Check progress anytime on the{" "}
                  <Link
                    href="/history"
                    className="text-indigo-300 hover:text-indigo-200 underline decoration-indigo-500/30 underline-offset-2"
                  >
                    history page
                  </Link>
                  .
                </p>
              </div>
            </div>

            {/* Scene Progress Grid */}
            {hasSceneProgress && (
              <div className="space-y-2">
                <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wider pl-1">
                  Scene Progress
                </h4>
                <div className="grid gap-2 max-h-60 overflow-y-auto pr-1">
                  {scenes.map((scene) => (
                    <SceneProgressCard key={scene.sceneId} scene={scene} />
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
