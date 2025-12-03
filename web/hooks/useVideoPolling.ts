/**
 * Custom Hook: useVideoPolling
 * 
 * Polls for video status updates for videos that are currently processing.
 * Uses exponential backoff to reduce server load.
 */

import { useEffect, useRef, useCallback } from "react";
import { apiFetch } from "@/lib/apiClient";
import { invalidateClipsCache } from "@/lib/cache";
import { frontendLogger } from "@/lib/logger";

export interface UserVideo {
  id?: string;
  video_id?: string;
  status?: "processing" | "completed";
  clips_count?: number;
}

interface UseVideoPollingOptions {
  videos: UserVideo[];
  enabled: boolean;
  getIdToken: () => Promise<string | null>;
  onVideosUpdate: (videos: UserVideo[]) => void;
  pollInterval?: number; // Default: 5000ms
  maxInterval?: number; // Maximum interval for exponential backoff
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
}: UseVideoPollingOptions) {
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const backoffRef = useRef<number>(pollInterval);
  const isPollingRef = useRef<boolean>(false);

  const poll = useCallback(
    async () => {
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

        const data = await apiFetch<{ videos: UserVideo[] }>("/api/user/videos", {
          token,
        });
        const newVideos = (data as { videos: UserVideo[] }).videos ?? [];

        // Create a map of old videos by ID for comparison
        const oldVideosMap = new Map<string, UserVideo>();
        videos.forEach((v) => {
          const id = v.video_id ?? v.id ?? "";
          if (id) oldVideosMap.set(id, v);
        });

        // Check if any video status changed
        let hasStatusChange = false;
        const completedVideoIds: string[] = [];

        newVideos.forEach((newV) => {
          const id = newV.video_id ?? newV.id ?? "";
          if (!id) return;

          const oldV = oldVideosMap.get(id);
          if (oldV && oldV.status === "processing" && newV.status === "completed") {
            hasStatusChange = true;
            completedVideoIds.push(id);
          } else if (oldV && oldV.status !== newV.status) {
            hasStatusChange = true;
          }
        });

        if (hasStatusChange) {
          onVideosUpdate(newVideos);
          // Invalidate cache for videos that completed
          completedVideoIds.forEach((videoId) => {
            void invalidateClipsCache(videoId);
          });
          // Reset backoff on successful update
          backoffRef.current = pollInterval;
        } else {
          // Exponential backoff if no changes (reduce server load)
          backoffRef.current = Math.min(backoffRef.current * 1.5, maxInterval);
        }
      } catch (err) {
        // Silently fail - don't disrupt UI
        frontendLogger.error("Failed to poll for video updates", { error: err });
        // Increase backoff on error
        backoffRef.current = Math.min(backoffRef.current * 2, maxInterval);
      } finally {
        isPollingRef.current = false;
      }
    },
    [videos, getIdToken, onVideosUpdate, pollInterval, maxInterval]
  );

  useEffect(() => {
    if (!enabled) {
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
        intervalRef.current = null;
      }
      return;
    }

    const hasProcessingVideos = videos.some((v) => v.status === "processing");
    if (!hasProcessingVideos) {
      // Reset backoff when no processing videos
      backoffRef.current = pollInterval;
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
        intervalRef.current = null;
      }
      return;
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

    return () => {
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [enabled, videos, poll, pollInterval]);

  return {
    currentInterval: backoffRef.current,
  };
}
