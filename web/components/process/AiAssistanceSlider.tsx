import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { cn } from "@/lib/utils";

export type AiLevel =
  | "fast"
  | "face_aware"
  | "face_tracking"
  | "motion_aware"
  | "premium";

interface AiAssistanceSliderProps {
  value: AiLevel;
  onChange: (value: AiLevel) => void;
}

const steps: {
  value: AiLevel;
  label: string;
  shortLabel: string;
  description: string;
}[] = [
  {
    value: "fast",
    label: "Fast",
    shortLabel: "Fast",
    description: "Static center crop. Fastest processing, no AI analysis.",
  },
  {
    value: "face_aware",
    label: "Face-aware",
    shortLabel: "Face-aware",
    description: "Heuristic based framing. Detects faces but doesn't track movement.",
  },
  {
    value: "face_tracking",
    label: "Tracking",
    shortLabel: "Tracking",
    description: "Keeps speaker in frame. Smoothly follows active speaker.",
  },
  {
    value: "motion_aware",
    label: "Motion",
    shortLabel: "Motion",
    description: "Follows movement & gestures. Dynamic camera work.",
  },
  {
    value: "premium",
    label: "Premium AI Detection",
    shortLabel: "Premium",
    description: "Full scene understanding. Best framing, timing & direction.",
  },
];

export function AiAssistanceSlider({ value, onChange }: AiAssistanceSliderProps) {
  // Convert string value to index for the slider (0-4)
  const currentIndex = steps.findIndex((s) => s.value === value);
  // Default to face_aware (index 1) if invalid or not set
  const safeIndex = currentIndex !== -1 ? currentIndex : 1;
  const currentStep = steps[safeIndex];

  if (!currentStep) return null;

  const handleSliderChange = (vals: number[]) => {
    const newIndex = vals[0];
    if (typeof newIndex === "number" && newIndex >= 0 && newIndex < steps.length) {
      onChange(steps[newIndex]!.value);
    }
  };

  return (
    <div className="w-full bg-white/5 border border-white/5 rounded-xl backdrop-blur-sm overflow-hidden">
      {/* Header / Current Selection */}
      <div className="px-6 py-5 border-b border-white/5 bg-white/[0.02]">
        <div className="flex items-center justify-between mb-2">
          <Label className="text-base text-muted-foreground uppercase tracking-widest font-semibold text-[10px]">
            Intelligence Level
          </Label>
          <span
            className={cn(
              "text-xs font-bold px-2 py-0.5 rounded-full border shadow-sm transition-colors uppercase tracking-wider",
              safeIndex >= 3
                ? "bg-primary/20 text-primary border-primary/20"
                : "bg-white/10 text-white border-white/10"
            )}
          >
            {currentStep.label}
          </span>
        </div>
        <h4 className="text-xl font-medium text-white mb-1">
          {currentStep.label} Mode
        </h4>
        <p className="text-sm text-muted-foreground leading-relaxed opacity-90">
          {currentStep.description}
        </p>
      </div>

      {/* Slider Area */}
      <div className="px-6 py-8">
        <div className="relative pb-16">
          <Slider
            defaultValue={[safeIndex]}
            value={[safeIndex]}
            max={steps.length - 1}
            step={1}
            onValueChange={handleSliderChange}
            className="cursor-pointer"
          />
          {/* Tick marks positioned at exact slider stop percentages */}
          {steps.map((step, idx) => {
            const percentage = (idx / (steps.length - 1)) * 98;
            return (
              <button
                key={step.value}
                type="button"
                className="absolute flex flex-col items-center gap-2 cursor-pointer group focus:outline-none focus-visible:ring-2 focus-visible:ring-primary rounded -translate-x-1/2"
                style={{ left: `${percentage}%`, top: "24px" }}
                onClick={() => onChange(step.value)}
              >
                <div
                  className={cn(
                    "w-1.5 h-1.5 rounded-full transition-all duration-300",
                    idx <= safeIndex
                      ? "bg-primary scale-125"
                      : "bg-white/20 group-hover:bg-white/40"
                  )}
                />
                <span
                  className={cn(
                    "text-[10px] uppercase font-bold tracking-wider transition-colors duration-300 whitespace-nowrap",
                    idx === safeIndex
                      ? "text-primary"
                      : "text-muted-foreground/50 group-hover:text-muted-foreground"
                  )}
                >
                  {step.shortLabel}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
