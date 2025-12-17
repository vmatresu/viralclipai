"use client";

/**
 * Processing Context (Simplified)
 *
 * Minimal state for tracking which videos were initiated for processing
 * in this session. Actual status is fetched from Firebase on page load/refresh.
 *
 * This replaces the complex WebSocket-based real-time tracking with a simple
 * "fire and forget" model where users refresh to see updates.
 */

import React, { createContext, useCallback, useContext, useState } from "react";

// ============================================================================
// Types
// ============================================================================

interface ProcessingContextValue {
  /** Set of video IDs currently being processed (initiated this session) */
  processingVideos: Set<string>;
  /** Mark a video as processing (just started) */
  startProcessing: (videoId: string) => void;
  /** Mark a video as no longer processing */
  stopProcessing: (videoId: string) => void;
  /** Check if a video was initiated for processing this session */
  isProcessing: (videoId: string) => boolean;
  /** Number of videos initiated for processing this session */
  activeJobCount: number;
}

const ProcessingContext = createContext<ProcessingContextValue | null>(null);

// ============================================================================
// Provider Component
// ============================================================================

export function ProcessingProvider({ children }: { children: React.ReactNode }) {
  const [processingVideos, setProcessingVideos] = useState<Set<string>>(new Set());

  const startProcessing = useCallback((videoId: string) => {
    setProcessingVideos((prev) => {
      const next = new Set(prev);
      next.add(videoId);
      return next;
    });
  }, []);

  const stopProcessing = useCallback((videoId: string) => {
    setProcessingVideos((prev) => {
      const next = new Set(prev);
      next.delete(videoId);
      return next;
    });
  }, []);

  const isProcessing = useCallback(
    (videoId: string) => processingVideos.has(videoId),
    [processingVideos]
  );

  const activeJobCount = processingVideos.size;

  return (
    <ProcessingContext.Provider
      value={{
        processingVideos,
        startProcessing,
        stopProcessing,
        isProcessing,
        activeJobCount,
      }}
    >
      {children}
    </ProcessingContext.Provider>
  );
}

export function useProcessing() {
  const context = useContext(ProcessingContext);
  if (!context) {
    throw new Error("useProcessing must be used within a ProcessingProvider");
  }
  return context;
}
