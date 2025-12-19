"use client";

import { useMemo } from "react";

import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import {
    ADDON_CREDIT_COSTS,
    formatCredits,
    getStyleCreditCost,
} from "@/lib/credits/pricing";
import { cn } from "@/lib/utils";

import { FULL_LEVELS, SPLIT_LEVELS } from "./constants";
import { LayoutCard } from "./LayoutCard";
import { DEFAULT_SELECTION } from "./types";
import { selectionToStyles, stylesToSelection } from "./utils";

import type { LayoutQualitySelection, StreamerSplitConfig } from "./types";

interface StyleQualitySelectorProps {
  selectedStyles: string[];
  onChange: (styles: string[]) => void;
  disabled?: boolean;
  className?: string;
  /** User's current plan - used to gate studio-only features */
  userPlan?: string;
  enableObjectDetection?: boolean;
  onEnableObjectDetectionChange?: (next: boolean) => void;
  /** StreamerSplit configuration */
  streamerSplitConfig?: StreamerSplitConfig;
  onStreamerSplitConfigChange?: (config: StreamerSplitConfig) => void;
  /** Callback when topScenesEnabled changes */
  onTopScenesEnabledChange?: (enabled: boolean) => void;
  /** Ordered list of scene IDs for Top Scenes compilation */
  compilationScenes?: number[];
  /** Map of scene ID to scene title */
  sceneTitles?: Map<number, string>;
  /** Callback when user removes a scene from the compilation */
  onRemoveCompilationScene?: (sceneId: number) => void;
  /** Cut silent parts from clips using VAD */
  cutSilentParts?: boolean;
  /** Callback when cutSilentParts changes */
  onCutSilentPartsChange?: (enabled: boolean) => void;
}

