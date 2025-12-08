"use client";

/**
 * Processing Context
 *
 * Provides a global state for tracking video processing jobs.
 * Persists state to localStorage and provides real-time updates via WebSocket.
 */

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";

import { useAuth } from "@/lib/auth";

// Types
export interface ProcessingJob {
  videoId: string;
  videoTitle?: string;
  status: "pending" | "processing" | "completed" | "failed";
  progress: number;
  currentStep?: string;
  logs: string[];
  startedAt: number;
  completedAt?: number;
  error?: string;
  clipsCompleted: number;
  totalClips: number;
  waitingForProcessing?: boolean; // If true, ignore 'completed'/'failed' status from API until 'processing' is seen or timeout
}

interface ProcessingContextValue {
  jobs: Map<string, ProcessingJob>;
  activeJobCount: number;
  getJob: (videoId: string) => ProcessingJob | undefined;
  startJob: (
    videoId: string,
    videoTitle?: string,
    totalClips?: number,
    waitForProcessing?: boolean
  ) => void;
  updateJob: (videoId: string, updates: Partial<ProcessingJob>) => void;
  completeJob: (videoId: string) => void;
  failJob: (videoId: string, error: string) => void;
  clearJob: (videoId: string) => void;
  clearAllCompleted: () => void;
  isVideoProcessing: (videoId: string) => boolean;
}

const ProcessingContext = createContext<ProcessingContextValue | null>(null);

const STORAGE_KEY = "vclip_processing_jobs";
const JOB_EXPIRY_MS = 24 * 60 * 60 * 1000; // 24 hours

// Helper to serialize Map to localStorage
function saveJobsToStorage(jobs: Map<string, ProcessingJob>) {
  try {
    const serializable = Array.from(jobs.entries());
    localStorage.setItem(STORAGE_KEY, JSON.stringify(serializable));
  } catch (e) {
    console.error("Failed to save processing jobs to localStorage:", e);
  }
}

// Helper to load Map from localStorage
function loadJobsFromStorage(): Map<string, ProcessingJob> {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return new Map();

    const entries: [string, ProcessingJob][] = JSON.parse(stored);
    const now = Date.now();

    // Filter out expired jobs
    const validEntries = entries.filter(([, job]) => {
      const age = now - job.startedAt;
      return age < JOB_EXPIRY_MS;
    });

    return new Map(validEntries);
  } catch (e) {
    console.error("Failed to load processing jobs from localStorage:", e);
    return new Map();
  }
}

// Request notification permission
async function requestNotificationPermission(): Promise<boolean> {
  if (!("Notification" in window)) {
    return false;
  }

  if (Notification.permission === "granted") {
    return true;
  }

  if (Notification.permission !== "denied") {
    const permission = await Notification.requestPermission();
    return permission === "granted";
  }

  return false;
}

export function ProcessingProvider({ children }: { children: React.ReactNode }) {
  useAuth(); // Ensure auth is initialized for processing state hydration
  const [jobs, setJobs] = useState<Map<string, ProcessingJob>>(new Map());
  const [initialized, setInitialized] = useState(false);

  // Load from localStorage on mount
  useEffect(() => {
    const loaded = loadJobsFromStorage();
    setJobs(loaded);
    setInitialized(true);

    // Request notification permission
    void requestNotificationPermission();
  }, []);

  // Save to localStorage when jobs change
  useEffect(() => {
    if (initialized) {
      saveJobsToStorage(jobs);
    }
  }, [jobs, initialized]);

  const getJob = useCallback((videoId: string) => jobs.get(videoId), [jobs]);

  const startJob = useCallback(
    (
      videoId: string,
      videoTitle?: string,
      totalClips?: number,
      waitForProcessing?: boolean
    ) => {
      setJobs((prev) => {
        const next = new Map(prev);
        next.set(videoId, {
          videoId,
          videoTitle,
          status: "processing",
          progress: 0,
          logs: [],
          startedAt: Date.now(),
          clipsCompleted: 0,
          totalClips: totalClips ?? 0,
          waitingForProcessing: waitForProcessing ?? false,
        });
        return next;
      });
    },
    []
  );

  const updateJob = useCallback((videoId: string, updates: Partial<ProcessingJob>) => {
    setJobs((prev) => {
      const existing = prev.get(videoId);
      if (!existing) return prev;

      const next = new Map(prev);
      next.set(videoId, { ...existing, ...updates });
      return next;
    });
  }, []);

  const completeJob = useCallback((videoId: string) => {
    setJobs((prev) => {
      const existing = prev.get(videoId);
      if (!existing) return prev;

      const next = new Map(prev);
      next.set(videoId, {
        ...existing,
        status: "completed",
        progress: 100,
        completedAt: Date.now(),
        currentStep: "Complete!",
        // Reset logs after completion so refreshes don't show stale monitoring
        logs: [],
      });
      return next;
    });
  }, []);

  const failJob = useCallback((videoId: string, error: string) => {
    setJobs((prev) => {
      const existing = prev.get(videoId);
      if (!existing) return prev;

      const next = new Map(prev);
      next.set(videoId, {
        ...existing,
        status: "failed",
        completedAt: Date.now(),
        error,
      });
      return next;
    });
  }, []);

  const clearJob = useCallback((videoId: string) => {
    setJobs((prev) => {
      const next = new Map(prev);
      next.delete(videoId);
      return next;
    });
  }, []);

  const clearAllCompleted = useCallback(() => {
    setJobs((prev) => {
      const next = new Map(prev);
      for (const [videoId, job] of next) {
        if (job.status === "completed" || job.status === "failed") {
          next.delete(videoId);
        }
      }
      return next;
    });
  }, []);

  const isVideoProcessing = useCallback(
    (videoId: string) => {
      const job = jobs.get(videoId);
      return job?.status === "pending" || job?.status === "processing";
    },
    [jobs]
  );

  const activeJobCount = Array.from(jobs.values()).filter(
    (j) => j.status === "pending" || j.status === "processing"
  ).length;

  return (
    <ProcessingContext.Provider
      value={{
        jobs,
        activeJobCount,
        getJob,
        startJob,
        updateJob,
        completeJob,
        failJob,
        clearJob,
        clearAllCompleted,
        isVideoProcessing,
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
