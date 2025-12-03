"use client";

import { Button } from "@/components/ui/button";

const STYLES = [
  { value: "split", label: "Split View" },
  { value: "left_focus", label: "Left Focus" },
  { value: "right_focus", label: "Right Focus" },
  { value: "intelligent_split", label: "Intelligent Split" },
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
  return (
    <div className="space-y-3">
      <h3 className="text-sm font-semibold">Select Styles</h3>
      <div className="flex flex-wrap gap-2">
        {STYLES.map((style) => (
          <Button
            key={style.value}
            variant={selectedStyles.has(style.value) ? "default" : "outline"}
            size="sm"
            onClick={() => onStyleToggle(style.value)}
            disabled={disabled}
          >
            {selectedStyles.has(style.value) && "✓ "}
            {style.label}
          </Button>
        ))}
      </div>
    </div>
  );
}

