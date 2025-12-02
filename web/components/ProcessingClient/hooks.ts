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

import { type Clip } from "../ClipGrid";

export function useVideoProcessing() {
  const searchParams = useSearchParams();
  const { getIdToken, loading: authLoading, user } = useAuth();

  const [url, setUrl] = useState("");
  const [style, setStyle] = useState("split");
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [videoId, setVideoId] = useState<string | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [customPrompt, setCustomPrompt] = useState("");
  const [customPromptUsed, setCustomPromptUsed] = useState<string | null>(null);
  const processingStartTime = useRef<number | null>(null);
  // Store processing parameters at start time for accurate analytics tracking
  const processingStyle = useRef<string>("split");
  const processingCustomPrompt = useRef<string>("");

  const log = useCallback(
    (msg: string, type: "info" | "error" | "success" = "info") => {
      let prefix = ">";
      if (type === "error") {
        prefix = "[ERROR]";
      } else if (type === "success") {
        prefix = "[OK]";
      }
      setLogs((prev) => [...prev, `${prefix} ${msg}`]);
    },
    []
  );

  const loadResults = useCallback(
    async (id: string) => {
      try {
        setSubmitting(false);
        const token = await getIdToken();
        if (!token) {
          throw new Error("You must be signed in to view your clips.");
        }
        const data = await apiFetch<{ clips: Clip[]; custom_prompt?: string }>(
          `/api/videos/${id}`,
          {
            token,
          }
        );
        const clipsData = data.clips || [];
        setClips(clipsData);
        setCustomPromptUsed(data.custom_prompt ?? null);

        // Track processing completion with actual clips count
        // Use stored values from when processing started, not current form values
        if (processingStartTime.current) {
          const durationMs = Date.now() - processingStartTime.current;
          void analyticsEvents.videoProcessingCompleted({
            videoId: id,
            style: processingStyle.current,
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
    if (existingId && !authLoading && user) {
      setVideoId(existingId);
      void loadResults(existingId);
    }
  }, [searchParams, loadResults, setVideoId, authLoading, user]);

  return {
    // State
    url,
    setUrl,
    style,
    setStyle,
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
    processingStartTime,
    processingStyle,
    processingCustomPrompt,
    // Actions
    log,
    loadResults,
    setSubmitting,
    setProgress,
    setError,
    setErrorDetails,
    setVideoId,
    setClips,
    searchParams,
  };
}
