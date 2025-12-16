"use client";

import { cn } from "@/lib/utils";

import type { StaticPosition } from "./types";

interface StaticPositionSelectorProps {
  position: StaticPosition;
  onChange: (next: StaticPosition) => void;
  disabled?: boolean;
}

const POSITIONS: { value: StaticPosition; label: string }[] = [
  { value: "left", label: "L" },
  { value: "center", label: "C" },
  { value: "right", label: "R" },
];

export function StaticPositionSelector({
  position,
  onChange,
  disabled,
}: StaticPositionSelectorProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-center gap-0.5 mt-2",
        disabled && "opacity-50 pointer-events-none"
      )}
      data-interactive="true"
    >
      <span className="text-[10px] text-muted-foreground mr-2">Position:</span>
      <div className="inline-flex rounded-full bg-slate-800/80 p-0.5 border border-white/10">
        {POSITIONS.map((pos) => (
          <button
            key={pos.value}
            type="button"
            onClick={() => onChange(pos.value)}
            className={cn(
              "px-2.5 py-0.5 text-[10px] font-medium rounded-full transition-all",
              position === pos.value
                ? "bg-indigo-500 text-white shadow-sm"
                : "text-muted-foreground hover:text-white hover:bg-white/5"
            )}
            disabled={disabled}
          >
            {pos.label}
          </button>
        ))}
      </div>
    </div>
  );
}
