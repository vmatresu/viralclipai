"use client";

import * as Slider from "@radix-ui/react-slider";

import { cn } from "@/lib/utils";

import type {
  HorizontalPosition,
  StreamerSplitConfig,
  VerticalPosition,
} from "./types";

interface StreamerSplitConfiguratorProps {
  config: StreamerSplitConfig;
  onChange: (next: StreamerSplitConfig) => void;
  disabled?: boolean;
}

const HORIZONTAL_POSITIONS: { value: HorizontalPosition; label: string }[] = [
  { value: "left", label: "Left" },
  { value: "center", label: "Center" },
  { value: "right", label: "Right" },
];

const VERTICAL_POSITIONS: { value: VerticalPosition; label: string }[] = [
  { value: "top", label: "Top" },
  { value: "middle", label: "Middle" },
  { value: "bottom", label: "Bottom" },
];

// Generate zoom levels from 1x to 15x with 0.5x increments
const ZOOM_LEVELS: number[] = Array.from({ length: 39 }, (_, i) => 1.0 + i * 0.5);

export function StreamerSplitConfigurator({
  config,
  onChange,
  disabled,
}: StreamerSplitConfiguratorProps) {
  const zoomIndex = ZOOM_LEVELS.findIndex((z) => z === config.zoom);
  const clampedZoomIndex = Math.max(
    0,
    Math.min(zoomIndex >= 0 ? zoomIndex : 0, ZOOM_LEVELS.length - 1)
  );

  const handleZoomChange = (values: number[]) => {
    const idx = values[0];
    if (idx === undefined) return;
    const clampedIdx = Math.min(Math.max(idx, 0), ZOOM_LEVELS.length - 1);
    const newZoom = ZOOM_LEVELS.at(clampedIdx);
    if (newZoom !== undefined) {
      onChange({ ...config, zoom: newZoom });
    }
  };

  return (
    <div
      className={cn(
        "mt-3 space-y-3 rounded-lg border border-white/10 bg-slate-900/60 p-3",
        disabled && "opacity-50 pointer-events-none"
      )}
      data-interactive="true"
    >
      <div className="text-[11px] uppercase tracking-wide text-muted-foreground">
        Top Panel – Webcam Position
      </div>

      {/* Position Grid - 3x3 visual selector */}
      <div className="flex items-center gap-4">
        <div className="grid grid-cols-3 gap-1 p-1 rounded-lg bg-slate-800/80 border border-white/10">
          {VERTICAL_POSITIONS.map((vPos) =>
            HORIZONTAL_POSITIONS.map((hPos) => {
              const isSelected =
                config.positionX === hPos.value && config.positionY === vPos.value;
              return (
                <button
                  key={`${vPos.value}-${hPos.value}`}
                  type="button"
                  onClick={() =>
                    onChange({
                      ...config,
                      positionX: hPos.value,
                      positionY: vPos.value,
                    })
                  }
                  className={cn(
                    "w-7 h-7 rounded transition-all text-[9px] font-medium",
                    isSelected
                      ? "bg-indigo-500 text-white shadow-sm"
                      : "bg-slate-700/50 text-muted-foreground hover:bg-slate-700 hover:text-white"
                  )}
                  disabled={disabled}
                  title={`${vPos.label} ${hPos.label}`}
                >
                  {isSelected ? "●" : "○"}
                </button>
              );
            })
          )}
        </div>

        <div className="flex-1 space-y-1">
          <div className="text-[10px] text-muted-foreground">
            Position:{" "}
            <span className="text-white font-medium capitalize">
              {config.positionY} {config.positionX}
            </span>
          </div>
          <div className="text-[10px] text-muted-foreground">
            Select where your webcam is in the original video
          </div>
        </div>
      </div>

      {/* Zoom Level Slider */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <div className="text-[11px] uppercase tracking-wide text-muted-foreground">
            Zoom Level
          </div>
          <div className="text-sm font-medium text-white">{config.zoom}×</div>
        </div>
        <Slider.Root
          className="relative flex w-full select-none items-center py-2"
          value={[clampedZoomIndex]}
          min={0}
          max={ZOOM_LEVELS.length - 1}
          step={1}
          onValueChange={handleZoomChange}
          disabled={disabled}
          aria-label="Zoom level"
        >
          <Slider.Track className="relative h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
            <Slider.Range className="absolute h-full rounded-full bg-indigo-500" />
          </Slider.Track>
          <Slider.Thumb className="block h-4 w-4 rounded-full border border-white/70 bg-white shadow-md outline-none transition-transform focus:scale-110 focus:ring-2 focus:ring-indigo-400/60" />
        </Slider.Root>
        <div className="flex justify-between text-[10px] text-muted-foreground">
          <span>1×</span>
          <span>Higher zoom = closer crop</span>
          <span>15×</span>
        </div>
      </div>
    </div>
  );
}
