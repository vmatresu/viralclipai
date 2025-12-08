"use client";

import { cn } from "@/lib/utils";

const STYLES = [
  // Fast/static styles
  { value: "split", label: "Split View", subtitle: "Top/Bottom", speed: "⚡ Fast" },
  {
    value: "split_fast",
    label: "Split View (Fast)",
    subtitle: "No Face Detection",
    speed: "⚡ Fastest",
  },
  {
    value: "left_focus",
    label: "Left Focus",
    subtitle: "Full Height",
    speed: "⚡ Fast",
  },
  {
    value: "right_focus",
    label: "Right Focus",
    subtitle: "Full Height",
    speed: "⚡ Fast",
  },

  // Intelligent single-view styles (face tracking only - no audio dependency)
  {
    value: "intelligent",
    label: "Intelligent Crop",
    subtitle: "Face Tracking",
    speed: "🧠 Standard",
  },

  // Intelligent split-view styles (face tracking only - no audio dependency)
  {
    value: "intelligent_split",
    label: "Smart Split",
    subtitle: "Split + Face Tracking",
    speed: "🧠 Standard",
  },

  // Special options
  { value: "original", label: "Original", subtitle: "No Cropping", speed: "⚡ Fast" },
  { value: "all", label: "All Styles", subtitle: "Generate All", speed: "⏱️ Varies" },
] as const;

interface StyleSelectorProps {
  selectedStyles: Set<string>;
  disabled?: boolean;
  onStyleToggle: (style: string) => void;
}

export function StyleSelector({
  selectedStyles,
  disabled = false,
  onStyleToggle,
}: StyleSelectorProps) {
  const toggleStyle = (styleValue: string) => {
    if (styleValue === "all") {
      // "All Styles" is a special case - toggle all available styles
      const allStyleValues = STYLES.filter((s) => s.value !== "all").map(
        (s) => s.value
      );
      if (
        selectedStyles.size === allStyleValues.length &&
        allStyleValues.every((s) => selectedStyles.has(s))
      ) {
        // If all are selected, deselect all
        onStyleToggle("all"); // This will trigger the parent to clear all
      } else {
        // Otherwise, select all
        allStyleValues.forEach((style) => {
          if (!selectedStyles.has(style)) {
            onStyleToggle(style);
          }
        });
      }
    } else {
      // Toggle individual style
      onStyleToggle(styleValue);
    }
  };

  return (
    <div className="space-y-3">
      <h3 className="text-sm font-semibold">Select Styles</h3>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {STYLES.map((s) => {
          const isSelected =
            s.value === "all"
              ? STYLES.filter((st) => st.value !== "all").every((st) =>
                  selectedStyles.has(st.value)
                )
              : selectedStyles.has(s.value);
          return (
            <label
              key={s.value}
              htmlFor={`style-${s.value}`}
              className="cursor-pointer"
            >
              <input
                id={`style-${s.value}`}
                type="checkbox"
                checked={isSelected}
                onChange={() => toggleStyle(s.value)}
                className="peer sr-only"
                aria-label={`${s.label} - ${s.subtitle}`}
                disabled={disabled}
              />
              <div
                className={cn(
                  "p-3 rounded-xl border transition-all text-center h-full flex flex-col justify-between",
                  "bg-card hover:bg-accent",
                  isSelected && "border-primary bg-primary/10",
                  disabled && "opacity-50 cursor-not-allowed"
                )}
              >
                <div>
                  <span className="font-medium block text-sm">{s.label}</span>
                  <span className="block text-xs text-muted-foreground mt-0.5">
                    {s.subtitle}
                  </span>
                </div>
                <span className="block text-xs text-muted-foreground mt-2 opacity-75">
                  {s.speed}
                </span>
              </div>
            </label>
          );
        })}
      </div>
      {selectedStyles.size === 0 && (
        <p className="text-sm text-muted-foreground">
          Please select at least one style
        </p>
      )}
      {selectedStyles.size > 0 && (
        <div className="mt-4">
          <p className="text-sm font-medium mb-2">SELECTED STYLES:</p>
          <div className="flex flex-wrap gap-2">
            {Array.from(selectedStyles).map((styleValue) => {
              const style = STYLES.find((s) => s.value === styleValue);
              return style ? (
                <span
                  key={styleValue}
                  className="inline-flex items-center px-3 py-1 rounded-full text-sm bg-primary/10 text-primary border border-primary/20"
                >
                  {style.label}
                </span>
              ) : null;
            })}
          </div>
        </div>
      )}
    </div>
  );
}
