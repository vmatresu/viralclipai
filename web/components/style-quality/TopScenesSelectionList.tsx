"use client";

import { X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface TopScenesSelectionListProps {
  /** Ordered list of scene IDs in selection order */
  orderedSceneIds: number[];
  /** Map of scene ID to scene title */
  sceneTitles: Map<number, string>;
  /** Callback when user removes a scene from the compilation */
  onRemoveScene?: (sceneId: number) => void;
  /** Whether the list is disabled */
  disabled?: boolean;
  /** Maximum number of allowed scenes */
  maxScenes?: number;
}

/**
 * Displays the ordered list of selected scenes for Top Scenes compilation.
 * Shows scenes in the order they were selected, with ability to remove individual scenes.
 */
export function TopScenesSelectionList({
  orderedSceneIds,
  sceneTitles,
  onRemoveScene,
  disabled = false,
  maxScenes = 10,
}: TopScenesSelectionListProps) {
  if (orderedSceneIds.length === 0) {
    return (
      <p className="text-xs text-muted-foreground italic mt-2">
        Select scenes below to add them to the compilation
      </p>
    );
  }

  const isAtLimit = orderedSceneIds.length >= maxScenes;

  return (
    <div className="mt-2 space-y-2">
      {/* Scene count indicator */}
      <div className="flex items-center justify-between">
        <p
          className={cn(
            "text-xs font-medium",
            isAtLimit ? "text-amber-400" : "text-muted-foreground"
          )}
        >
          {orderedSceneIds.length}/{maxScenes} scenes selected
        </p>
        {isAtLimit && <span className="text-xs text-amber-400">Maximum reached</span>}
      </div>

      {/* Ordered scene list - displayed in video output order (last selected first with highest countdown) */}
      <div className="flex flex-wrap gap-1.5">
        {[...orderedSceneIds].reverse().map((sceneId, index) => {
          const title = sceneTitles.get(sceneId) ?? `Scene ${sceneId}`;
          const truncatedTitle =
            title.length > 20 ? `${title.substring(0, 18)}…` : title;
          // Countdown number: matches video output order (highest first)
          const countdownNum = orderedSceneIds.length - index;

          return (
            <div
              key={sceneId}
              className={cn(
                "inline-flex items-center gap-1 px-2 py-1 rounded-md text-xs",
                "bg-indigo-500/20 border border-indigo-500/30 text-white",
                disabled && "opacity-50"
              )}
              title={`${title} - Countdown #${countdownNum}`}
            >
              <span className="font-semibold text-indigo-400">#{countdownNum}</span>
              <span className="truncate max-w-[100px]">{truncatedTitle}</span>
              {onRemoveScene && !disabled && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-4 w-4 p-0 ml-0.5 hover:bg-indigo-500/30 rounded-full"
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveScene(sceneId);
                  }}
                  aria-label={`Remove ${title} from compilation`}
                >
                  <X className="h-3 w-3" />
                </Button>
              )}
            </div>
          );
        })}
      </div>

      {/* Countdown order explanation */}
      <p className="text-xs text-muted-foreground">
        Output order: Last selected scene appears first with highest countdown number
      </p>
    </div>
  );
}
