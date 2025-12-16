"use client";

import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

import { QualitySlider } from "./QualitySlider";
import { StaticPositionSelector } from "./StaticPositionSelector";
import { StreamerSplitConfigurator } from "./StreamerSplitConfigurator";

import type { QualityLevel, StaticPosition, StreamerSplitConfig } from "./types";
import type { KeyboardEvent, MouseEvent } from "react";

interface LayoutCardProps {
  title: string;
  enabled: boolean;
  onToggle: (next: boolean) => void;
  levelValue: string;
  levels: QualityLevel[];
  onLevelChange: (next: string) => void;
  hasStudioPlan?: boolean;
  hasProPlan?: boolean;
  staticPosition?: StaticPosition;
  onStaticPositionChange?: (next: StaticPosition) => void;
  streamerSplitConfig?: StreamerSplitConfig;
  onStreamerSplitConfigChange?: (next: StreamerSplitConfig) => void;
  topScenesEnabled?: boolean;
  onTopScenesChange?: (next: boolean) => void;
}

export function LayoutCard({
  title,
  enabled,
  onToggle,
  levelValue,
  levels,
  onLevelChange,
  hasStudioPlan,
  hasProPlan,
  staticPosition,
  onStaticPositionChange,
  streamerSplitConfig,
  onStreamerSplitConfigChange,
  topScenesEnabled,
  onTopScenesChange,
}: LayoutCardProps) {
  const enableId = `${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-toggle`;
  const isSplitView = title.toLowerCase().includes("split");

  const isInteractiveTarget = (event: MouseEvent<HTMLElement>) => {
    const target = event.target as HTMLElement | null;
    return Boolean(target?.closest("[data-interactive='true']"));
  };

  const handleCardToggle = () => onToggle(!enabled);
  const handleKeyToggle = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      handleCardToggle();
    }
  };

  return (
    <div
      className={cn(
        "relative rounded-2xl border bg-gradient-to-b from-slate-900/70 to-slate-950/70 p-4 shadow-xl transition-all",
        enabled
          ? "border-indigo-500/60 shadow-indigo-900/40"
          : "border-white/10 opacity-75 grayscale",
        "cursor-pointer focus:outline-none focus:ring-2 focus:ring-indigo-500/60 focus:ring-offset-2 focus:ring-offset-slate-950"
      )}
      role="button"
      tabIndex={0}
      aria-pressed={enabled}
      onClick={(event) => {
        if (isInteractiveTarget(event)) return;
        handleCardToggle();
      }}
      onKeyDown={handleKeyToggle}
    >
      {/* Header */}
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="text-sm font-semibold text-white">{title}</div>
          <p className="text-xs text-muted-foreground">
            9×16 vertical • Optimized for shorts
          </p>
        </div>
        <label
          className="flex items-center gap-2 text-xs font-medium text-white/80"
          htmlFor={enableId}
          data-interactive="true"
        >
          <Checkbox
            checked={enabled}
            onCheckedChange={(checked) => onToggle(Boolean(checked))}
            id={enableId}
          />
          Enable
        </label>
      </div>

      {/* Preview */}
      <div className="relative overflow-hidden rounded-xl border border-white/10 bg-gradient-to-b from-indigo-900/40 to-slate-950/60 shadow-inner flex items-center justify-center px-6 py-8">
        <div className="grid h-full w-full max-w-[180px] grid-cols-1 gap-2">
          {isSplitView ? (
            <>
              <div className="h-20 rounded-lg bg-indigo-500/25 border border-white/10" />
              <div className="h-20 rounded-lg bg-emerald-500/20 border border-white/10" />
            </>
          ) : (
            <div className="h-40 rounded-xl bg-indigo-500/20 border border-white/10" />
          )}
        </div>
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/5 via-transparent to-slate-950/40" />
      </div>

      {/* Controls */}
      <div className="mt-4" data-interactive="true">
        <QualitySlider
          levels={levels}
          value={levelValue}
          onChange={onLevelChange}
          disabled={!enabled}
          hasStudioPlan={hasStudioPlan}
          hasProPlan={hasProPlan}
        />

        {/* Static position selector */}
        {levelValue === "center_focus" && staticPosition && onStaticPositionChange && (
          <StaticPositionSelector
            position={staticPosition}
            onChange={onStaticPositionChange}
            disabled={!enabled}
          />
        )}

        {/* StreamerSplit configurator */}
        {levelValue === "streamer_split" &&
          streamerSplitConfig &&
          onStreamerSplitConfigChange && (
            <StreamerSplitConfigurator
              config={streamerSplitConfig}
              onChange={onStreamerSplitConfigChange}
              disabled={!enabled}
            />
          )}

        {/* Top Scenes checkbox for Streamer style */}
        {levelValue === "streamer" && onTopScenesChange && (
          <div
            className={cn(
              "mt-3 flex items-start gap-3 rounded-lg border border-white/10 bg-slate-900/60 p-3",
              !enabled && "opacity-50 pointer-events-none"
            )}
            data-interactive="true"
          >
            <Checkbox
              id="top-scenes-toggle"
              checked={topScenesEnabled}
              onCheckedChange={(checked) => onTopScenesChange(Boolean(checked))}
              disabled={!enabled}
            />
            <div className="space-y-0.5">
              <Label
                htmlFor="top-scenes-toggle"
                className="text-sm font-medium cursor-pointer text-white"
              >
                Create Top Scenes compilation
              </Label>
              <p className="text-xs text-muted-foreground">
                Combine selected scenes (max 5) with countdown overlay (5, 4, 3, 2, 1)
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
