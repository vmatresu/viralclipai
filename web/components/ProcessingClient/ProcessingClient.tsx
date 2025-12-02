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
import { limitLength, sanitizeUrl } from "@/lib/security/validation";

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

      // Validate and sanitize inputs
      const sanitizedUrl = sanitizeUrl(url);
      if (!sanitizedUrl) {
        log("Invalid video URL. Please provide a valid YouTube or TikTok URL.", "error");
        setSubmitting(false);
        return;
      }

      const sanitizedPrompt = limitLength(customPrompt.trim(), 5000);

      // Track processing start
      void analyticsEvents.videoProcessingStarted({
        style,
        hasCustomPrompt: sanitizedPrompt.length > 0,
        videoUrl: sanitizedUrl,
      });

      // Validate and sanitize API base URL
      const apiBase = process.env.NEXT_PUBLIC_API_BASE_URL ?? window.location.origin;
      let baseUrl: URL;
      try {
        baseUrl = new URL(apiBase);
        // Ensure only http/https protocols
        if (baseUrl.protocol !== "http:" && baseUrl.protocol !== "https:") {
          throw new Error("Invalid API protocol");
        }
      } catch {
        throw new Error("Invalid API base URL configuration");
      }

      // Build WebSocket URL securely
      const wsProtocol = baseUrl.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${wsProtocol}//${baseUrl.host}/ws/process`;

      // Validate WebSocket URL
      if (
        !wsUrl.startsWith("ws://") &&
        !wsUrl.startsWith("wss://") &&
        !wsUrl.startsWith(`${window.location.protocol === "https:" ? "wss" : "ws"}://`)
      ) {
        throw new Error("Invalid WebSocket URL");
      }

      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        log("Connected to server...", "success");
        ws.send(
          JSON.stringify({
            url: sanitizedUrl,
            style,
            token,
            prompt: sanitizedPrompt || undefined,
          })
        );
      };

      ws.onmessage = (event) => {
        // Security: Limit message size to prevent DoS
        if (event.data.length > 1024 * 1024) {
          // 1MB limit
          frontendLogger.error("WebSocket message too large", {
            size: event.data.length,
          });
          ws.close();
          setError("Received message is too large");
          setSubmitting(false);
          return;
        }

        let data: unknown;
        try {
          data = JSON.parse(event.data);
        } catch (error) {
          frontendLogger.error("Failed to parse WebSocket message", error);
          ws.close();
          setError("Invalid message format received");
          setSubmitting(false);
          return;
        }

        // Validate data structure
        if (!data || typeof data !== "object") {
          frontendLogger.error("Invalid WebSocket message format", { data });
          ws.close();
          setError("Invalid message format");
          setSubmitting(false);
          return;
        }

        const message = data as {
          type?: string;
          message?: string;
          value?: number;
          videoId?: string;
          details?: string;
        };
        if (message.type === "log") {
          // Sanitize log message
          const logMessage =
            typeof message.message === "string"
              ? message.message.substring(0, 1000)
              : "Unknown log message";
          log(logMessage);
        } else if (message.type === "progress") {
          // Validate progress value
          const progressValue =
            typeof message.value === "number" &&
            message.value >= 0 &&
            message.value <= 100
              ? message.value
              : 0;
          setProgress(progressValue);
        } else if (message.type === "error") {
          ws.close();
          // Sanitize error message
          const errorMessage =
            typeof message.message === "string"
              ? message.message.substring(0, 500)
              : "An unexpected error occurred.";
          // Don't expose internal error details
          const errorDetails =
            typeof message.details === "string"
              ? message.details.substring(0, 200)
              : null;
          setError(errorMessage);
          setErrorDetails(errorDetails);
          setSubmitting(false);

          // Track processing failure
          void analyticsEvents.videoProcessingFailed({
            errorType: errorDetails ?? "unknown",
            errorMessage,
            style,
          });
        } else if (message.type === "done") {
          ws.close();
          // Validate video ID
          const id =
            typeof message.videoId === "string" && message.videoId.trim() !== ""
              ? message.videoId.trim()
              : null;

          if (!id) {
            setError("Invalid video ID received");
            setSubmitting(false);
            return;
          }

          // Sanitize video ID (prevent XSS in URL)
          const sanitizedId = id.replace(/[^a-zA-Z0-9_-]/g, "");
          if (sanitizedId !== id) {
            frontendLogger.warn("Video ID contained invalid characters", { id });
          }

          setVideoId(sanitizedId);
          const newUrl = new URL(window.location.href);
          newUrl.searchParams.set("id", sanitizedId);
          window.history.pushState({}, "", newUrl.toString());

          // Track processing completion after loading results
          void loadResults(sanitizedId);
        } else {
          frontendLogger.warn("Unknown WebSocket message type", {
            type: message.type,
          });
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