export function StyleQualitySelector({
  selectedStyles,
  onChange,
  disabled = false,
  className,
  userPlan,
  enableObjectDetection,
  onEnableObjectDetectionChange,
  streamerSplitConfig: externalConfig,
  onStreamerSplitConfigChange,
  onTopScenesEnabledChange,
  compilationScenes,
  sceneTitles,
  onRemoveCompilationScene,
  cutSilentParts,
  onCutSilentPartsChange,
}: StyleQualitySelectorProps) {
  const hasStudioPlan = userPlan === "studio";
  const hasProPlan = userPlan === "pro" || userPlan === "studio";
  const silentCostLabel = `${formatCredits(ADDON_CREDIT_COSTS.silentRemoverPerScene)} /scene`;
  const originalCostLabel = `${formatCredits(getStyleCreditCost("original"))} /scene`;

  // Use external config if provided, otherwise use internal state
  const baseSelection = useMemo(
    () => stylesToSelection(selectedStyles, DEFAULT_SELECTION),
    [selectedStyles]
  );

  const selection = useMemo(
    () => ({
      ...baseSelection,
      streamerSplitConfig: externalConfig ?? baseSelection.streamerSplitConfig,
    }),
    [baseSelection, externalConfig]
  );

  const updateSelection = (patch: Partial<LayoutQualitySelection>) => {
    const next = { ...selection, ...patch };
    const styles = selectionToStyles(next);
    onChange(styles);

    // Notify parent of streamerSplitConfig changes
    if (patch.streamerSplitConfig && onStreamerSplitConfigChange) {
      onStreamerSplitConfigChange(patch.streamerSplitConfig);
    }

    // Notify parent of topScenesEnabled changes
    if (patch.topScenesEnabled !== undefined && onTopScenesEnabledChange) {
      onTopScenesEnabledChange(patch.topScenesEnabled);
    }
  };

  return (
    <Card
      className={cn("glass shadow-2xl border-indigo-500/30 bg-slate-950/60", className)}
    >
      <CardHeader className="space-y-2">
        <CardTitle className="text-xl text-white">
          Output Layout &amp; Quality
        </CardTitle>
        <CardDescription className="text-sm text-muted-foreground">
          Choose layout and AI processing level. Enable Split, Full, or both, plus
          optionally keep the Original.
        </CardDescription>
      </CardHeader>
      <CardContent
        className={cn("space-y-6", disabled && "opacity-70 pointer-events-none")}
      >
        <div className="grid gap-4 lg:grid-cols-2">
          {/* Split View Card */}
          <LayoutCard
            title="Split View (9×16)"
            enabled={selection.splitEnabled}
            onToggle={(next) => updateSelection({ splitEnabled: next })}
            levelValue={selection.splitStyle}
            levels={SPLIT_LEVELS}
            onLevelChange={(next) =>
              updateSelection({ splitStyle: next, splitEnabled: true })
            }
            hasStudioPlan={hasStudioPlan}
            hasProPlan={hasProPlan}
            streamerSplitConfig={selection.streamerSplitConfig}
            onStreamerSplitConfigChange={(next) =>
              updateSelection({ streamerSplitConfig: next })
            }
          />

          {/* Full View Card */}
          <div className="space-y-3">
            <LayoutCard
              title="Full View (9×16)"
              enabled={selection.fullEnabled}
              onToggle={(next) => updateSelection({ fullEnabled: next })}
              levelValue={selection.fullStyle}
              levels={FULL_LEVELS}
              onLevelChange={(next) =>
                updateSelection({ fullStyle: next, fullEnabled: true })
              }
              hasStudioPlan={hasStudioPlan}
              hasProPlan={hasProPlan}
              staticPosition={selection.staticPosition}
              onStaticPositionChange={(next) =>
                updateSelection({ staticPosition: next })
              }
              topScenesEnabled={selection.topScenesEnabled}
              onTopScenesChange={(next) => updateSelection({ topScenesEnabled: next })}
              compilationScenes={compilationScenes}
              sceneTitles={sceneTitles}
              onRemoveCompilationScene={onRemoveCompilationScene}
            />

            {/* Object Detection checkbox for Cinematic style */}
            {selection.fullEnabled &&
              selection.fullStyle?.toLowerCase().includes("cinematic") &&
              onEnableObjectDetectionChange && (
                <div className="flex items-start gap-3 rounded-lg border border-white/10 bg-slate-900/70 p-3">
                  <Checkbox
                    id="enableObjectDetection"
                    checked={enableObjectDetection}
                    onCheckedChange={(checked) =>
                      onEnableObjectDetectionChange(Boolean(checked))
                    }
                    disabled={disabled}
                    data-interactive="true"
                  />
                  <div className="space-y-0.5">
                    <Label
                      htmlFor="enableObjectDetection"
                      className="text-sm font-medium cursor-pointer text-white"
                    >
                      Use object detection
                    </Label>
                    <p className="text-xs text-muted-foreground">
                      Enable object detection to track objects on scenes without faces
                      for improved camera motion (slower)
                    </p>
                  </div>
                </div>
              )}
          </div>
        </div>

        {/* Cut silent parts checkbox */}
        <div className="space-y-3 rounded-xl border border-white/10 bg-slate-900/60 p-4">
          <label
            className="flex items-start gap-3 text-sm text-white"
            htmlFor="cut-silent-parts"
          >
            <Checkbox
              checked={cutSilentParts ?? selection.cutSilentParts}
              onCheckedChange={(checked) => {
                const enabled = Boolean(checked);
                onCutSilentPartsChange?.(enabled);
                updateSelection({ cutSilentParts: enabled });
              }}
              id="cut-silent-parts"
              disabled={disabled}
            />
            <div className="space-y-0.5">
              <div className="flex items-center gap-2">
                <span className="font-medium">
                  Cut silent parts for more dynamic scenes
                </span>
                <span className="rounded-full bg-white/10 px-2 py-0.5 text-[11px] font-semibold text-indigo-100">
                  {silentCostLabel}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                Remove sections without speech (applies to all styles)
              </p>
            </div>
          </label>
        </div>

        {/* Original checkbox */}
        <div className="space-y-3 rounded-xl border border-white/10 bg-slate-900/60 p-4">
          <label
            className="flex items-start gap-3 text-sm text-white"
            htmlFor="include-original"
          >
            <Checkbox
              checked={selection.includeOriginal}
              onCheckedChange={(checked) =>
                updateSelection({ includeOriginal: Boolean(checked) })
              }
              id="include-original"
            />
            <div className="space-y-0.5">
              <div className="flex items-center gap-2">
                <span className="font-medium">Also export Original (no cropping)</span>
                <span className="rounded-full bg-white/10 px-2 py-0.5 text-[11px] font-semibold text-indigo-100">
                  {originalCostLabel}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">Optional extra output</p>
            </div>
          </label>

          <p className="text-xs text-muted-foreground">
            You can enable Split, Full, or both. Each layout uses one processing level.
            Optionally include the Original.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

export const STYLE_LEVELS = {
  split: SPLIT_LEVELS,
  full: FULL_LEVELS,
};

// Re-export types for backward compatibility
export type {
    HorizontalPosition,
    LayoutQualitySelection,
    StaticPosition,
    StreamerSplitConfig,
    VerticalPosition
} from "./types";

export { DEFAULT_SELECTION, DEFAULT_STREAMER_SPLIT_CONFIG } from "./types";
export { selectionToStyles, stylesToSelection } from "./utils";

