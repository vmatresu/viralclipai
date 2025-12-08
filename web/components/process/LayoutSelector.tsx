import { Check } from "lucide-react";

import { cn } from "@/lib/utils";

export type LayoutOption = "split" | "full";

interface LayoutSelectorProps {
  selectedLayout: LayoutOption;
  onSelect: (layout: LayoutOption) => void;
}

export function LayoutSelector({ selectedLayout, onSelect }: LayoutSelectorProps) {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <LayoutCard
        title="Split View (9:16)"
        description="Perfect for commentary & reactions. Top half video, bottom half gameplay/content."
        isSelected={selectedLayout === "split"}
        onClick={() => onSelect("split")}
        visual={<SplitViewVisual />}
      />
      <LayoutCard
        title="Full View (9:16)"
        description="Immersive full-screen experience. Best for single-subject videos or vlogs."
        isSelected={selectedLayout === "full"}
        onClick={() => onSelect("full")}
        visual={<FullViewVisual />}
      />
    </div>
  );
}

interface LayoutCardProps {
  title: string;
  description: string;
  isSelected: boolean;
  onClick: () => void;
  visual: React.ReactNode;
}

function LayoutCard({
  title,
  description,
  isSelected,
  onClick,
  visual,
}: LayoutCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={isSelected}
      className={cn(
        "relative group cursor-pointer rounded-xl border-2 p-4 transition-all duration-200 ease-in-out text-left w-full",
        "hover:border-primary/50 hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
        isSelected
          ? "border-primary bg-primary/5 shadow-[0_0_20px_-10px_theme(colors.primary.DEFAULT)]"
          : "border-white/10 bg-white/5"
      )}
    >
      {isSelected && (
        <div className="absolute top-3 right-3 text-primary bg-white rounded-full p-0.5 shadow-sm">
          <Check className="h-4 w-4" />
        </div>
      )}

      <div className="flex gap-4">
        <div className="shrink-0 pt-1">{visual}</div>
        <div className="space-y-2">
          <h3
            className={cn(
              "font-medium text-lg",
              isSelected ? "text-primary" : "text-foreground"
            )}
          >
            {title}
          </h3>
          <p className="text-sm text-muted-foreground leading-relaxed opacity-90">
            {description}
          </p>
        </div>
      </div>
    </button>
  );
}

function SplitViewVisual() {
  return (
    <div className="w-12 h-20 bg-background border border-white/10 rounded-md overflow-hidden flex flex-col shadow-sm">
      <div className="h-1/2 bg-indigo-500/20 flex items-center justify-center border-b border-white/5">
        <div className="w-4 h-4 rounded-full bg-indigo-500/40" />
      </div>
      <div className="h-1/2 bg-emerald-500/10 flex items-center justify-center">
        <div className="w-6 h-2 rounded bg-emerald-500/20" />
      </div>
    </div>
  );
}

function FullViewVisual() {
  return (
    <div className="w-12 h-20 bg-background border border-white/10 rounded-md overflow-hidden shadow-sm relative">
      <div className="absolute inset-0 bg-gradient-to-br from-indigo-500/10 to-transparent p-2">
        <div className="w-full h-full flex flex-col items-center justify-center gap-1">
          <div className="w-4 h-4 rounded-full bg-indigo-500/20" />
          <div className="w-6 h-1 rounded bg-indigo-500/20" />
        </div>
      </div>
    </div>
  );
}
