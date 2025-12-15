/**
 * React hook for job progress tracking
 *
 * Provides a simple interface to track job progress with automatic
 * cleanup on unmount.
 */

"use client";

import { useCallback, useEffect, useRef, useState } from "react";

import {
  getProgressManager,
  type ConnectionState,
  type JobProgress,
  type ProgressEvent,
} from "./ProgressManager";

export interface UseJobProgressOptions {
  /** Callback when progress events are received */
  onEvent?: (event: ProgressEvent) => void;
  /** Callback when status is updated */
  onStatus?: (status: JobProgress) => void;
  /** Callback when connection state changes */
  onConnectionStateChange?: (state: ConnectionState) => void;
}

export interface UseJobProgressReturn {
  /** Start tracking a job */
  startTracking: (jobId: string, token: string) => void;
  /** Stop tracking */
  stopTracking: () => void;
  /** Current job progress */
  progress: JobProgress | null;
  /** Current connection state */
  connectionState: ConnectionState;
  /** Whether currently tracking a job */
  isTracking: boolean;
  /** Manually fetch current status */
  fetchStatus: () => Promise<JobProgress | null>;
}

export function useJobProgress(
  options: UseJobProgressOptions = {}
): UseJobProgressReturn {
  const { onEvent, onStatus, onConnectionStateChange } = options;

  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [connectionState, setConnectionState] =
    useState<ConnectionState>("disconnected");
  const [isTracking, setIsTracking] = useState(false);

  // Use refs to avoid stale closures
  const onEventRef = useRef(onEvent);
  const onStatusRef = useRef(onStatus);
  const onConnectionStateChangeRef = useRef(onConnectionStateChange);

  useEffect(() => {
    onEventRef.current = onEvent;
    onStatusRef.current = onStatus;
    onConnectionStateChangeRef.current = onConnectionStateChange;
  }, [onEvent, onStatus, onConnectionStateChange]);

  // Subscribe to progress manager events
  useEffect(() => {
    const manager = getProgressManager();

    const unsubEvent = manager.onEvent((event) => {
      // Handle connection state changes
      if (event.type === "connection_state") {
        setConnectionState(event.state);
        onConnectionStateChangeRef.current?.(event.state);
      }

      onEventRef.current?.(event);
    });

    const unsubStatus = manager.onStatus((status) => {
      setProgress(status);
      onStatusRef.current?.(status);
    });

    return () => {
      unsubEvent();
      unsubStatus();
    };
  }, []);

  const startTracking = useCallback((jobId: string, token: string) => {
    const manager = getProgressManager();
    setIsTracking(true);
    manager.startTracking(jobId, token);
  }, []);

  const stopTracking = useCallback(() => {
    const manager = getProgressManager();
    manager.stopTracking();
    setIsTracking(false);
    setProgress(null);
  }, []);

  const fetchStatus = useCallback(() => {
    const manager = getProgressManager();
    return manager.fetchStatus(true);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      // Don't stop tracking on unmount - let the manager continue
      // This allows progress to continue when navigating between pages
    };
  }, []);

  return {
    startTracking,
    stopTracking,
    progress,
    connectionState,
    isTracking,
    fetchStatus,
  };
}
