/**
 * Custom Hook: useVideoPolling
 *
 * Polls for video status updates for videos that are currently processing.
 * Uses exponential backoff to reduce server load.
 */

import { useCallback, useEffect, useMemo, useRef } from "react";

import { mergeProcessingStatuses } from "@/hooks/videoPollingMerge";
import { getProcessingStatuses } from "@/lib/apiClient";
import { invalidateClipsCache } from "@/lib/cache";
import { frontendLogger } from "@/lib/logger";

export interface UserVideo {
  id?: string;
  video_id?: string;
  status?: "processing" | "analyzed" | "completed" | "failed";
  clips_count?: number;
  updated_at?: string;
}

interface UseVideoPollingOptions {
  videos: UserVideo[];
  enabled: boolean;
  getIdToken: () => Promise<string | null>;
  onVideosUpdate: (videos: UserVideo[]) => void;
  pollInterval?: number; // Default: 5000ms
  maxInterval?: number; // Maximum interval for exponential backoff
  longProcessingAfterMs?: number; // Default: 10 min
}

function getVideoId(video: UserVideo): string {
  return video.video_id ?? video.id ?? "";
}

/**
 * Poll for video status updates
 */
export function useVideoPolling({
  videos,
  enabled,
  getIdToken,
  onVideosUpdate,
  pollInterval = 5000,
  maxInterval = 30000,
  longProcessingAfterMs = 10 * 60 * 1000,
}: UseVideoPollingOptions) {
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const backoffRef = useRef<number>(pollInterval);
  const isPollingRef = useRef<boolean>(false);
  const firstSeenProcessingAtRef = useRef<Map<string, number>>(new Map());

  const processingIds = useMemo(() => {
    return videos
      .filter((v) => v.status === "processing")
      .map(getVideoId)
      .filter(Boolean);
  }, [videos]);

  const processingIdsKey = useMemo(() => processingIds.join(","), [processingIds]);

  const poll = useCallback(async () => {
    // Prevent concurrent polls
    if (isPollingRef.current) {
      return;
    }

    isPollingRef.current = true;

    try {
      const token = await getIdToken();
      if (!token) {
        return;
      }

      const now = Date.now();
      processingIds.forEach((id) => {
        if (!firstSeenProcessingAtRef.current.has(id)) {
          firstSeenProcessingAtRef.current.set(id, now);
        }
      });

      const data = await getProcessingStatuses(token, processingIds);
      const { merged, completedVideoIds, hadAnyChange } = mergeProcessingStatuses(
        videos,
        data.videos
      );

      if (hadAnyChange) {
        onVideosUpdate(merged);
        // Invalidate cache for videos that completed
        completedVideoIds.forEach((videoId: string) => {
          void invalidateClipsCache(videoId);
        });
        // Reset backoff on successful update
        backoffRef.current = pollInterval;
      } else {
        // Exponential backoff if no changes (reduce server load)
        backoffRef.current = Math.min(backoffRef.current * 1.5, maxInterval);
      }

      const processingDurationsMs = processingIds
        .map((id) => {
          const start = firstSeenProcessingAtRef.current.get(id);
          return typeof start === "number" ? now - start : 0;
        })
        .filter((v) => v > 0);
      const maxProcessingMs = Math.max(0, ...processingDurationsMs);
      if (maxProcessingMs > longProcessingAfterMs) {
        backoffRef.current = Math.min(Math.max(backoffRef.current, maxInterval), 60000);
      }
    } catch (err) {
      // Silently fail - don't disrupt UI
      frontendLogger.error("Failed to poll for video updates", { error: err });
      // Increase backoff on error
      backoffRef.current = Math.min(backoffRef.current * 2, maxInterval);
    } finally {
      isPollingRef.current = false;
    }
  }, [
    videos,
    getIdToken,
    onVideosUpdate,
    pollInterval,
    maxInterval,
    longProcessingAfterMs,
    processingIds,
  ]);

  useEffect(() => {
    const cleanup = () => {
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
        intervalRef.current = null;
      }
    };

    if (!enabled) {
      cleanup();
      return cleanup;
    }

    if (!processingIdsKey) {
      // Reset backoff when no processing videos
      backoffRef.current = pollInterval;
      firstSeenProcessingAtRef.current.clear();
      cleanup();
      return cleanup;
    }

    // Set up polling interval with exponential backoff
    const scheduleNextPoll = () => {
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
      }

      intervalRef.current = setTimeout(() => {
        void poll();
        scheduleNextPoll();
      }, backoffRef.current);
    };

    // Start polling immediately, then schedule next
    void poll();
    scheduleNextPoll();

    return cleanup;
  }, [enabled, processingIdsKey, poll, pollInterval]);

  return {
    currentInterval: backoffRef.current,
  };
}
