import { Activity, Crown, Gauge, ScanFace, Sparkles } from "lucide-react";

import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { cn } from "@/lib/utils";

import type { ComponentType } from "react";

export type AiLevel = "static" | "motion" | "basic_face" | "active_face";

type LegacyAiLevel =
  | "fast"
  | "face_aware"
  | "face_tracking"
  | "motion_aware"
  | "premium";

const LEGACY_TO_CURRENT: Record<LegacyAiLevel, AiLevel> = {
  fast: "static",
  face_aware: "basic_face",
  face_tracking: "basic_face",
  motion_aware: "motion",
  premium: "active_face",
};

/** Tiers that require at least a Pro plan (Smart Face) */
const PRO_ONLY_TIERS: AiLevel[] = ["basic_face"];

/** Tiers that require a Studio plan (Active Speaker / Premium) */
const STUDIO_ONLY_TIERS: AiLevel[] = ["active_face"];

/** Check if a tier requires a paid plan */
export function isTierGated(tier: AiLevel, userPlan?: string): boolean {
  const hasStudioPlan = userPlan === "studio";
  const hasProPlan = userPlan === "pro" || userPlan === "studio";

  if (STUDIO_ONLY_TIERS.includes(tier) && !hasStudioPlan) return true;
  if (PRO_ONLY_TIERS.includes(tier) && !hasProPlan) return true;
  return false;
}

/** Get the required plan name for a tier */
export function getRequiredPlan(tier: AiLevel): "Pro" | "Studio" | null {
  if (STUDIO_ONLY_TIERS.includes(tier)) return "Studio";
  if (PRO_ONLY_TIERS.includes(tier)) return "Pro";
  return null;
}

interface AiAssistanceSliderProps {
  value: AiLevel | LegacyAiLevel;
  onChange: (value: AiLevel) => void;
  /** User's current plan - used to show plan badges. Undefined means not logged in. */
  userPlan?: string;
}

const steps: {
  value: AiLevel;
  label: string;
  shortLabel: string;
  description: string;
  icon: ComponentType<{ className?: string }>;
}[] = [
  {
    value: "static",
    label: "Static",
    shortLabel: "Static",
    description: "Static crop or split. Fastest processing, no AI.",
    icon: Gauge,
  },
  {
    value: "motion",
    label: "Motion",
    shortLabel: "Motion",
    description: "Follows movement & gestures using fast heuristics (no neural nets).",
    icon: Activity,
  },
  {
    value: "basic_face",
    label: "Smart Face",
    shortLabel: "Smart",
    description: "AI face detection keeps the main face centered.",
    icon: ScanFace,
  },
  {
    value: "active_face",
    label: "Active Speaker (Premium)",
    shortLabel: "Premium",
    description: "Premium face mesh tracking for whoever is actively speaking.",
    icon: Sparkles,
  },
];

export function AiAssistanceSlider({
  value,
  onChange,
  userPlan,
}: AiAssistanceSliderProps) {
  const normalizedValue =
    LEGACY_TO_CURRENT[value as LegacyAiLevel] ?? (value as AiLevel | undefined);

  const currentIndex = steps.findIndex((s) => s.value === normalizedValue);
  const defaultIndex = steps.findIndex((s) => s.value === "basic_face");
  const safeIndex = currentIndex !== -1 ? currentIndex : Math.max(defaultIndex, 0);
  const currentStep = steps[safeIndex];

  if (!currentStep) return null;

  // Allow selecting any tier - gating happens at launch time
  const handleSliderChange = (vals: number[]) => {
    const newIndex = vals[0];
    if (typeof newIndex === "number" && newIndex >= 0 && newIndex < steps.length) {
      onChange(steps[newIndex]!.value);
    }
  };

  return (
    <div className="w-full rounded-xl border border-brand-100/70 bg-white/95 shadow-xl shadow-brand-500/10 backdrop-blur-sm overflow-hidden dark:border-white/5 dark:bg-white/5 dark:shadow-none">
      {/* Header / Current Selection */}
      <div className="px-6 py-5 border-b border-brand-100/80 bg-white/90 backdrop-blur-sm dark:border-white/5 dark:bg-white/[0.02]">
        <div className="flex items-center justify-between mb-2">
          <Label className="text-base text-muted-foreground uppercase tracking-widest font-semibold text-[10px]">
            Detection Tier
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
        <h4 className="text-xl font-medium text-white mb-1">{currentStep.label}</h4>
        <p className="text-sm text-muted-foreground leading-relaxed opacity-90">
          {currentStep.description}
        </p>
      </div>

      {/* Slider Area */}
      <div className="px-6 py-8 bg-gradient-to-b from-brand-50/60 to-white dark:from-transparent dark:to-transparent">
        <div className="relative pb-16">
          <Slider
            defaultValue={[safeIndex]}
            value={[safeIndex]}
            max={steps.length - 1}
            step={1}
            onValueChange={handleSliderChange}
            className="cursor-pointer"
            aria-label="AI intelligence level"
          />
          {/* Tick marks positioned at exact slider stop percentages */}
          {steps.map((step, idx) => {
            const percentage = (idx / (steps.length - 1)) * 98;
            const planLabel = getRequiredPlan(step.value);
            const isGated = isTierGated(step.value, userPlan);
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
                    "hidden sm:inline text-[10px] uppercase font-bold tracking-wider transition-colors duration-300 whitespace-nowrap",
                    idx === safeIndex
                      ? "text-primary"
                      : "text-muted-foreground/50 group-hover:text-muted-foreground"
                  )}
                >
                  {step.shortLabel}
                  {planLabel && isGated && (
                    <Crown className="inline-block ml-0.5 h-2.5 w-2.5 text-amber-400" />
                  )}
                </span>
                <span className="sr-only">{step.label}</span>
              </button>
            );
          })}
        </div>

        {/* Mobile legend with icons to keep options legible */}
        <div className="mt-6 grid grid-cols-2 gap-3 sm:hidden" aria-hidden="true">
          {steps.map((step, idx) => {
            const Icon = step.icon;
            const isActive = idx === safeIndex;
            const planLabel = getRequiredPlan(step.value);
            const isGated = isTierGated(step.value, userPlan);
            return (
              <button
                key={step.value}
                type="button"
                onClick={() => onChange(step.value)}
                className={cn(
                  "flex items-center gap-2 rounded-lg border px-3 py-2 text-left transition-colors relative",
                  isActive
                    ? "border-brand-300 bg-brand-50 text-foreground shadow-sm dark:border-primary/60 dark:bg-primary/10 dark:text-white"
                    : "border-brand-100 bg-white text-muted-foreground hover:border-brand-200 hover:text-foreground dark:border-white/5 dark:bg-white/5 dark:text-muted-foreground dark:hover:border-white/20 dark:hover:text-white"
                )}
              >
                <Icon className="h-4 w-4 shrink-0" />
                <div className="flex flex-col">
                  <span className="text-xs font-semibold leading-tight">
                    {step.label}
                  </span>
                  {planLabel && isGated && (
                    <span
                      className={cn(
                        "text-[9px] font-semibold uppercase tracking-wide flex items-center gap-0.5",
                        planLabel === "Studio" ? "text-amber-400" : "text-blue-400"
                      )}
                    >
                      <Crown className="h-2.5 w-2.5" />
                      {planLabel}
                    </span>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
