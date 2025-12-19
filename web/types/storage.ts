/**
 * Storage usage information for a user account.
 */
export interface StorageInfo {
  /** Total storage used in bytes. */
  used_bytes: number;
  /** Storage limit in bytes. */
  limit_bytes: number;
  /** Total number of clips. */
  total_clips: number;
  /** Usage percentage (0-100). */
  percentage: number;
  /** Human-readable used storage. */
  used_formatted: string;
  /** Human-readable storage limit. */
  limit_formatted: string;
  /** Human-readable remaining storage. */
  remaining_formatted: string;
}

/**
 * Plan storage limits in bytes.
 * These must match backend values in vclip-models/src/plan.rs
 */
export const PLAN_STORAGE_LIMITS = {
  free: 1 * 1024 * 1024 * 1024, // 1 GB
  pro: 30 * 1024 * 1024 * 1024, // 30 GB
  studio: 150 * 1024 * 1024 * 1024, // 150 GB
} as const;

/**
 * Format bytes as human-readable string.
 */
export function formatBytes(bytes: number): string {
  const KB = 1024;
  const MB = KB * 1024;
  const GB = MB * 1024;

  if (bytes >= GB) {
    return `${(bytes / GB).toFixed(2)} GB`;
  } else if (bytes >= MB) {
    return `${(bytes / MB).toFixed(2)} MB`;
  } else if (bytes >= KB) {
    return `${(bytes / KB).toFixed(2)} KB`;
  } else {
    return `${bytes} B`;
  }
}

/**
 * Calculate storage usage percentage.
 */
export function calculateStoragePercentage(used: number, limit: number): number {
  if (limit === 0) return 0;
  return Math.min((used / limit) * 100, 100);
}

/**
 * Check if storage would exceed limit.
 */
export function wouldExceedStorage(
  currentUsed: number,
  additionalBytes: number,
  limit: number
): boolean {
  return currentUsed + additionalBytes > limit;
}

/**
 * Parse a human-readable size string (e.g., "1.5 MB") to bytes.
 * Returns 0 for invalid or missing input.
 */
export function parseSizeToBytes(sizeStr?: string): number {
  if (!sizeStr) return 0;
  const match = sizeStr.match(/^([\d.]+)\s*(B|KB|MB|GB)$/i);
  if (!match || match.length < 3) return 0;
  const value = parseFloat(match[1] ?? "0");
  const unit = (match[2] ?? "B").toUpperCase();
  switch (unit) {
    case "GB":
      return value * 1024 * 1024 * 1024;
    case "MB":
      return value * 1024 * 1024;
    case "KB":
      return value * 1024;
    default:
      return value;
  }
}
