"use client";

import { useEffect, useState } from "react";

import * as Slider from "@radix-ui/react-slider";

import { cn } from "@/lib/utils";

import type { StreamerSplitConfig } from "./types";

interface StreamerSplitConfiguratorProps {
  config: StreamerSplitConfig;
  onChange: (next: StreamerSplitConfig) => void;
  disabled?: boolean;
}

// Generate zoom levels from 1x to 15x with 0.5x increments
const ZOOM_LEVELS: number[] = Array.from({ length: 39 }, (_, i) => 1.0 + i * 0.5);

export function StreamerSplitConfigurator({
  config,
  onChange,
  disabled,
}: StreamerSplitConfiguratorProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState<number | null>(null);

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

  // Grid is 16x9 = 144 cells (widescreen to match 16:9 YouTube source)
  // indices 0..143
  const GRID_COLS = 16;
  const GRID_ROWS = 9;

  const handleGridMouseDown = (index: number) => {
    if (disabled) return;
    setIsDragging(true);
    setDragStart(index);
    // Start new selection with just this cell
    updateGridSelection([index]);
  };

  const handleGridMouseEnter = (index: number) => {
    if (!isDragging || disabled || dragStart === null) return;

    // Calculate rectangle from dragStart to current index
    const startX = dragStart % GRID_COLS;
    const startY = Math.floor(dragStart / GRID_COLS);
    const endX = index % GRID_COLS;
    const endY = Math.floor(index / GRID_COLS);

    const minX = Math.min(startX, endX);
    const maxX = Math.max(startX, endX);
    const minY = Math.min(startY, endY);
    const maxY = Math.max(startY, endY);

    const newSelection: number[] = [];
    for (let y = minY; y <= maxY; y++) {
      for (let x = minX; x <= maxX; x++) {
        newSelection.push(y * GRID_COLS + x);
      }
    }
    updateGridSelection(newSelection);
  };

  const handleGridMouseUp = () => {
    setIsDragging(false);
    setDragStart(null);
  };

  // Convert grid selection to manual crop rect and update config
  const updateGridSelection = (selection: number[]) => {
    if (selection.length === 0) {
      onChange({ ...config, gridSelection: [], manualCrop: undefined });
      return;
    }

    // Calculate bounding box of selection
    let minX = GRID_COLS;
    let maxX = 0;
    let minY = GRID_ROWS;
    let maxY = 0;

    selection.forEach((idx) => {
      const x = idx % GRID_COLS;
      const y = Math.floor(idx / GRID_COLS);
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x);
      minY = Math.min(minY, y);
      maxY = Math.max(maxY, y);
    });

    // Normalized coordinates (0-1)
    // Add 1 to max to include the width/height of the last cell
    const manualCrop = {
      x: minX / GRID_COLS,
      y: minY / GRID_ROWS,
      width: (maxX - minX + 1) / GRID_COLS,
      height: (maxY - minY + 1) / GRID_ROWS,
    };

    onChange({
      ...config,
      gridSelection: selection,
      manualCrop,
    });
  };

  useEffect(() => {
    const handleGlobalMouseUp = () => {
      if (isDragging) {
        setIsDragging(false);
        setDragStart(null);
      }
    };
    window.addEventListener("mouseup", handleGlobalMouseUp);
    return () => window.removeEventListener("mouseup", handleGlobalMouseUp);
  }, [isDragging]);

  return (
    <div
      className={cn(
        "mt-3 space-y-3 rounded-lg border border-white/10 bg-slate-900/60 p-3",
        disabled && "opacity-50 pointer-events-none"
      )}
      data-interactive="true"
    >
      <div className="flex items-center justify-between">
        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">
          Top Panel – Webcam Position
        </div>
        <div className="text-[10px] text-muted-foreground">Drag to select region</div>
      </div>

      {/* Position Grid - 9x9 visual selector */}
      <div className="flex items-start gap-4">
        <div
          className="grid grid-cols-16 gap-[1px] p-1 rounded-lg bg-slate-800/80 border border-white/10 select-none touch-none"
          style={{ gridTemplateColumns: `repeat(${GRID_COLS}, minmax(0, 1fr))` }}
          onMouseLeave={handleGridMouseUp}
        >
          {Array.from({ length: GRID_COLS * GRID_ROWS }).map((_, i) => {
            const isSelected = config.gridSelection?.includes(i);
            return (
              <div
                key={i}
                onMouseDown={() => handleGridMouseDown(i)}
                onMouseEnter={() => handleGridMouseEnter(i)}
                className={cn(
                  "w-3 h-3 rounded-[1px] transition-colors cursor-pointer",
                  isSelected ? "bg-indigo-500" : "bg-slate-700/30 hover:bg-slate-700/60"
                )}
              />
            );
          })}
        </div>

        <div className="flex-1 space-y-2">
          {config.manualCrop ? (
            <div className="space-y-1">
              <div className="text-[10px] text-muted-foreground">
                Custom Region Selected
              </div>
              <div className="text-xs font-medium text-white">
                {Math.round(config.manualCrop.width * 100)}% ×{" "}
                {Math.round(config.manualCrop.height * 100)}%
              </div>
            </div>
          ) : (
            <div className="text-[10px] text-muted-foreground italic">
              Select a region on the grid to position your camera manually.
            </div>
          )}
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
