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
  useRef,
  useState,
} from "react";
import { toast } from "sonner";

import { apiFetch } from "@/lib/apiClient";
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
}

interface ProcessingContextValue {
  jobs: Map<string, ProcessingJob>;
  activeJobCount: number;
  getJob: (videoId: string) => ProcessingJob | undefined;
  startJob: (videoId: string, videoTitle?: string, totalClips?: number) => void;
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

// Show browser notification
function showNotification(title: string, body: string, onClick?: () => void) {
  if (!("Notification" in window) || Notification.permission !== "granted") {
    return;
  }

  try {
    const notification = new Notification(title, {
      body,
      icon: "/favicon.ico",
      tag: "vclip-processing",
    });

    if (onClick) {
      notification.onclick = () => {
        window.focus();
        onClick();
        notification.close();
      };
    }

    // Auto-close after 10 seconds
    setTimeout(() => notification.close(), 10000);
  } catch (e) {
    console.error("Failed to show notification:", e);
  }
}

export function ProcessingProvider({ children }: { children: React.ReactNode }) {
  const { getIdToken, user } = useAuth();
  const [jobs, setJobs] = useState<Map<string, ProcessingJob>>(new Map());
  const [initialized, setInitialized] = useState(false);
  const pollIntervalRef = useRef<NodeJS.Timeout | null>(null);

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

  // Poll for status updates on processing jobs
  useEffect(() => {
    if (!user || !initialized) return;

    const processingJobs = Array.from(jobs.values()).filter(
      (j) => j.status === "pending" || j.status === "processing"
    );

    if (processingJobs.length === 0) {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
      return;
    }

    const pollStatus = async () => {
      try {
        const token = await getIdToken();
        if (!token) return;

        const data = await apiFetch<{
          videos: Array<{
            video_id?: string;
            id?: string;
            status?: string;
            clips_count?: number;
            video_title?: string;
          }>;
        }>("/api/user/videos", { token });

        const videoMap = new Map(data.videos.map((v) => [v.video_id ?? v.id ?? "", v]));

        setJobs((prev) => {
          const next = new Map(prev);
          let hasChanges = false;

          for (const [videoId, job] of next) {
            if (job.status !== "pending" && job.status !== "processing") continue;

            const apiVideo = videoMap.get(videoId);
            if (!apiVideo) continue;

            if (apiVideo.status === "completed") {
              hasChanges = true;
              next.set(videoId, {
                ...job,
                status: "completed",
                progress: 100,
                completedAt: Date.now(),
                clipsCompleted: apiVideo.clips_count ?? job.totalClips,
                currentStep: "Complete!",
              });

              // Show notification
              showNotification(
                "Video Processing Complete! 🎉",
                `${apiVideo.video_title ?? "Your video"} is ready to view.`,
                () => {
                  window.location.href = `/?id=${encodeURIComponent(videoId)}`;
                }
              );

              toast.success(`${apiVideo.video_title ?? "Video"} processing complete!`);
            } else if (apiVideo.status === "failed") {
              hasChanges = true;
              next.set(videoId, {
                ...job,
                status: "failed",
                completedAt: Date.now(),
                error: "Processing failed",
              });

              toast.error(`${apiVideo.video_title ?? "Video"} processing failed`);
            }
          }

          return hasChanges ? next : prev;
        });
      } catch (e) {
        console.error("Failed to poll processing status:", e);
      }
    };

    // Poll immediately and then every 5 seconds
    void pollStatus();
    pollIntervalRef.current = setInterval(() => void pollStatus(), 5000);

    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, [user, initialized, jobs, getIdToken]);

  const getJob = useCallback((videoId: string) => jobs.get(videoId), [jobs]);

  const startJob = useCallback(
    (videoId: string, videoTitle?: string, totalClips?: number) => {
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
