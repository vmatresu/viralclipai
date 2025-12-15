"use client";

import * as Slider from "@radix-ui/react-slider";
import { Activity, Film, ScanFace, Sparkles, Zap } from "lucide-react";
import {
  type ComponentType,
  type KeyboardEvent,
  type MouseEvent,
  useMemo,
} from "react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { normalizeStyleForSelection } from "@/lib/styleTiers";
import { cn } from "@/lib/utils";

type QualityLevel = {
  value: string;
  label: string;
  helper?: string;
  icon?: ComponentType<{ className?: string }>;
};

export type StaticPosition = "left" | "center" | "right";

export type LayoutQualitySelection = {
  splitEnabled: boolean;
  splitStyle: string;
  fullEnabled: boolean;
  fullStyle: string;
  /** Position for Static Full style (left_focus, center_focus, right_focus) */
  staticPosition: StaticPosition;
  includeOriginal: boolean;
};

const SPLIT_LEVELS: QualityLevel[] = [
  {
    value: "split_fast",
    label: "Static – Fast",
    helper: "Heuristic split, no AI",
    icon: Zap,
  },
  {
    value: "split",
    label: "Static – Balanced",
    helper: "Fixed split layout",
    icon: Zap,
  },
  {
    value: "intelligent_split_motion",
    label: "Motion",
    helper: "High-speed motion-aware split (no neural nets)",
    icon: Activity,
  },
  {
    value: "intelligent_split",
    label: "Smart Face",
    helper: "AI face framing for both panels",
    icon: ScanFace,
  },
  {
    value: "intelligent_split_speaker",
    label: "Active Speaker",
    helper: "Premium face mesh AI",
    icon: Sparkles,
  },
];

const FULL_LEVELS: QualityLevel[] = [
  {
    value: "center_focus",
    label: "Static",
    helper: "No AI, fixed crop position",
    icon: Zap,
  },
  {
    value: "intelligent_motion",
    label: "Motion",
    helper: "High-speed motion-aware crop (no neural nets)",
    icon: Activity,
  },
  {
    value: "intelligent",
    label: "Smart Face",
    helper: "AI face framing for main subject",
    icon: ScanFace,
  },
  {
    value: "intelligent_speaker",
    label: "Active Speaker",
    helper: "Premium face mesh AI for the active speaker",
    icon: Sparkles,
  },
  {
    value: "intelligent_cinematic",
    label: "Cinematic",
    helper: "Smooth camera motion",
    icon: Film,
  },
];

const splitValues = SPLIT_LEVELS.map((lvl) => lvl.value);
const fullValues = FULL_LEVELS.map((lvl) => lvl.value);

export const DEFAULT_SELECTION: LayoutQualitySelection = {
  splitEnabled: false,
  splitStyle: "intelligent_split",
  fullEnabled: true,
  fullStyle: "intelligent",
  staticPosition: "center",
  includeOriginal: false,
};

const STYLE_SELECTION_ALIASES: Record<string, string> = {
  intelligent_split_activity: "intelligent_split_speaker",
  intelligent_activity: "intelligent_speaker",
  intelligent_split_basic: "intelligent_split",
  intelligent_basic: "intelligent",
};

/** Map static position to backend style name */
const STATIC_POSITION_STYLES: Record<StaticPosition, string> = {
  left: "left_focus",
  center: "center_focus",
  right: "right_focus",
};

export function selectionToStyles(selection: LayoutQualitySelection): string[] {
  const styles = new Set<string>();

  if (selection.splitEnabled) {
    styles.add(selection.splitStyle);
  }

  if (selection.fullEnabled) {
    // For Static style, use the position-specific style name
    if (selection.fullStyle === "center_focus") {
      styles.add(STATIC_POSITION_STYLES[selection.staticPosition]);
    } else {
      styles.add(selection.fullStyle);
    }
  }

  if (selection.includeOriginal) {
    styles.add("original");
  }

  return Array.from(styles);
}

