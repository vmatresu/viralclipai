/**
 * Custom hook for scene reprocessing via WebSocket.
 *
 * Provides a clean interface for reprocessing scenes with real-time progress updates.
 * Integrates with the global ProcessingContext for persistent state across page navigations.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { useAuth } from "@/lib/auth";
import { invalidateClipsCache } from "@/lib/cache";
import { useProcessing, type ProcessingJob } from "@/lib/processing-context";
import {
  reprocessScenesWebSocket,
  type ReprocessCallbacks,
} from "@/lib/websocket/reprocess-client";
import { type SceneProgress } from "@/types/processing";

interface ReprocessingState {
  isProcessing: boolean;
  progress: number;
  logs: string[];
  error: string | null;
  sceneProgress: Map<number, SceneProgress>;
}

interface UseReprocessingOptions {
  videoId: string;
  videoTitle?: string;
  onComplete?: () => void;
  onError?: (error: string) => void;
}

export function useReprocessing({
  videoId,
  videoTitle,
  onComplete,
  onError,
}: UseReprocessingOptions) {
  const { getIdToken } = useAuth();
  const { startJob, updateJob, completeJob, failJob, getJob } = useProcessing();
  const [state, setState] = useState<ReprocessingState>({
    isProcessing: false,
    progress: 0,
    logs: [],
    error: null,
    sceneProgress: new Map(),
  });
  const wsRef = useRef<WebSocket | null>(null);
  // Track if done was received to avoid stale closure issue in onClose
  const doneReceivedRef = useRef<boolean>(false);

  // Avoid React “setState during render of another component” by deferring cross-context updates
  const deferUpdateJob = useCallback(
    (updates: Partial<ProcessingJob>) => {
      // Defer to next macrotask to escape render phase
      setTimeout(() => updateJob(videoId, updates), 0);
    },
    [updateJob, videoId]
  );

  const addLog = useCallback(
    (message: string, timestamp?: string) => {
      setState((prev) => {
        // Prepend timestamp if provided
        const formattedMessage = timestamp ? `[${timestamp}] ${message}` : message;
        const nextLogs = [...prev.logs, formattedMessage];
        // Persist logs + current step to global processing context for refresh resilience
        deferUpdateJob({ logs: nextLogs, currentStep: formattedMessage });
        return {
          ...prev,
          logs: nextLogs,
        };
      });
    },
    [deferUpdateJob]
  );

  const cleanup = useCallback(() => {
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
  }, []);

  const reprocess = useCallback(
    async (sceneIds: number[], styles: string[]) => {
      if (state.isProcessing) {
        toast.error("Reprocessing already in progress");
        return;
      }

      if (sceneIds.length === 0 || styles.length === 0) {
        toast.error("Please select at least one scene and one style");
        return;
      }

      try {
        const token = await getIdToken();
        if (!token) {
          throw new Error("Failed to get authentication token");
        }

        // Calculate total clips
        const totalClips = sceneIds.length * styles.length;

        // Reset state and done tracking
        doneReceivedRef.current = false;
        setState({
          isProcessing: true,
          progress: 0,
          logs: [],
          error: null,
          sceneProgress: new Map(),
        });

        // Start job in global processing context
        // Set waitForProcessing=true to handle race condition where API might still show old 'completed' status
        startJob(videoId, videoTitle, totalClips, true);

        // Use dedicated WebSocket client for better separation of concerns
        const callbacks: ReprocessCallbacks = {
          onProgress: (value) => {
            setState((prev) => ({
              ...prev,
              progress: value,
            }));
            // Update global processing context
            deferUpdateJob({ progress: value, status: "processing" });
          },
          onLog: (message, timestamp) => {
            addLog(message, timestamp);
            // Update current step in global context
            const formattedMessage = timestamp ? `[${timestamp}] ${message}` : message;
            deferUpdateJob({ currentStep: formattedMessage });
          },
          onDone: () => {
            // Mark done received before any state updates to avoid stale closure issue in onClose
            doneReceivedRef.current = true;
            setState((prev) => ({
              ...prev,
              isProcessing: false,
              progress: 100,
            }));
            addLog("Reprocessing complete!");
            toast.success("Reprocessing complete!");
            // Invalidate cache to ensure fresh data is loaded (defense in depth)
            void invalidateClipsCache(videoId);
            // Complete job in global context
            completeJob(videoId);
            cleanup();
            onComplete?.();
          },
          onError: (message, details) => {
            // Mark as handled to prevent onClose from double-reporting
            doneReceivedRef.current = true;
            const errorMsg = message || "An error occurred during reprocessing";
            setState((prev) => ({
              ...prev,
              isProcessing: false,
              error: errorMsg,
            }));
            toast.error(errorMsg);
            if (details) {
              console.error("Reprocessing error details:", details);
            }
            // Fail job in global context
            failJob(videoId, errorMsg);
            cleanup();
            onError?.(errorMsg);
          },
          onClose: () => {
            // Connection closed - check if we completed successfully using ref (avoids stale closure)
            // If done was received, this is expected close after success
            if (!doneReceivedRef.current) {
              setState((prev) => ({
                ...prev,
                isProcessing: false,
                error: "Connection closed unexpectedly",
              }));
              toast.error("Connection closed unexpectedly");
              failJob(videoId, "Connection closed unexpectedly");
              onError?.("Connection closed unexpectedly");
            }
            cleanup();
          },
          // Scene progress handlers
          onSceneStarted: (sceneId, sceneTitle, styleCount, startSec, durationSec) => {
            setState((prev) => {
              const next = new Map(prev.sceneProgress);
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
              return { ...prev, sceneProgress: next };
            });
          },
          onSceneCompleted: (sceneId, clipsCompleted, clipsFailed) => {
            setState((prev) => {
              const next = new Map(prev.sceneProgress);
              const scene = next.get(sceneId);
              if (scene) {
                next.set(sceneId, {
                  ...scene,
                  status: clipsFailed > 0 ? "failed" : "completed",
                  clipsCompleted,
                  clipsFailed,
                });
              }
              return { ...prev, sceneProgress: next };
            });
          },
          onClipProgress: (sceneId, style, step, details) => {
            setState((prev) => {
              const next = new Map(prev.sceneProgress);
              const scene = next.get(sceneId);
              if (scene) {
                const newSteps = new Map(scene.currentSteps);
                newSteps.set(style, { step, details });
                next.set(sceneId, { ...scene, currentSteps: newSteps });
              }
              return { ...prev, sceneProgress: next };
            });
          },
        };

        // Create WebSocket connection using dedicated client
        wsRef.current = reprocessScenesWebSocket(
          {
            videoId,
            sceneIds,
            styles,
            token,
            cropMode: "none",
            targetAspect: "9:16",
          },
          callbacks
        );

        addLog("Connecting to reprocessing service...");
        toast.success("Starting reprocessing...");
      } catch (err) {
        const errorMessage =
          err instanceof Error ? err.message : "Failed to start reprocessing";
        setState((prev) => ({
          ...prev,
          isProcessing: false,
          error: errorMessage,
        }));
        toast.error(errorMessage);
        onError?.(errorMessage);
      }
    },
    [
      videoId,
      videoTitle,
      getIdToken,
      state.isProcessing,
      addLog,
      cleanup,
      onComplete,
      onError,
      startJob,
      deferUpdateJob,
      completeJob,
      failJob,
    ]
  );

  const cancel = useCallback(() => {
    cleanup();
    setState({
      isProcessing: false,
      progress: 0,
      logs: [],
      error: null,
      sceneProgress: new Map(),
    });
    toast.info("Reprocessing cancelled");
  }, [cleanup]);

  // Hydrate from processing context on mount/refresh so logs/progress persist
  useEffect(() => {
    const job = getJob(videoId);
    if (job && (job.status === "pending" || job.status === "processing")) {
      setState((prev) => ({
        ...prev,
        isProcessing: true,
        progress: job.progress ?? prev.progress,
        logs: job.logs ?? prev.logs,
        error: null,
      }));
    }
  }, [getJob, videoId]);

  // Cleanup WebSocket on unmount
  useEffect(() => {
    return () => {
      cleanup();
    };
  }, [cleanup]);

  return {
    ...state,
    reprocess,
    cancel,
  };
}
