export type TierColor = "static" | "motion" | "basic" | "premium" | "legacy";

type StyleMeta = {
  label: string;
  color: TierColor;
};

export const STYLE_TIER_LABELS: Record<string, StyleMeta> = {
  // Static / Tier 0
  split: { label: "Static", color: "static" },
  split_fast: { label: "Static", color: "static" },
  left_focus: { label: "Static", color: "static" },
  center_focus: { label: "Static", color: "static" },
  right_focus: { label: "Static", color: "static" },
  original: { label: "Static", color: "static" },

  // Motion / Tier 1
  intelligent_motion: { label: "Motion", color: "motion" },
  intelligent_split_motion: { label: "Motion (Split)", color: "motion" },

  // Smart Face / Tier 2
  intelligent: { label: "Smart Face", color: "basic" },
  intelligent_split: { label: "Smart Face (Split)", color: "basic" },

  // Active Face / Tier 3
  intelligent_speaker: { label: "Active Speaker", color: "premium" },
  intelligent_split_speaker: { label: "Active Speaker (Split)", color: "premium" },

  // Legacy fallbacks (history only)
  intelligent_activity: { label: "Active Speaker (Legacy)", color: "legacy" },
  intelligent_split_activity: {
    label: "Active Speaker (Legacy Split)",
    color: "legacy",
  },
  intelligent_basic: { label: "Smart Face (Legacy)", color: "legacy" },
  intelligent_split_basic: { label: "Smart Face (Legacy Split)", color: "legacy" },
};

export const STYLE_DISPLAY_LABELS: Record<string, string> = {
  split: "Static – Split",
  split_fast: "Static – Fast",
  left_focus: "Static – Focus Left",
  center_focus: "Static – Focus Center",
  right_focus: "Static – Focus Right",
  original: "Original",
  intelligent_motion: "Motion",
  intelligent_split_motion: "Motion (Split)",
  intelligent: "Smart Face",
  intelligent_split: "Smart Face (Split)",
  intelligent_speaker: "Active Speaker",
  intelligent_split_speaker: "Active Speaker (Split)",
  intelligent_activity: "Active Speaker (Legacy)",
  intelligent_split_activity: "Active Speaker (Legacy Split)",
  intelligent_basic: "Smart Face (Legacy)",
  intelligent_split_basic: "Smart Face (Legacy Split)",
};

const STYLE_ALIASES: Record<string, string> = {
  intelligent_activity: "intelligent_speaker",
  intelligent_split_activity: "intelligent_split_speaker",
  intelligent_basic: "intelligent",
  intelligent_split_basic: "intelligent_split",
};

export const TIER_BADGE_CLASSES: Record<TierColor, string> = {
  static:
    "bg-slate-100 text-slate-800 border-slate-200 dark:bg-slate-900/70 dark:text-slate-100 dark:border-slate-700",
  motion:
    "bg-sky-100 text-sky-800 border-sky-200 dark:bg-sky-900/40 dark:text-sky-100 dark:border-sky-700",
  basic:
    "bg-emerald-100 text-emerald-800 border-emerald-200 dark:bg-emerald-900/40 dark:text-emerald-100 dark:border-emerald-700",
  premium:
    "bg-amber-100 text-amber-800 border-amber-200 dark:bg-amber-900/40 dark:text-amber-100 dark:border-amber-700",
  legacy:
    "bg-slate-200 text-slate-800 border-slate-300 dark:bg-slate-800 dark:text-slate-100 dark:border-slate-600",
};

export function normalizeStyleForSelection(style?: string): string | undefined {
  if (!style) return undefined;
  const normalized = style.toLowerCase();
  // Lookup in static constant object with normalized key - safe pattern
  // eslint-disable-next-line security/detect-object-injection
  return STYLE_ALIASES[normalized] ?? normalized;
}

export function getStyleTier(style?: string): StyleMeta | undefined {
  if (!style) return undefined;
  const normalized = style.toLowerCase();
  // Lookup in static constant objects with normalized keys - safe pattern
  // eslint-disable-next-line security/detect-object-injection
  const alias = STYLE_ALIASES[normalized] ?? "";
  // eslint-disable-next-line security/detect-object-injection
  return STYLE_TIER_LABELS[normalized] ?? STYLE_TIER_LABELS[alias];
}

export function getStyleLabel(style?: string): string | undefined {
  if (!style) return undefined;
  const normalized = style.toLowerCase();
  // Lookup in static constant objects with normalized keys - safe pattern
  // eslint-disable-next-line security/detect-object-injection
  const alias = STYLE_ALIASES[normalized] ?? "";
  // eslint-disable-next-line security/detect-object-injection
  return STYLE_DISPLAY_LABELS[normalized] ?? STYLE_DISPLAY_LABELS[alias] ?? normalized;
}

export function getTierBadgeClasses(color: TierColor = "legacy"): string {
  // eslint-disable-next-line security/detect-object-injection
  return TIER_BADGE_CLASSES[color] ?? TIER_BADGE_CLASSES.legacy;
}
