import type { ComponentType } from "react";

export type QualityLevel = {
  value: string;
  label: string;
  helper?: string;
  icon?: ComponentType<{ className?: string }>;
};

export type StaticPosition = "left" | "center" | "right";
export type HorizontalPosition = "left" | "center" | "right";
export type VerticalPosition = "top" | "middle" | "bottom";

/** StreamerSplit configuration for user-controlled crop */
export type StreamerSplitConfig = {
  positionX: HorizontalPosition;
  positionY: VerticalPosition;
  zoom: number;
};

export const DEFAULT_STREAMER_SPLIT_CONFIG: StreamerSplitConfig = {
  positionX: "left",
  positionY: "top",
  zoom: 1.5,
};

export type LayoutQualitySelection = {
  splitEnabled: boolean;
  splitStyle: string;
  fullEnabled: boolean;
  fullStyle: string;
  /** Position for Static Full style (left_focus, center_focus, right_focus) */
  staticPosition: StaticPosition;
  includeOriginal: boolean;
  /** StreamerSplit configuration */
  streamerSplitConfig: StreamerSplitConfig;
  /** Enable Top Scenes compilation for Streamer style (max 5 scenes with countdown) */
  topScenesEnabled: boolean;
  /** Cut silent parts from clips using VAD (default: true for more dynamic content) */
  cutSilentParts: boolean;
};

export const DEFAULT_SELECTION: LayoutQualitySelection = {
  splitEnabled: false,
  splitStyle: "intelligent_split",
  fullEnabled: false,
  fullStyle: "intelligent",
  staticPosition: "center",
  includeOriginal: false,
  streamerSplitConfig: DEFAULT_STREAMER_SPLIT_CONFIG,
  topScenesEnabled: false,
  cutSilentParts: false,
};

/** Map static position to backend style name */
export const STATIC_POSITION_STYLES: Record<StaticPosition, string> = {
  left: "left_focus",
  center: "center_focus",
  right: "right_focus",
};

/** Styles that require a studio plan (Active Speaker) */
export const STUDIO_ONLY_STYLES: string[] = [];

/** Styles that require at least a pro plan (Smart Face, Active Speaker, Cinematic) */
export const PRO_ONLY_STYLES = [
  "intelligent",
  "intelligent_split",
  "intelligent_speaker",
  "intelligent_split_speaker",
  "intelligent_cinematic",
];
