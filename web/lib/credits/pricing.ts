/**
 * Credit pricing utilities and types.
 * Client-side helpers for calculating and displaying credit costs.
 * 
 * NOTE: These are for display only. The backend is the source of truth
 * for actual credit calculations and reservations.
 */

import type { CreditLineItem } from "@/components/credits/CreditCostBreakdown";

// ============================================================================
// Pricing Constants (fallback values matching backend)
// ============================================================================

/** Cost of analyzing a video */
export const ANALYSIS_CREDIT_COST = 3;

/** Credits per style (matching backend vclip_models::plan) */
export const STYLE_CREDIT_COSTS: Record<string, number> = {
  // Static styles (10 credits - DetectionTier::None)
  original: 10,
  split: 10,
  left_focus: 10,
  right_focus: 10,
  center_focus: 10,
  split_fast: 10,

  // Basic AI styles (10 credits - DetectionTier::Basic)
  intelligent: 10,
  intelligent_split: 10,

  // Smart AI styles (20 credits - MotionAware/SpeakerAware)
  intelligent_speaker: 20,
  intelligent_split_speaker: 20,
  intelligent_motion: 20,
  // IntelligentSplitMotion has special pricing (10 credits)
  intelligent_split_motion: 10,
  intelligent_split_activity: 20,

  // Cinematic styles (30 credits - DetectionTier::Cinematic)
  intelligent_cinematic: 30,
  cinematic: 30, // alias

  // Streamer styles (10 credits - special pricing, no AI)
  streamer: 10,
  streamer_split: 10,
  streamer_top_scenes: 10,
};

/** Add-on costs */
export const ADDON_CREDIT_COSTS = {
  silentRemoverPerScene: 5,
  objectDetectionAddon: 10,
  sceneOriginalsDownloadPerScene: 5,
};

// ============================================================================
// Pricing Types
// ============================================================================

export interface CreditPricing {
  analysis_credit_cost: number;
  style_credit_costs: Record<string, number>;
  addons: {
    silent_remover_per_scene: number;
    object_detection_addon: number;
    scene_originals_download_per_scene: number;
  };
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Get the credit cost for a given style.
 * Falls back to 10 credits if style is unknown.
 */
export function getStyleCreditCost(style: string): number {
  const normalizedStyle = style.toLowerCase().replace(/-/g, "_");
  return STYLE_CREDIT_COSTS[normalizedStyle] ?? 10;
}

/**
 * Calculate a cost breakdown for scene processing.
 */
export function calculateSceneCostBreakdown(
  fullCount: number,
  fullStyle: string,
  splitCount: number,
  splitStyle: string,
  options?: {
    cutSilentParts?: boolean;
    objectDetection?: boolean;
    downloadOriginals?: boolean;
    sceneCount?: number;
  }
): { lineItems: CreditLineItem[]; total: number } {
  const lineItems: CreditLineItem[] = [];
  let total = 0;

  // Full renders
  if (fullCount > 0) {
    const unitCost = getStyleCreditCost(fullStyle);
    const totalCost = fullCount * unitCost;
    lineItems.push({
      label: "Full renders",
      qty: fullCount,
      unitCost,
      totalCost,
    });
    total += totalCost;
  }

  // Split renders
  if (splitCount > 0) {
    const unitCost = getStyleCreditCost(splitStyle);
    const totalCost = splitCount * unitCost;
    lineItems.push({
      label: "Split renders",
      qty: splitCount,
      unitCost,
      totalCost,
    });
    total += totalCost;
  }

  // Add-ons
  const sceneCount = options?.sceneCount ?? Math.max(fullCount, splitCount);

  if (options?.cutSilentParts && sceneCount > 0) {
    const totalCost = sceneCount * ADDON_CREDIT_COSTS.silentRemoverPerScene;
    lineItems.push({
      label: "Cut silent parts",
      qty: sceneCount,
      unitCost: ADDON_CREDIT_COSTS.silentRemoverPerScene,
      totalCost,
    });
    total += totalCost;
  }

  if (options?.objectDetection) {
    const totalCost = ADDON_CREDIT_COSTS.objectDetectionAddon;
    lineItems.push({
      label: "Object detection",
      qty: 1,
      unitCost: totalCost,
      totalCost,
    });
    total += totalCost;
  }

  if (options?.downloadOriginals && sceneCount > 0) {
    const totalCost =
      sceneCount * ADDON_CREDIT_COSTS.sceneOriginalsDownloadPerScene;
    lineItems.push({
      label: "Download originals",
      qty: sceneCount,
      unitCost: ADDON_CREDIT_COSTS.sceneOriginalsDownloadPerScene,
      totalCost,
    });
    total += totalCost;
  }

  return { lineItems, total };
}

/**
 * Format a credit amount for display.
 */
export function formatCredits(amount: number): string {
  return `${amount} credit${amount !== 1 ? "s" : ""}`;
}