export function stylesToSelection(
  styles: string[],
  fallback: LayoutQualitySelection = DEFAULT_SELECTION
): LayoutQualitySelection {
  const normalized = (styles ?? []).map((s) => {
    const lowered = s.toLowerCase();
    // Static constant lookup with normalized string key
    // eslint-disable-next-line security/detect-object-injection
    const alias = STYLE_SELECTION_ALIASES[lowered];
    return alias ?? normalizeStyleForSelection(lowered) ?? lowered;
  });

  // Check for static position styles
  let staticPosition: StaticPosition = fallback.staticPosition;
  if (normalized.includes("left_focus")) staticPosition = "left";
  else if (normalized.includes("right_focus")) staticPosition = "right";
  else if (normalized.includes("center_focus")) staticPosition = "center";

  // Map position-specific styles to center_focus for UI display
  const normalizedForUI = normalized.map((s) =>
    ["left_focus", "right_focus"].includes(s) ? "center_focus" : s
  );

  const splitStyle =
    splitValues.find((val) => normalizedForUI.includes(val)) ?? fallback.splitStyle;
  const fullStyle =
    fullValues.find((val) => normalizedForUI.includes(val)) ?? fallback.fullStyle;
  const splitEnabled = normalizedForUI.some((s) => splitValues.includes(s));
  const fullEnabled =
    normalizedForUI.some((s) => fullValues.includes(s)) ||
    normalized.some((s) => ["left_focus", "right_focus", "center_focus"].includes(s));

  return {
    splitEnabled,
    fullEnabled,
    includeOriginal: normalized.includes("original"),
    splitStyle,
    fullStyle,
    staticPosition,
  };
}

/** Styles that require a studio plan (Active Speaker) */
const STUDIO_ONLY_STYLES: string[] = [];

/** Styles that require at least a pro plan (Smart Face, Motion, Cinematic) */
const PRO_ONLY_STYLES = [
  "intelligent",
  "intelligent_split",
  "intelligent_speaker",
  "intelligent_split_speaker",
  "intelligent_cinematic",
];

