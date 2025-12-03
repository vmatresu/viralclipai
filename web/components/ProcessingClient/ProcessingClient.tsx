/**
 * ProcessingClient Component
 *
 * Main component for video processing workflow.
 */

"use client";

import { type FormEvent } from "react";
import { toast } from "sonner";

import { analyticsEvents } from "@/lib/analytics";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { limitLength, sanitizeUrl } from "@/lib/security/validation";
import { getWebSocketUrl, createWebSocketConnection } from "@/lib/websocket-client";

import { ErrorDisplay } from "./ErrorDisplay";
import { useVideoProcessing } from "./hooks";
import { ProcessingStatus } from "./ProcessingStatus";
import { Results } from "./Results";
import { VideoForm } from "./VideoForm";

export function ProcessingClient() {
  const {
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
    videoTitle,
    videoUrl,
    processingStartTime,
    processingStyles,
    processingCustomPrompt,
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
  } = useVideoProcessing();

  const { getIdToken, loading: authLoading, user } = useAuth();
  const hasResults = clips.length > 0;
  
  // SECURITY: Don't show results if user is not authenticated
  const canShowResults = user !== null && !authLoading;

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    setErrorDetails(null);
    setLogs([]);
    setProgress(0);
    setClips([]);
    setVideoId(null);
    // Store processing parameters at start time for accurate analytics tracking
    processingStartTime.current = Date.now();
    processingStyles.current = [...styles];
    processingCustomPrompt.current = customPrompt;

    try {
      const token = await getIdToken();
      if (!token) {
        log("You must be signed in to process videos.", "error");
        toast.error("Please sign in with your Google account to use this app.");
        setSubmitting(false);
        return;
      }

      // Validate and sanitize inputs
      const sanitizedUrl = sanitizeUrl(url);
      if (!sanitizedUrl) {
        const msg = "Invalid video URL. Please provide a valid YouTube or TikTok URL.";
        log(msg, "error");
        toast.error(msg);
        setSubmitting(false);
        return;
      }

      const sanitizedPrompt = limitLength(customPrompt.trim(), 5000);

      // Track processing start
      void analyticsEvents.videoProcessingStarted({
        style: styles.join(","),
        hasCustomPrompt: sanitizedPrompt.length > 0,
        videoUrl: sanitizedUrl,
      });

      const wsUrl = getWebSocketUrl(process.env.NEXT_PUBLIC_API_BASE_URL);

      const ws = createWebSocketConnection(
        wsUrl,
        // onOpen
        () => {
          log("Connected to server...", "success");
          ws.send(
            JSON.stringify({
              url: sanitizedUrl,
              styles: styles.length > 0 ? styles : ["split"], // Fallback to default if none selected
              token,
              prompt: sanitizedPrompt || undefined,
            })
          );
        },
        // onMessage
        (message: any) => { // Using any here as strict type parsing is done inside createWebSocketConnection/here
            // Note: createWebSocketConnection parses JSON but returns `any`.
            // We should validate the structure here.
            
            if (!message || typeof message !== "object") {
               frontendLogger.error("Invalid WebSocket message format", { message });
               ws.close();
               setError("Invalid message format");
               setSubmitting(false);
               return;
            }

            const typedMessage = message as {
                type?: string;
                message?: string;
                value?: number;
                videoId?: string;
                details?: string;
                timestamp?: string;
            };

            if (typedMessage.type === "log") {
                const logMessage = typeof typedMessage.message === "string"
                    ? typedMessage.message.substring(0, 1000)
                    : "Unknown log message";
                const timestamp = typeof typedMessage.timestamp === "string"
                    ? typedMessage.timestamp
                    : undefined;
                log(logMessage, "info", timestamp);
            } else if (typedMessage.type === "progress") {
                const progressValue = typeof typedMessage.value === "number" && typedMessage.value >= 0 && typedMessage.value <= 100
                    ? typedMessage.value
                    : 0;
                setProgress(progressValue);
            } else if (typedMessage.type === "error") {
                ws.close();
                const errorMessage = typeof typedMessage.message === "string"
                    ? typedMessage.message.substring(0, 500)
                    : "An unexpected error occurred.";
                const errorDetails = typeof typedMessage.details === "string"
                    ? typedMessage.details.substring(0, 200)
                    : null;
                
                setError(errorMessage);
                setErrorDetails(errorDetails);
                toast.error(errorMessage);
                setSubmitting(false);

                void analyticsEvents.videoProcessingFailed({
                    errorType: errorDetails ?? "unknown",
                    errorMessage,
                    style: styles.join(","),
                });
            } else if (typedMessage.type === "done") {
                ws.close();
                const id = typeof typedMessage.videoId === "string" && typedMessage.videoId.trim() !== ""
                    ? typedMessage.videoId.trim()
                    : null;

                if (!id) {
                    setError("Invalid video ID received");
                    toast.error("Invalid video ID received");
                    setSubmitting(false);
                    return;
                }

                const sanitizedId = id.replace(/[^a-zA-Z0-9_-]/g, "");
                if (sanitizedId !== id) {
                    frontendLogger.warn("Video ID contained invalid characters", { id });
                }

                setVideoId(sanitizedId);
                const newUrl = new URL(window.location.href);
                newUrl.searchParams.set("id", sanitizedId);
                window.history.pushState({}, "", newUrl.toString());
                
                toast.success("Video processed successfully!");
                void loadResults(sanitizedId);
            } else {
                frontendLogger.warn("Unknown WebSocket message type", { type: typedMessage.type });
            }
        },
        // onError
        (ev) => {
            frontendLogger.error("WebSocket error occurred", ev);
            log("WebSocket error occurred.", "error");
            toast.error("Connection error occurred");
        },
        // onClose
        () => {
            if (!hasResults && !error) {
                setSubmitting(false);
            }
        }
      );

    } catch (err: unknown) {
      frontendLogger.error("Failed to start processing", err);
      const errorMessage =
        err instanceof Error ? err.message : "Failed to start processing";
      setError(errorMessage);
      toast.error(errorMessage);
      setSubmitting(false);

      void analyticsEvents.videoProcessingFailed({
        errorType: "initialization_error",
        errorMessage,
        style: styles.join(","),
      });
    }
  }

  function handleReset() {
    setVideoId(null);
    setClips([]);
    setVideoTitle(null);
    setVideoUrl(null);
    setCustomPromptUsed(null);
    const newUrl = new URL(window.location.href);
    newUrl.searchParams.delete("id");
    window.history.pushState({}, "", newUrl.toString());
  }

  const handleClipDeleted = async (clipName: string) => {
    // Reload clips from the API to get updated list
    if (videoId) {
      try {
        await loadResults(videoId);
      } catch (err) {
        frontendLogger.error("Failed to reload clips after deletion", err);
        // Optimistically remove the clip from the list
        setClips((prev) => prev.filter((clip) => clip.name !== clipName));
      }
    } else {
      // Fallback: optimistically remove from list
      setClips((prev) => prev.filter((clip) => clip.name !== clipName));
    }
  };

  return (
    <div className="space-y-8">
      {/* Input Section */}
      {!hasResults && (
        <VideoForm
          url={url}
          setUrl={setUrl}
          styles={styles}
          setStyles={setStyles}
          customPrompt={customPrompt}
          setCustomPrompt={setCustomPrompt}
          onSubmit={onSubmit}
          submitting={submitting}
        />
      )}

      {/* Status Section */}
      {submitting && <ProcessingStatus progress={progress} logs={logs} />}

      {/* Error Section */}
      {error && <ErrorDisplay error={error} errorDetails={errorDetails} />}

      {/* Results Section */}
      {/* SECURITY: Only show results when user is authenticated */}
      {videoId && !error && canShowResults && (
        <Results
          videoId={videoId}
          clips={clips}
          customPromptUsed={customPromptUsed}
          videoTitle={videoTitle}
          videoUrl={videoUrl}
          log={log}
          onReset={handleReset}
          onClipDeleted={handleClipDeleted}
          onTitleUpdated={(newTitle) => {
            setVideoTitle(newTitle);
          }}
        />
      )}
    </div>
  );
}