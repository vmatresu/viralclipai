import { normalizeStyleForSelection } from "@/lib/styleTiers";

import { STYLE_SELECTION_ALIASES, fullValues, splitValues } from "./constants";
import {
  DEFAULT_SELECTION,
  STATIC_POSITION_STYLES,
  type LayoutQualitySelection,
  type StaticPosition,
} from "./types";

/**
 * Convert a LayoutQualitySelection to an array of style strings for the backend.
 */
export function selectionToStyles(selection: LayoutQualitySelection): string[] {
  const styles = new Set<string>();

  if (selection.splitEnabled) {
    styles.add(selection.splitStyle);
  }

  if (selection.fullEnabled) {
    // For Static style, use the position-specific style name
    if (selection.fullStyle === "center_focus") {
      styles.add(STATIC_POSITION_STYLES[selection.staticPosition]);
    } else if (selection.fullStyle === "streamer" && selection.topScenesEnabled) {
      // Use streamer_top_scenes when Top Scenes is enabled
      styles.add("streamer_top_scenes");
    } else {
      styles.add(selection.fullStyle);
    }
  }

  if (selection.includeOriginal) {
    styles.add("original");
  }

  return Array.from(styles);
}

/**
 * Parse an array of style strings into a LayoutQualitySelection.
 * Handles legacy style names and aliases.
 */
export function stylesToSelection(
  styles: string[],
  fallback: LayoutQualitySelection = DEFAULT_SELECTION
): LayoutQualitySelection {
  if (!styles.length) return fallback;

  // Normalize and apply aliases
  const normalized = styles.map((s) => {
    const lower = s.toLowerCase();
    return normalizeStyleForSelection(lower) ?? lower;
  });

  // Detect static position from focus styles
  let staticPosition: StaticPosition = fallback.staticPosition;
  if (normalized.includes("left_focus")) {
    staticPosition = "left";
  } else if (normalized.includes("right_focus")) {
    staticPosition = "right";
  } else if (normalized.includes("center_focus")) {
    staticPosition = "center";
  }

  // Map focus styles to center_focus for UI display
  const normalizedForUI = normalized.map((s) =>
    ["left_focus", "right_focus"].includes(s) ? "center_focus" : s
  );

  // Also map streamer_top_scenes to streamer for UI
  const normalizedForUIWithStreamer = normalizedForUI.map((s) =>
    s === "streamer_top_scenes" ? "streamer" : s
  );

  // Apply aliases for legacy styles
  const withAliases = normalizedForUIWithStreamer.map((s) => {
    // eslint-disable-next-line security/detect-object-injection
    return STYLE_SELECTION_ALIASES[s] ?? s;
  });

  const splitStyle =
    splitValues.find((val) => withAliases.includes(val)) ?? fallback.splitStyle;
  const fullStyle =
    fullValues.find((val) => withAliases.includes(val)) ?? fallback.fullStyle;
  const splitEnabled = withAliases.some((s) => splitValues.includes(s));
  const fullEnabled =
    withAliases.some((s) => fullValues.includes(s)) ||
    normalized.some((s) => ["left_focus", "right_focus", "center_focus"].includes(s)) ||
    normalized.includes("streamer_top_scenes");

  // Check if top scenes is enabled (streamer_top_scenes style)
  const topScenesEnabled = normalized.includes("streamer_top_scenes");

  return {
    splitEnabled,
    fullEnabled,
    includeOriginal: normalized.includes("original"),
    splitStyle,
    fullStyle,
    staticPosition,
    streamerSplitConfig: fallback.streamerSplitConfig,
    topScenesEnabled,
    cutSilentParts: fallback.cutSilentParts,
  };
}

/**
 * Check if a style requires a specific plan tier.
 */
export function getRequiredPlan(
  style: string,
  proOnlyStyles: string[],
  studioOnlyStyles: string[]
): "free" | "pro" | "studio" {
  const normalized = style.toLowerCase();
  if (studioOnlyStyles.includes(normalized)) return "studio";
  if (proOnlyStyles.includes(normalized)) return "pro";
  return "free";
}

/**
 * Check if user has access to a style based on their plan.
 */
export function hasAccessToStyle(
  style: string,
  hasProPlan: boolean,
  hasStudioPlan: boolean,
  proOnlyStyles: string[],
  studioOnlyStyles: string[]
): boolean {
  const required = getRequiredPlan(style, proOnlyStyles, studioOnlyStyles);
  if (required === "studio") return hasStudioPlan;
  if (required === "pro") return hasProPlan || hasStudioPlan;
  return true;
}
