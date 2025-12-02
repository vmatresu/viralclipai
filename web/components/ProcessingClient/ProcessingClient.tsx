/**
 * ProcessingClient Component
 *
 * Main component for video processing workflow.
 */

"use client";

import { type FormEvent, useEffect } from "react";

import { analyticsEvents } from "@/lib/analytics";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";

import { ErrorDisplay } from "./ErrorDisplay";
import { useVideoProcessing } from "./hooks";
import { ProcessingStatus } from "./ProcessingStatus";
import { Results } from "./Results";
import { VideoForm } from "./VideoForm";

export function ProcessingClient() {
  const {
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
    processingStartTime,
    processingStyle,
    processingCustomPrompt,
    log,
    loadResults,
    setSubmitting,
    setProgress,
    setError,
    setErrorDetails,
    setVideoId,
    setClips,
    searchParams,
  } = useVideoProcessing();

  const { getIdToken } = useAuth();
  const hasResults = clips.length > 0;

  useEffect(() => {
    const existingId = searchParams.get("id");
    if (existingId) {
      setVideoId(existingId);
      void loadResults(existingId);
    }
  }, [searchParams, loadResults, setVideoId]);

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
    processingStyle.current = style;
    processingCustomPrompt.current = customPrompt;

    try {
      const token = await getIdToken();
      if (!token) {
        log("You must be signed in to process videos.", "error");
        // eslint-disable-next-line no-alert
        alert("Please sign in with your Google account to use this app.");
        setSubmitting(false);
        return;
      }

      // Track processing start
      void analyticsEvents.videoProcessingStarted({
        style,
        hasCustomPrompt: customPrompt.trim().length > 0,
        videoUrl: url,
      });

      const apiBase = process.env.NEXT_PUBLIC_API_BASE_URL ?? window.location.origin;
      const baseUrl = new URL(apiBase);
      const wsProtocol = baseUrl.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${wsProtocol}//${baseUrl.host}/ws/process`;
      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        log("Connected to server...", "success");
        ws.send(
          JSON.stringify({
            url,
            style,
            token,
            prompt: customPrompt.trim() || undefined,
          })
        );
      };

      ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        if (data.type === "log") {
          log(data.message);
        } else if (data.type === "progress") {
          setProgress(data.value ?? 0);
        } else if (data.type === "error") {
          ws.close();
          const errorMessage = data.message || "An unexpected error occurred.";
          setError(errorMessage);
          setErrorDetails(data.details || null);
          setSubmitting(false);

          // Track processing failure
          void analyticsEvents.videoProcessingFailed({
            errorType: data.details ?? "unknown",
            errorMessage,
            style,
          });
        } else if (data.type === "done") {
          ws.close();
          const id = data.videoId as string;
          setVideoId(id);
          const newUrl = new URL(window.location.href);
          newUrl.searchParams.set("id", id);
          window.history.pushState({}, "", newUrl.toString());

          // Track processing completion after loading results
          void loadResults(id);
        }
      };

      ws.onerror = (ev) => {
        frontendLogger.error("WebSocket error occurred", ev);
        log("WebSocket error occurred.", "error");
      };

      ws.onclose = () => {
        if (!hasResults && !error) {
          setSubmitting(false);
        }
      };
    } catch (err: unknown) {
      frontendLogger.error("Failed to start processing", err);
      const errorMessage =
        err instanceof Error ? err.message : "Failed to start processing";
      setError(errorMessage);
      setSubmitting(false);

      // Track processing failure
      void analyticsEvents.videoProcessingFailed({
        errorType: "initialization_error",
        errorMessage,
        style,
      });
    }
  }

  function handleReset() {
    setVideoId(null);
    setClips([]);
    const newUrl = new URL(window.location.href);
    newUrl.searchParams.delete("id");
    window.history.pushState({}, "", newUrl.toString());
  }

  return (
    <div className="space-y-8">
      {/* Input Section */}
      {!hasResults && (
        <VideoForm
          url={url}
          setUrl={setUrl}
          style={style}
          setStyle={setStyle}
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
      {videoId && !error && (
        <Results
          videoId={videoId}
          clips={clips}
          customPromptUsed={customPromptUsed}
          log={log}
          onReset={handleReset}
        />
      )}
    </div>
  );
}
