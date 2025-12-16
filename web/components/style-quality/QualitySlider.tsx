"use client";

import * as Slider from "@radix-ui/react-slider";
import { Lock } from "lucide-react";

import { cn } from "@/lib/utils";

import { PRO_ONLY_STYLES, STUDIO_ONLY_STYLES } from "./types";

import type { QualityLevel } from "./types";

interface QualitySliderProps {
  levels: QualityLevel[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  hasStudioPlan?: boolean;
  hasProPlan?: boolean;
}

export function QualitySlider({
  levels,
  value,
  onChange,
  disabled,
  hasStudioPlan,
  hasProPlan,
}: QualitySliderProps) {
  const currentIndex = levels.findIndex((lvl) => lvl.value === value);
  const safeIndex = currentIndex >= 0 ? currentIndex : 0;
  const current = levels.at(safeIndex) ?? levels[0];
  const Icon = current?.icon;

  const isLocked = (style: string): boolean => {
    const normalized = style.toLowerCase();
    if (STUDIO_ONLY_STYLES.includes(normalized)) return !hasStudioPlan;
    if (PRO_ONLY_STYLES.includes(normalized)) return !hasProPlan && !hasStudioPlan;
    return false;
  };

  const handleSliderChange = (values: number[]) => {
    const idx = values[0];
    if (idx === undefined || idx < 0 || idx >= levels.length) return;
    const level = levels.at(idx);
    if (!level || isLocked(level.value)) return;
    onChange(level.value);
  };

  return (
    <div className={cn("space-y-3", disabled && "opacity-50 pointer-events-none")}>
      {/* Current level display */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {Icon && <Icon className="h-4 w-4 text-indigo-400" />}
          <span className="text-sm font-medium text-white">{current?.label}</span>
          {isLocked(value) && <Lock className="h-3 w-3 text-amber-400" />}
        </div>
        <span className="text-xs text-muted-foreground">{current?.helper}</span>
      </div>

      {/* Slider */}
      <Slider.Root
        className="relative flex h-5 w-full touch-none select-none items-center"
        value={[safeIndex]}
        max={levels.length - 1}
        step={1}
        onValueChange={handleSliderChange}
        disabled={disabled}
      >
        <Slider.Track className="relative h-1.5 w-full grow overflow-hidden rounded-full bg-slate-800">
          <Slider.Range className="absolute h-full bg-gradient-to-r from-indigo-500 to-purple-500" />
        </Slider.Track>
        <Slider.Thumb
          className={cn(
            "block h-4 w-4 rounded-full border-2 border-indigo-500 bg-white shadow-lg",
            "ring-offset-slate-950 transition-colors",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-offset-2",
            "disabled:pointer-events-none disabled:opacity-50"
          )}
        />
      </Slider.Root>

      {/* Level indicators */}
      <div className="flex justify-between px-0.5">
        {levels.map((lvl, idx) => {
          const LvlIcon = lvl.icon;
          const locked = isLocked(lvl.value);
          const isActive = idx === safeIndex;

          const isDisabled = (disabled ?? false) || locked;

          return (
            <button
              key={lvl.value}
              type="button"
              onClick={() => !locked && onChange(lvl.value)}
              disabled={isDisabled}
              className={cn(
                "flex flex-col items-center gap-0.5 transition-opacity",
                isActive ? "opacity-100" : "opacity-50 hover:opacity-75",
                locked && "cursor-not-allowed"
              )}
              title={locked ? "Upgrade to unlock" : lvl.label}
            >
              {LvlIcon && (
                <LvlIcon
                  className={cn(
                    "h-3.5 w-3.5",
                    isActive ? "text-indigo-400" : "text-slate-500"
                  )}
                />
              )}
              {locked && <Lock className="h-2.5 w-2.5 text-amber-400" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}
