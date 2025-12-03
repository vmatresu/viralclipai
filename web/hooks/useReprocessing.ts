/**
 * Custom hook for scene reprocessing via WebSocket.
 * 
 * Provides a clean interface for reprocessing scenes with real-time progress updates.
 */

import { useCallback, useRef, useState, useEffect } from "react";
import { toast } from "sonner";
import { useAuth } from "@/lib/auth";
import { reprocessScenesWebSocket, type ReprocessCallbacks } from "@/lib/websocket/reprocess-client";

interface ReprocessingState {
  isProcessing: boolean;
  progress: number;
  logs: string[];
  error: string | null;
}

interface UseReprocessingOptions {
  videoId: string;
  onComplete?: () => void;
  onError?: (error: string) => void;
}


export function useReprocessing({
  videoId,
  onComplete,
  onError,
}: UseReprocessingOptions) {
  const { getIdToken } = useAuth();
  const [state, setState] = useState<ReprocessingState>({
    isProcessing: false,
    progress: 0,
    logs: [],
    error: null,
  });
  const wsRef = useRef<WebSocket | null>(null);

  const addLog = useCallback((message: string) => {
    setState((prev) => ({
      ...prev,
      logs: [...prev.logs, message],
    }));
  }, []);

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

        // Reset state
        setState({
          isProcessing: true,
          progress: 0,
          logs: [],
          error: null,
        });

        // Use dedicated WebSocket client for better separation of concerns
        const callbacks: ReprocessCallbacks = {
          onProgress: (value) => {
            setState((prev) => ({
              ...prev,
              progress: value,
            }));
          },
          onLog: (message) => {
            addLog(message);
          },
          onDone: () => {
            setState((prev) => ({
              ...prev,
              isProcessing: false,
              progress: 100,
            }));
            addLog("Reprocessing complete!");
            toast.success("Reprocessing complete!");
            cleanup();
            onComplete?.();
          },
          onError: (message, details) => {
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
            cleanup();
            onError?.(errorMsg);
          },
          onClose: () => {
            // Connection closed - check if we're still processing
            if (state.isProcessing) {
              setState((prev) => ({
                ...prev,
                isProcessing: false,
                error: "Connection closed unexpectedly",
              }));
              toast.error("Connection closed unexpectedly");
              onError?.("Connection closed unexpectedly");
            }
            cleanup();
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
    [videoId, getIdToken, state.isProcessing, addLog, cleanup, onComplete, onError]
  );

  const cancel = useCallback(() => {
    cleanup();
    setState({
      isProcessing: false,
      progress: 0,
      logs: [],
      error: null,
    });
    toast.info("Reprocessing cancelled");
  }, [cleanup]);

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

