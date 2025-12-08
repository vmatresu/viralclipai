/**
 * ProcessingClient Custom Hooks
 *
 * Custom hooks for video processing logic.
 */

import { useSearchParams } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";

import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { getCachedClips, invalidateClipsCache, setCachedClips } from "@/lib/cache";
import { type ClipProcessingStep } from "@/lib/websocket/types";
import { type SceneProgress } from "@/types/processing";

import { type Clip } from "../ClipGrid";

export function useVideoProcessing() {
  const searchParams = useSearchParams();
  const { getIdToken, loading: authLoading, user } = useAuth();

  const [url, setUrl] = useState("");
  const [styles, setStyles] = useState<string[]>(["split"]);
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [videoId, setVideoId] = useState<string | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [customPrompt, setCustomPrompt] = useState("");
  const [customPromptUsed, setCustomPromptUsed] = useState<string | null>(null);
  const [videoTitle, setVideoTitle] = useState<string | null>(null);
  const [videoUrl, setVideoUrl] = useState<string | null>(null);
  // Detailed scene progress tracking
  const [sceneProgress, setSceneProgress] = useState<Map<number, SceneProgress>>(
    new Map()
  );
  const processingStartTime = useRef<number | null>(null);
  // Store processing parameters at start time for accurate analytics tracking
  const processingStyles = useRef<string[]>(["split"]);
  const processingCustomPrompt = useRef<string>("");

  // Scene progress handlers
  const handleSceneStarted = useCallback(
    (
      sceneId: number,
      sceneTitle: string,
      styleCount: number,
      startSec: number,
      durationSec: number
    ) => {
      setSceneProgress((prev) => {
        const next = new Map(prev);
        next.set(sceneId, {
          sceneId,
          sceneTitle,
          styleCount,
          startSec,
          durationSec,
          status: "processing",
          clipsCompleted: 0,
          clipsFailed: 0,
          currentSteps: new Map(),
        });
        return next;
      });
    },
    []
  );

  const handleSceneCompleted = useCallback(
    (sceneId: number, clipsCompleted: number, clipsFailed: number) => {
      setSceneProgress((prev) => {
        const next = new Map(prev);
        const scene = next.get(sceneId);
        if (scene) {
          next.set(sceneId, {
            ...scene,
            status: clipsFailed > 0 ? "failed" : "completed",
            clipsCompleted,
            clipsFailed,
          });
        }
        return next;
      });
    },
    []
  );

  const handleClipProgress = useCallback(
    (sceneId: number, style: string, step: ClipProcessingStep, details?: string) => {
      setSceneProgress((prev) => {
        const next = new Map(prev);
        const scene = next.get(sceneId);
        if (scene) {
          const newSteps = new Map(scene.currentSteps);
          newSteps.set(style, { step, details });
          next.set(sceneId, { ...scene, currentSteps: newSteps });
        }
        return next;
      });
    },
    []
  );

  const resetSceneProgress = useCallback(() => {
    setSceneProgress(new Map());
  }, []);

  const log = useCallback(
    (msg: string, type: "info" | "error" | "success" = "info", timestamp?: string) => {
      let prefix = ">";
      if (type === "error") {
        prefix = "[ERROR]";
      } else if (type === "success") {
        prefix = "[OK]";
      }

      // Format timestamp if provided (ISO 8601 format from server)
      let timestampStr = "";
      if (timestamp) {
        try {
          const date = new Date(timestamp);
          // Format as HH:MM:SS (local time)
          timestampStr = date.toLocaleTimeString("en-US", {
            hour12: false,
            hour: "2-digit",
            minute: "2-digit",
            second: "2-digit",
          });
          timestampStr = `[${timestampStr}] `;
        } catch {
          // If timestamp parsing fails, ignore it
        }
      }

      setLogs((prev) => [...prev, `${timestampStr}${prefix} ${msg}`]);
    },
    []
  );

  const loadResults = useCallback(
    async (id: string, forceRefresh: boolean = false) => {
      try {
        setSubmitting(false);

        // SECURITY: Always check authentication first before accessing any data
        const token = await getIdToken();
        if (!token) {
          // Clear any cached data if user is not authenticated
          setClips([]);
          setCustomPromptUsed(null);
          setVideoTitle(null);
          setVideoUrl(null);
          throw new Error("You must be signed in to view your clips.");
        }

        // Skip cache when force refreshing (e.g., after clip upload)
        if (forceRefresh) {
          await invalidateClipsCache(id);
        }

        // Only check cache after authentication is verified (unless force refresh)
        if (!forceRefresh) {
          const cachedData = await getCachedClips(id);
          if (cachedData) {
            // Use cached data (user is authenticated at this point)
            setClips(cachedData.clips);
            setCustomPromptUsed(cachedData.custom_prompt ?? null);
            setVideoTitle(cachedData.video_title ?? null);
            setVideoUrl(cachedData.video_url ?? null);

            // Track processing completion with actual clips count
            // Use stored values from when processing started, not current form values
            if (processingStartTime.current) {
              const durationMs = Date.now() - processingStartTime.current;
              void analyticsEvents.videoProcessingCompleted({
                videoId: id,
                style: processingStyles.current.join(","),
                clipsGenerated: cachedData.clips.length,
                durationMs,
                hasCustomPrompt: processingCustomPrompt.current.trim().length > 0,
              });
              processingStartTime.current = null;
            }
            return;
          }
        }

        // Cache miss or force refresh - fetch from API (token already verified above)
        const data = await apiFetch<{
          clips: Clip[];
          custom_prompt?: string;
          video_title?: string;
          video_url?: string;
        }>(`/api/videos/${id}`, {
          token,
        });
        const clipsData = data.clips || [];

        // Cache the data for future use (fire and forget)
        void setCachedClips(id, {
          clips: clipsData,
          custom_prompt: data.custom_prompt ?? null,
          video_title: data.video_title ?? null,
          video_url: data.video_url ?? null,
        });

        setClips(clipsData);
        setCustomPromptUsed(data.custom_prompt ?? null);
        setVideoTitle(data.video_title ?? null);
        setVideoUrl(data.video_url ?? null);

        // Track processing completion with actual clips count
        // Use stored values from when processing started, not current form values
        if (processingStartTime.current) {
          const durationMs = Date.now() - processingStartTime.current;
          void analyticsEvents.videoProcessingCompleted({
            videoId: id,
            style: processingStyles.current.join(","),
            clipsGenerated: clipsData.length,
            durationMs,
            hasCustomPrompt: processingCustomPrompt.current.trim().length > 0,
          });
          processingStartTime.current = null;
        }
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : "Error loading results";
        setError(errorMessage);
      }
    },
    [getIdToken]
  );

  useEffect(() => {
    const existingId = searchParams.get("id");

    // SECURITY: Only load results if user is authenticated
    if (existingId && !authLoading && user) {
      setVideoId(existingId);
      // Ensure we're not in a submitting state when viewing results
      setSubmitting(false);
      // Don't reset scene progress when loading existing results - preserve it for display
      void loadResults(existingId);
    } else if (!user && !authLoading) {
      // User is signed out - clear any displayed data
      setVideoId(null);
      setClips([]);
      setCustomPromptUsed(null);
      setVideoTitle(null);
      setVideoUrl(null);
      setSubmitting(false);
      setProgress(0);
      setLogs([]);
      resetSceneProgress();
    }
  }, [searchParams, loadResults, authLoading, user, resetSceneProgress]);

  return {
    // State
    url,
    setUrl,
    styles,
    setStyles,
    logs,
    setLogs,
    progress,
    submitting,
    error,
    errorDetails,
    videoId,
    clips,
    customPrompt,
    setCustomPrompt,
    customPromptUsed,
    setCustomPromptUsed,
    videoTitle,
    videoUrl,
    processingStartTime,
    processingStyles,
    processingCustomPrompt,
    // Scene progress tracking
    sceneProgress,
    // Actions
    log,
    loadResults,
    setSubmitting,
    setProgress,
    setError,
    setErrorDetails,
    setVideoId,
    setClips,
    setVideoTitle,
    setVideoUrl,
    searchParams,
    // Scene progress handlers
    handleSceneStarted,
    handleSceneCompleted,
    handleClipProgress,
    resetSceneProgress,
  };
}
