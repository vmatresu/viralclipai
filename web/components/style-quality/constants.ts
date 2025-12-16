import {
  Activity,
  Film,
  Gamepad2,
  Monitor,
  ScanFace,
  Sparkles,
  Zap,
} from "lucide-react";

import type { QualityLevel } from "./types";

export const SPLIT_LEVELS: QualityLevel[] = [
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
    value: "streamer_split",
    label: "Streamer Split",
    helper: "Original on top, custom crop on bottom",
    icon: Monitor,
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
    helper: "Premium face mesh AI for active speaker",
    icon: Sparkles,
  },
];

export const FULL_LEVELS: QualityLevel[] = [
  {
    value: "center_focus",
    label: "Static",
    helper: "No AI, fixed crop position",
    icon: Zap,
  },
  {
    value: "streamer",
    label: "Streamer",
    helper: "Landscape centered with blurred background",
    icon: Gamepad2,
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

export const splitValues = SPLIT_LEVELS.map((lvl) => lvl.value);
export const fullValues = FULL_LEVELS.map((lvl) => lvl.value);

/** Style aliases for backward compatibility */
export const STYLE_SELECTION_ALIASES: Record<string, string> = {
  intelligent_split_activity: "intelligent_split_speaker",
  intelligent_activity: "intelligent_speaker",
  intelligent_split_basic: "intelligent_split",
  intelligent_basic: "intelligent",
};
