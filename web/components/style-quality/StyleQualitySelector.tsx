"use client";

import * as Slider from "@radix-ui/react-slider";
import { Activity, ScanFace, Sparkles, Zap } from "lucide-react";
import { type ComponentType, type KeyboardEvent, type MouseEvent, useMemo } from "react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { normalizeStyleForSelection } from "@/lib/styleTiers";
import { cn } from "@/lib/utils";

type QualityLevel = {
  value: string;
  label: string;
  helper?: string;
  icon?: ComponentType<{ className?: string }>;
};

export type LayoutQualitySelection = {
  splitEnabled: boolean;
  splitStyle: string;
  fullEnabled: boolean;
  fullStyle: string;
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
    helper: "Premium face mesh AI, left=top/right=bottom",
    icon: Sparkles,
  },
];

const FULL_LEVELS: QualityLevel[] = [
  {
    value: "left_focus",
    label: "Static – Focus Left",
    helper: "No AI, fixed left crop",
    icon: Zap,
  },
  {
    value: "center_focus",
    label: "Static – Focus Center",
    helper: "No AI, fixed center crop",
    icon: Zap,
  },
  {
    value: "right_focus",
    label: "Static – Focus Right",
    helper: "No AI, fixed right crop",
    icon: Zap,
  },
  {
    value: "intelligent_motion",
    label: "Motion",
    helper: "Heuristic motion-aware crop (no neural nets)",
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
];

const splitValues = SPLIT_LEVELS.map((lvl) => lvl.value);
const fullValues = FULL_LEVELS.map((lvl) => lvl.value);

export const DEFAULT_SELECTION: LayoutQualitySelection = {
  splitEnabled: true,
  splitStyle: "intelligent_split",
  fullEnabled: false,
  fullStyle: "intelligent",
  includeOriginal: false,
};

const STYLE_SELECTION_ALIASES: Record<string, string> = {
  intelligent_split_activity: "intelligent_split_speaker",
  intelligent_activity: "intelligent_speaker",
  intelligent_split_basic: "intelligent_split",
  intelligent_basic: "intelligent",
};

export function selectionToStyles(selection: LayoutQualitySelection): string[] {
  const styles = new Set<string>();

  if (selection.splitEnabled) {
    styles.add(selection.splitStyle);
  }

  if (selection.fullEnabled) {
    styles.add(selection.fullStyle);
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
    return STYLE_SELECTION_ALIASES[lowered] ?? normalizeStyleForSelection(lowered) ?? lowered;
  });

  const splitStyle =
    splitValues.find((val) => normalized.includes(val)) ?? fallback.splitStyle;
  const fullStyle =
    fullValues.find((val) => normalized.includes(val)) ?? fallback.fullStyle;
  const splitEnabled = normalized.some((s) => splitValues.includes(s));
  const fullEnabled = normalized.some((s) => fullValues.includes(s));

  return {
    splitEnabled,
    fullEnabled,
    includeOriginal: normalized.includes("original"),
    splitStyle,
    fullStyle,
  };
}

function QualitySlider({
  levels,
  value,
  onChange,
  disabled,
}: {
  levels: QualityLevel[];
  value: string;
  disabled?: boolean;
  onChange: (next: string) => void;
}) {
  const activeIndex = Math.max(
    levels.findIndex((level) => level.value === value),
    0
  );
  const columnsClass =
    levels.length >= 6 ? "grid-cols-6" : levels.length >= 5 ? "grid-cols-5" : "grid-cols-4";

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
          if (target) onChange(target.value);
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
          return (
            <button
              key={level.value}
              type="button"
              onClick={() => onChange(level.value)}
              disabled={disabled}
              className={cn(
                "space-y-0.5 rounded-lg border border-transparent px-2 py-1 transition-colors",
                !disabled && "hover:border-white/10 hover:bg-white/5",
                isActive && "border-fuchsia-400/40 bg-fuchsia-500/10 text-white"
              )}
              data-interactive="true"
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
            </button>
          );
        })}
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
}: {
  title: string;
  enabled: boolean;
  onToggle: (next: boolean) => void;
  levelValue: string;
  levels: QualityLevel[];
  onLevelChange: (next: string) => void;
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
        />
      </div>
    </div>
  );
}

interface StyleQualitySelectorProps {
  selectedStyles: string[];
  onChange: (styles: string[]) => void;
  disabled?: boolean;
  className?: string;
}

export function StyleQualitySelector({
  selectedStyles,
  onChange,
  disabled = false,
  className,
}: StyleQualitySelectorProps) {
  const selection = useMemo(
    () => stylesToSelection(selectedStyles, DEFAULT_SELECTION),
    [selectedStyles]
  );

  const updateSelection = (patch: Partial<LayoutQualitySelection>) => {
    const next = { ...selection, ...patch };
    onChange(selectionToStyles(next));
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
          />

          <LayoutCard
            title="Full View (9×16)"
            enabled={selection.fullEnabled}
            onToggle={(next) => updateSelection({ fullEnabled: next })}
            levelValue={selection.fullStyle}
            levels={FULL_LEVELS}
            onLevelChange={(next) =>
              updateSelection({ fullStyle: next, fullEnabled: true })
            }
          />
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
