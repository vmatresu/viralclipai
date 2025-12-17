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
    resetSceneProgress,
  } = useVideoProcessing();

  const { getIdToken, loading: authLoading, user } = useAuth();
  const { startProcessing } = useProcessing();
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
    resetSceneProgress();
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

      log("Submitting video for processing...", "info");

      // Submit via REST API instead of WebSocket
      const response = await apiFetch<{
        video_id: string;
        job_id: string;
        status: string;
        message?: string;
      }>("/api/videos/process", {
        method: "POST",
        token,
        body: {
          url: sanitizedUrl,
          styles: styles.length > 0 ? styles : ["intelligent"],
          prompt: sanitizedPrompt || undefined,
        },
      });

      // Mark as processing in context
      startProcessing(response.video_id);
      setVideoId(response.video_id);

      // Update URL with video ID
      const newUrl = new URL(window.location.href);
      newUrl.searchParams.set("id", response.video_id);
      window.history.pushState({}, "", newUrl.toString());

      log("Processing started! Refresh the page to check progress.", "success");
      toast.success(
        "Processing started! Your video is being analyzed. Refresh the page to check progress."
      );

      setSubmitting(false);
      setProgress(5); // Show initial progress

      // Load initial results after a delay
      setTimeout(() => {
        void loadResults(response.video_id);
      }, 3000);
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
