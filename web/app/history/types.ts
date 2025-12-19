// Re-export sort types from hooks for backwards compatibility
export type { SortField, SortDirection } from "./hooks";

export interface UserVideo {
  id?: string;
  video_id?: string;
  video_title?: string;
  video_url?: string;
  created_at?: string;
  custom_prompt?: string;
  status?: "processing" | "analyzed" | "completed" | "failed";
  clips_count?: number;
  /** Total size of all clips in bytes. */
  total_size_bytes?: number;
  /** Human-readable total size. */
  total_size_formatted?: string;
}

export interface StorageInfo {
  used_bytes: number;
  limit_bytes: number;
  total_clips: number;
  percentage: number;
  used_formatted: string;
  limit_formatted: string;
  remaining_formatted: string;
}

export interface PlanUsage {
  plan: string;
  /** Monthly credits included in plan. */
  monthly_credits_limit: number;
  /** Credits used this billing month. */
  credits_used_this_month: number;
  storage?: StorageInfo;
}

export interface DeleteTarget {
  type: "single" | "bulk" | "all";
  videoId?: string;
}

/**
 * Parse size string (e.g., "1.5 MB") to bytes for sorting
 */
export function parseSizeToBytes(sizeStr?: string): number {
  if (!sizeStr) return 0;
  const match = sizeStr.match(/^([\d.]+)\s*(B|KB|MB|GB|TB)?$/i);
  if (!match) return 0;
  const value = parseFloat(match[1] ?? "0");
  const unit = (match[2] ?? "B").toUpperCase();
  const multipliers: Record<string, number> = {
    B: 1,
    KB: 1024,
    MB: 1024 * 1024,
    GB: 1024 * 1024 * 1024,
    TB: 1024 * 1024 * 1024 * 1024,
  };
  // Unit is extracted from regex match - safe to use as lookup key
  // eslint-disable-next-line security/detect-object-injection
  return value * (multipliers[unit] ?? 1);
}