function QualitySlider({
  levels,
  value,
  onChange,
  disabled,
  hasStudioPlan,
  hasProPlan,
}: {
  levels: QualityLevel[];
  value: string;
  disabled?: boolean;
  onChange: (next: string) => void;
  hasStudioPlan?: boolean;
  hasProPlan?: boolean;
}) {
  const activeIndex = Math.max(
    levels.findIndex((level) => level.value === value),
    0
  );
  const getColumnsClass = () => {
    if (levels.length >= 6) return "grid-cols-6";
    if (levels.length >= 5) return "grid-cols-5";
    return "grid-cols-4";
  };
  const columnsClass = getColumnsClass();

  return (
    <div className={cn("space-y-3", disabled && "opacity-50 pointer-events-none")}>
      <div className="flex items-center justify-between text-[11px] uppercase tracking-wide text-muted-foreground">
        <span>Processing Quality</span>
        <span>Less AI → More AI</span>
      </div>

      <Slider.Root
        className="relative flex w-full select-none items-center px-1 py-4"
        value={[activeIndex]}
        min={0}
        max={levels.length - 1}
        step={1}
        onValueChange={(val) => {
          const idx = val?.[0] ?? 0;
          const clampedIndex = Math.min(Math.max(idx, 0), levels.length - 1);
          // levels is a trusted local constant; clamp protects bounds.
          // eslint-disable-next-line security/detect-object-injection
          const target = levels[clampedIndex];
          if (!target) return;
          // Prevent selecting studio-only styles without studio plan
          const isStudioLocked =
            STUDIO_ONLY_STYLES.includes(target.value) && !hasStudioPlan;
          // Prevent selecting pro-only styles without pro/studio plan
          const isProLocked = PRO_ONLY_STYLES.includes(target.value) && !hasProPlan;
          if (!isStudioLocked && !isProLocked) {
            onChange(target.value);
          }
        }}
        disabled={disabled}
        aria-label="Processing quality"
        data-interactive="true"
      >
        <Slider.Track className="relative h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
          <Slider.Range className="absolute h-full rounded-full bg-gradient-to-r from-fuchsia-500 via-indigo-500 to-cyan-400 shadow-[0_0_18px_rgba(129,140,248,0.45)]" />
        </Slider.Track>
        <Slider.Thumb className="relative z-10 h-5 w-5 rounded-full border border-white/70 bg-white shadow-[0_0_0_6px_rgba(99,102,241,0.35)] outline-none transition-transform focus:scale-110 focus:ring-2 focus:ring-fuchsia-400/60" />
      </Slider.Root>

      <div
        className={cn(
          "grid gap-2 text-center text-xs font-medium text-white/90",
          columnsClass
        )}
      >
        {levels.map((level, idx) => {
          const isActive = idx === activeIndex;
          const Icon = level.icon;
          const isStudioOnly = STUDIO_ONLY_STYLES.includes(level.value);
          const isProOnly = PRO_ONLY_STYLES.includes(level.value);
          const isStudioLocked = isStudioOnly && !hasStudioPlan;
          const isProLocked = isProOnly && !hasProPlan;
          const isLocked = isStudioLocked || isProLocked;
          const getPlanLabel = () => {
            if (isStudioOnly) return "Studio";
            if (isProOnly) return "Pro";
            return null;
          };
          const getLockTitle = () => {
            if (isStudioLocked) return "Studio plan required";
            if (isProLocked) return "Pro plan required";
            return undefined;
          };
          const planLabel = getPlanLabel();
          const lockTitle = getLockTitle();
          return (
            <button
              key={level.value}
              type="button"
              onClick={() => !isLocked && onChange(level.value)}
              // Using || for boolean OR is correct here - ?? would not work for false values
              // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing
              disabled={disabled || isLocked}
              className={cn(
                "space-y-0.5 rounded-lg border border-transparent px-2 py-1 transition-colors relative",
                !disabled && !isLocked && "hover:border-white/10 hover:bg-white/5",
                isActive &&
                  !isLocked &&
                  "border-fuchsia-400/40 bg-fuchsia-500/10 text-white",
                isLocked && "opacity-50 cursor-not-allowed"
              )}
              data-interactive="true"
              title={lockTitle}
            >
              <div className="flex items-center justify-center gap-1">
                {Icon && <Icon className="h-3 w-3" />}
                <span>{level.label}</span>
              </div>
              {level.helper && (
                <div className="text-[11px] text-muted-foreground leading-tight">
                  {level.helper}
                </div>
              )}
              {planLabel && (
                <div
                  className={cn(
                    "text-[9px] font-semibold uppercase tracking-wide mt-0.5",
                    isStudioOnly ? "text-amber-400" : "text-blue-400"
                  )}
                >
                  {planLabel}
                </div>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** Static position selector (L | C | R) for Static Full style */
function StaticPositionSelector({
  position,
  onChange,
  disabled,
}: {
  position: StaticPosition;
  onChange: (next: StaticPosition) => void;
  disabled?: boolean;
}) {
  const positions: { value: StaticPosition; label: string }[] = [
    { value: "left", label: "L" },
    { value: "center", label: "C" },
    { value: "right", label: "R" },
  ];

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
        {positions.map((pos) => (
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

function LayoutCard({
  title,
  enabled,
  onToggle,
  levelValue,
  levels,
  onLevelChange,
  hasStudioPlan,
  hasProPlan,
  staticPosition,
  onStaticPositionChange,
}: {
  title: string;
  enabled: boolean;
  onToggle: (next: boolean) => void;
  levelValue: string;
  levels: QualityLevel[];
  onLevelChange: (next: string) => void;
  hasStudioPlan?: boolean;
  hasProPlan?: boolean;
  staticPosition?: StaticPosition;
  onStaticPositionChange?: (next: StaticPosition) => void;
}) {
  const enableId = `${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-toggle`;

  const isInteractiveTarget = (event: MouseEvent<HTMLElement>) => {
    const target = event.target as HTMLElement | null;
    return Boolean(target?.closest("[data-interactive='true']"));
  };

  const handleCardToggle = () => onToggle(!enabled);
  const handleKeyToggle = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      handleCardToggle();
    }
  };

  return (
    <div
      className={cn(
        "relative rounded-2xl border bg-gradient-to-b from-slate-900/70 to-slate-950/70 p-4 shadow-xl transition-all",
        enabled
          ? "border-indigo-500/60 shadow-indigo-900/40"
          : "border-white/10 opacity-75 grayscale",
        "cursor-pointer focus:outline-none focus:ring-2 focus:ring-indigo-500/60 focus:ring-offset-2 focus:ring-offset-slate-950"
      )}
      role="button"
      tabIndex={0}
      aria-pressed={enabled}
      onClick={(event) => {
        if (isInteractiveTarget(event)) return;
        handleCardToggle();
      }}
      onKeyDown={handleKeyToggle}
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="text-sm font-semibold text-white">{title}</div>
          <p className="text-xs text-muted-foreground">
            9×16 vertical • Optimized for shorts
          </p>
        </div>
        <label
          className="flex items-center gap-2 text-xs font-medium text-white/80"
          htmlFor={enableId}
          data-interactive="true"
        >
          <Checkbox
            checked={enabled}
            onCheckedChange={(checked) => onToggle(Boolean(checked))}
            id={enableId}
          />
          Enable
        </label>
      </div>

      <div className="relative overflow-hidden rounded-xl border border-white/10 bg-gradient-to-b from-indigo-900/40 to-slate-950/60 shadow-inner flex items-center justify-center px-6 py-8">
        <div className="grid h-full w-full max-w-[180px] grid-cols-1 gap-2">
          {title.toLowerCase().includes("split") ? (
            <>
              <div className="h-20 rounded-lg bg-indigo-500/25 border border-white/10" />
              <div className="h-20 rounded-lg bg-emerald-500/20 border border-white/10" />
            </>
          ) : (
            <div className="h-40 rounded-xl bg-indigo-500/20 border border-white/10" />
          )}
        </div>
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-b from-white/5 via-transparent to-slate-950/40" />
      </div>

      <div className="mt-4" data-interactive="true">
        <QualitySlider
          levels={levels}
          value={levelValue}
          onChange={onLevelChange}
          disabled={!enabled}
          hasStudioPlan={hasStudioPlan}
          hasProPlan={hasProPlan}
        />
        {/* Show position selector for Static style */}
        {levelValue === "center_focus" && staticPosition && onStaticPositionChange && (
          <StaticPositionSelector
            position={staticPosition}
            onChange={onStaticPositionChange}
            disabled={!enabled}
          />
        )}
      </div>
    </div>
  );
}

interface StyleQualitySelectorProps {
  selectedStyles: string[];
  onChange: (styles: string[]) => void;
  disabled?: boolean;
  className?: string;
  /** User's current plan - used to gate studio-only features */
  userPlan?: string;
  enableObjectDetection?: boolean;
  onEnableObjectDetectionChange?: (next: boolean) => void;
}

export function StyleQualitySelector({
  selectedStyles,
  onChange,
  disabled = false,
  className,
  userPlan,
  enableObjectDetection,
  onEnableObjectDetectionChange,
}: StyleQualitySelectorProps) {
  const hasStudioPlan = userPlan === "studio";
  const hasProPlan = userPlan === "pro" || userPlan === "studio";
  const selection = useMemo(
    () => stylesToSelection(selectedStyles, DEFAULT_SELECTION),
    [selectedStyles]
  );

  const updateSelection = (patch: Partial<LayoutQualitySelection>) => {
    const next = { ...selection, ...patch };
    let styles = selectionToStyles(next);
    // If user disables Split and Full, re-enable Full to avoid empty selection.
    if (styles.length === 0) {
      styles = selectionToStyles({ ...next, fullEnabled: true });
    }
    onChange(styles);
  };

  return (
    <Card
      className={cn("glass shadow-2xl border-indigo-500/30 bg-slate-950/60", className)}
    >
      <CardHeader className="space-y-2">
        <CardTitle className="text-xl text-white">
          Output Layout &amp; Quality
        </CardTitle>
        <CardDescription className="text-sm text-muted-foreground">
          Choose layout and AI processing level. Enable Split, Full, or both, plus
          optionally keep the Original.
        </CardDescription>
      </CardHeader>
      <CardContent
        className={cn("space-y-6", disabled && "opacity-70 pointer-events-none")}
      >
        <div className="grid gap-4 lg:grid-cols-2">
          <LayoutCard
            title="Split View (9×16)"
            enabled={selection.splitEnabled}
            onToggle={(next) => updateSelection({ splitEnabled: next })}
            levelValue={selection.splitStyle}
            levels={SPLIT_LEVELS}
            onLevelChange={(next) =>
              updateSelection({ splitStyle: next, splitEnabled: true })
            }
            hasStudioPlan={hasStudioPlan}
            hasProPlan={hasProPlan}
          />

          <div className="space-y-3">
            <LayoutCard
              title="Full View (9×16)"
              enabled={selection.fullEnabled}
              onToggle={(next) => updateSelection({ fullEnabled: next })}
              levelValue={selection.fullStyle}
              levels={FULL_LEVELS}
              onLevelChange={(next) =>
                updateSelection({ fullStyle: next, fullEnabled: true })
              }
              hasStudioPlan={hasStudioPlan}
              hasProPlan={hasProPlan}
              staticPosition={selection.staticPosition}
              onStaticPositionChange={(next) =>
                updateSelection({ staticPosition: next })
              }
            />

            {selection.fullEnabled &&
              selection.fullStyle?.toLowerCase().includes("cinematic") &&
              onEnableObjectDetectionChange && (
                <div className="flex items-start gap-3 rounded-lg border border-white/10 bg-slate-900/70 p-3">
                  <Checkbox
                    id="enableObjectDetection"
                    checked={enableObjectDetection}
                    onCheckedChange={(checked) =>
                      onEnableObjectDetectionChange(Boolean(checked))
                    }
                    disabled={disabled}
                    data-interactive="true"
                  />
                  <div className="space-y-0.5">
                    <Label
                      htmlFor="enableObjectDetection"
                      className="text-sm font-medium cursor-pointer text-white"
                    >
                      Use object detection
                    </Label>
                    <p className="text-xs text-muted-foreground">
                      Enable object detection to track objects on scenes without faces
                      for improved camera motion (slower)
                    </p>
                  </div>
                </div>
              )}
          </div>
        </div>

        <div className="space-y-3 rounded-xl border border-white/10 bg-slate-900/60 p-4">
          <label
            className="flex items-start gap-3 text-sm text-white"
            htmlFor="include-original"
          >
            <Checkbox
              checked={selection.includeOriginal}
              onCheckedChange={(checked) =>
                updateSelection({ includeOriginal: Boolean(checked) })
              }
              id="include-original"
            />
            <div className="space-y-0.5">
              <div className="font-medium">Also export Original (no cropping)</div>
              <p className="text-xs text-muted-foreground">Optional extra output</p>
            </div>
          </label>

          <p className="text-xs text-muted-foreground">
            You can enable Split, Full, or both. Each layout uses one processing level.
            Optionally include the Original.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

export const STYLE_LEVELS = {
  split: SPLIT_LEVELS,
  full: FULL_LEVELS,
};
