/**
 * ProcessingClient Component
 *
 * Main component for video processing workflow.
 */

"use client";

import { type FormEvent, useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { DetailedProcessingStatus } from "@/components/shared/DetailedProcessingStatus";
import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { useProcessing } from "@/lib/processing-context";
import { limitLength, sanitizeUrl } from "@/lib/security/validation";
import {
  handleWSMessage,
  type MessageHandlerCallbacks,
} from "@/lib/websocket/messageHandler";
import { createWebSocketConnection, getWebSocketUrl } from "@/lib/websocket-client";

import { ErrorDisplay } from "./ErrorDisplay";
import { useVideoProcessing } from "./hooks";
import { Results } from "./Results";
import { VideoForm } from "./VideoForm";

interface UserSettings {
  plan: string;
}

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
    setCustomPromptUsed,
    videoTitle,
    videoUrl,
    processingStartTime,
    processingStyles,
    processingCustomPrompt,
    sceneProgress,
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
    handleSceneStarted,
    handleSceneCompleted,
    handleClipProgress,
    resetSceneProgress,
  } = useVideoProcessing();

  const { getIdToken, loading: authLoading, user } = useAuth();
  const { startJob, startJobPolling, completeJob } = useProcessing();
  const hasResults = clips.length > 0;
  const [userSettings, setUserSettings] = useState<UserSettings | null>(null);

  // SECURITY: Don't show results if user is not authenticated
  const canShowResults = user !== null && !authLoading;

  // Load user settings to get plan info
  const loadUserSettings = useCallback(async () => {
    if (authLoading || !user) return;
    try {
      const token = await getIdToken();
      if (!token) return;
      const settings = await apiFetch<UserSettings>("/api/settings", { token });
      setUserSettings(settings);
    } catch (err) {
      frontendLogger.error("Failed to load user settings:", err);
    }
  }, [authLoading, user, getIdToken]);

  useEffect(() => {
    void loadUserSettings();
  }, [loadUserSettings]);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    setErrorDetails(null);
    setLogs([]);
    setProgress(0);
    setClips([]);
    setVideoId(null);
    resetSceneProgress(); // Reset scene progress for new processing
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
              styles: styles.length > 0 ? styles : ["intelligent"], // Fallback to default if none selected
              token,
              prompt: sanitizedPrompt || undefined,
            })
          );
        },
        // onMessage
        (message: unknown) => {
          const callbacks: MessageHandlerCallbacks = {
            onLog: (logMessage, timestamp) => {
              log(logMessage, "info", timestamp);
            },
            onProgress: (progressValue) => {
              setProgress(progressValue);
            },
            onError: (errorMessage, errorDetails) => {
              ws.close();
              setError(errorMessage);
              setErrorDetails(errorDetails ?? null);
              toast.error(errorMessage);
              setSubmitting(false);
              void analyticsEvents.videoProcessingFailed({
                errorType: errorDetails ?? "unknown",
                errorMessage,
                style: styles.join(","),
              });
            },
            onDone: (videoId) => {
              ws.close();
              setVideoId(videoId);
              completeJob(videoId); // Mark job as complete in processing context
              const newUrl = new URL(window.location.href);
              newUrl.searchParams.set("id", videoId);
              window.history.pushState({}, "", newUrl.toString());
              toast.success("Video processed successfully!");
              void loadResults(videoId);
            },
            onClipUploaded: (videoId, clipCount, totalClips) => {
              // If we're currently viewing this video, force reload results (bypasses cache)
              if (videoId === searchParams.get("id")) {
                void loadResults(videoId, true); // forceRefresh=true to bypass cache
              }
              // Log progress
              if (clipCount > 0 && totalClips > 0) {
                log(`📦 Clip ${clipCount}/${totalClips} uploaded`, "success");
              }
            },
            // Job tracking for polling fallback
            onJobStarted: (jobId, videoId) => {
              frontendLogger.info(`Job started: ${jobId} for video ${videoId}`);
              // Start tracking the job in processing context with polling fallback
              startJob(videoId);
              startJobPolling(videoId, jobId, token);
            },
            // Detailed progress callbacks
            onSceneStarted: handleSceneStarted,
            onSceneCompleted: handleSceneCompleted,
            onClipProgress: handleClipProgress,
          };

          const handled = handleWSMessage(message, callbacks, searchParams.get("id"));
          if (!handled) {
            // Invalid message - close connection for security
            frontendLogger.error("Invalid WebSocket message format", { message });
            ws.close();
            setError("Invalid message format");
            setSubmitting(false);
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
      {/* Input Section - Only show when not viewing a specific video */}
      {!hasResults && !videoId && (
        <VideoForm
          url={url}
          setUrl={setUrl}
          styles={styles}
          setStyles={setStyles}
          customPrompt={customPrompt}
          setCustomPrompt={setCustomPrompt}
          onSubmit={onSubmit}
          submitting={submitting}
          userPlan={userSettings?.plan}
        />
      )}

      {/* Status Section */}
      {submitting && !videoId && (
        <DetailedProcessingStatus
          progress={progress}
          logs={logs}
          sceneProgress={sceneProgress}
        />
      )}

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
