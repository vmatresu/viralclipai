"use client";

/**
 * Processing Context
 *
 * Provides a global state for tracking video processing jobs.
 * Features:
 * - Persists state to localStorage
 * - Tracks scene progress for detailed UI
 * - Supports recovery after page refresh
 * - Integrates with backend job status polling
 */

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";

import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { getProgressManager, type JobProgress } from "@/lib/progress";

// ============================================================================
// Types
// ============================================================================

export interface SceneProgressData {
  sceneId: number;
  sceneTitle: string;
  styleCount: number;
  startSec?: number;
  durationSec?: number;
  status: "pending" | "processing" | "completed" | "failed";
  clipsCompleted: number;
  clipsFailed: number;
  currentSteps: Record<string, { step: string; details?: string }>;
}

export interface ProcessingJob {
  videoId: string;
  videoTitle?: string;
  status: "pending" | "processing" | "completed" | "failed" | "stale";
  progress: number;
  currentStep?: string;
  logs: string[];
  startedAt: number;
  completedAt?: number;
  error?: string;
  clipsCompleted: number;
  totalClips: number;
  /** Backend job ID for REST API polling */
  jobId?: string;
  /** Scene progress for detailed tracking */
  sceneProgress?: Record<number, SceneProgressData>;
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
  /** Set the backend job ID for a video */
  setJobId: (videoId: string, jobId: string) => void;
  /** Start REST API polling for job status */
  startJobPolling: (videoId: string, jobId: string, token: string) => void;
  /** Stop polling for job status */
  stopJobPolling: (videoId: string) => void;
  /** Update scene progress */
  updateSceneProgress: (
    videoId: string,
    sceneId: number,
    updates: Partial<SceneProgressData>
  ) => void;
  /** Initialize scene progress */
  initSceneProgress: (videoId: string, scene: SceneProgressData) => void;
  /** Get scene progress for a video */
  getSceneProgress: (videoId: string) => Record<number, SceneProgressData> | undefined;
  /** Check and recover any stale jobs on mount */
  checkAndRecoverJobs: () => Promise<void>;
}

const ProcessingContext = createContext<ProcessingContextValue | null>(null);

function normalizeJobStatus(status: JobProgress["status"]): ProcessingJob["status"] {
  if (status === "queued") return "pending";
  return status;
}

// ============================================================================
// Storage Helpers
// ============================================================================

const STORAGE_KEY = "vclip_processing_jobs";
const SCENE_PROGRESS_KEY_PREFIX = "vclip_scene_progress_";
const JOB_EXPIRY_MS = 24 * 60 * 60 * 1000; // 24 hours

function saveJobsToStorage(jobs: Map<string, ProcessingJob>) {
  try {
    const serializable = Array.from(jobs.entries()).map(([id, job]) => {
      // Don't persist sceneProgress to main storage (too large)
      const { sceneProgress: _sceneProgress, ...rest } = job;
      return [id, rest] as [string, Omit<ProcessingJob, "sceneProgress">];
    });
    localStorage.setItem(STORAGE_KEY, JSON.stringify(serializable));
  } catch (err) {
    frontendLogger.error("Failed to save processing jobs to localStorage:", err);
  }
}

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

    // Load scene progress for each valid job
    const jobsWithSceneProgress = validEntries.map(([id, job]) => {
      const sceneProgress = loadSceneProgressFromStorage(id);
      return [id, { ...job, sceneProgress }] as [string, ProcessingJob];
    });

    return new Map(jobsWithSceneProgress);
  } catch (e) {
    frontendLogger.error("Failed to load processing jobs from localStorage:", e);
    return new Map();
  }
}

function saveSceneProgressToStorage(
  videoId: string,
  progress: Record<number, SceneProgressData>
) {
  try {
    const key = `${SCENE_PROGRESS_KEY_PREFIX}${videoId}`;
    localStorage.setItem(key, JSON.stringify(progress));
  } catch (e) {
    frontendLogger.error("Failed to save scene progress:", e);
  }
}

function loadSceneProgressFromStorage(
  videoId: string
): Record<number, SceneProgressData> | undefined {
  try {
    const key = `${SCENE_PROGRESS_KEY_PREFIX}${videoId}`;
    const stored = localStorage.getItem(key);
    if (!stored) return undefined;
    return JSON.parse(stored);
  } catch {
    return undefined;
  }
}

function clearSceneProgressFromStorage(videoId: string) {
  try {
    const key = `${SCENE_PROGRESS_KEY_PREFIX}${videoId}`;
    localStorage.removeItem(key);
  } catch {
    // Ignore storage errors
  }
}

// ============================================================================
// Notification Helper
// ============================================================================

async function requestNotificationPermission(): Promise<boolean> {
  if (typeof window === "undefined" || !("Notification" in window)) {
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

// ============================================================================
// Provider Component
// ============================================================================

export function ProcessingProvider({ children }: { children: React.ReactNode }) {
  const { getIdToken } = useAuth();
  const [jobs, setJobs] = useState<Map<string, ProcessingJob>>(new Map());
  const [initialized, setInitialized] = useState(false);
  const pollingJobsRef = useRef<Set<string>>(new Set());

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

      // Also save scene progress for each job
      for (const [videoId, job] of jobs) {
        if (job.sceneProgress) {
          saveSceneProgressToStorage(videoId, job.sceneProgress);
        }
      }
    }
  }, [jobs, initialized]);

  // Check and recover stale jobs on mount
  const checkAndRecoverJobs = useCallback(async () => {
    if (!initialized) return;

    const token = await getIdToken();
    if (!token) return;

    const activeJobs = Array.from(jobs.entries()).filter(
      ([, job]) => job.status === "processing" || job.status === "pending"
    );

    // Recover jobs in parallel for better performance
    const recoveryPromises = activeJobs
      .filter(([, job]) => job.jobId)
      .map(async ([videoId, job]) => {
        frontendLogger.info(`Recovering job status for ${videoId} (job: ${job.jobId})`);

        const manager = getProgressManager();
        try {
          // Check current status from backend
          const jobId = job.jobId;
          if (!jobId) return;
          void manager.startTracking(jobId, token);
          const status = await manager.fetchStatus(true);

          if (status) {
            const normalizedStatus = normalizeJobStatus(status.status);
            // Update local state based on backend status
            setJobs((prev) => {
              const next = new Map(prev);
              const existing = next.get(videoId);
              if (existing) {
                const updates: Partial<ProcessingJob> = {
                  status: normalizedStatus,
                  progress: status.progress,
                  clipsCompleted: status.clipsCompleted,
                  totalClips: status.clipsTotal,
                  currentStep: status.currentStep,
                };

                if (normalizedStatus === "completed") {
                  updates.completedAt = Date.now();
                } else if (
                  normalizedStatus === "failed" ||
                  normalizedStatus === "stale"
                ) {
                  updates.completedAt = Date.now();
                  updates.error = status.errorMessage ?? "Processing failed";
                }

                next.set(videoId, { ...existing, ...updates });
              }
              return next;
            });

            // Continue polling if still processing
            if (normalizedStatus === "processing" || normalizedStatus === "pending") {
              pollingJobsRef.current.add(videoId);
            } else {
              manager.stopTracking();
            }
          }
        } catch (err) {
          frontendLogger.error(`Failed to recover job ${videoId}:`, err);
        }
      });

    await Promise.all(recoveryPromises);
  }, [initialized, jobs, getIdToken]);

  // Run recovery check on mount
  useEffect(() => {
    if (initialized) {
      void checkAndRecoverJobs();
    }
    // Only run once on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialized]);

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
          sceneProgress: {},
        });
        return next;
      });

      // Clear any old scene progress
      clearSceneProgressFromStorage(videoId);
    },
    []
  );

  const setJobId = useCallback((videoId: string, jobId: string) => {
    setJobs((prev) => {
      const existing = prev.get(videoId);
      if (!existing) return prev;

      const next = new Map(prev);
      next.set(videoId, { ...existing, jobId });
      return next;
    });
  }, []);

  const startJobPolling = useCallback(
    (videoId: string, jobId: string, token: string) => {
      // Store job ID
      setJobId(videoId, jobId);

      // Start polling
      const manager = getProgressManager();

      // Subscribe to status updates
      manager.onStatus((status: JobProgress) => {
        setJobs((prev) => {
          const existing = prev.get(videoId);
          if (!existing) return prev;

          const next = new Map(prev);
          const normalizedStatus = normalizeJobStatus(status.status);
          const updates: Partial<ProcessingJob> = {
            status: normalizedStatus,
            progress: status.progress,
            clipsCompleted: status.clipsCompleted,
            totalClips: status.clipsTotal,
            currentStep: status.currentStep,
          };

          if (normalizedStatus === "completed") {
            updates.completedAt = Date.now();
            updates.logs = []; // Clear logs on completion
            pollingJobsRef.current.delete(videoId);
          } else if (normalizedStatus === "failed" || normalizedStatus === "stale") {
            updates.completedAt = Date.now();
            updates.error = status.errorMessage ?? "Processing failed";
            pollingJobsRef.current.delete(videoId);
          }

          next.set(videoId, { ...existing, ...updates });
          return next;
        });
      });

      pollingJobsRef.current.add(videoId);
      void manager.startTracking(jobId, token);
    },
    [setJobId]
  );

  const stopJobPolling = useCallback((videoId: string) => {
    if (pollingJobsRef.current.has(videoId)) {
      pollingJobsRef.current.delete(videoId);
      const manager = getProgressManager();
      manager.stopTracking();
    }
  }, []);

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
        logs: [],
      });
      return next;
    });

    // Stop polling if active
    if (pollingJobsRef.current.has(videoId)) {
      pollingJobsRef.current.delete(videoId);
    }
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

    // Stop polling if active
    if (pollingJobsRef.current.has(videoId)) {
      pollingJobsRef.current.delete(videoId);
    }
  }, []);

  const clearJob = useCallback((videoId: string) => {
    setJobs((prev) => {
      const next = new Map(prev);
      next.delete(videoId);
      return next;
    });

    // Clear scene progress
    clearSceneProgressFromStorage(videoId);

    // Stop polling if active
    if (pollingJobsRef.current.has(videoId)) {
      pollingJobsRef.current.delete(videoId);
    }
  }, []);

  const clearAllCompleted = useCallback(() => {
    setJobs((prev) => {
      const next = new Map(prev);
      for (const [videoId, job] of next) {
        if (job.status === "completed" || job.status === "failed") {
          next.delete(videoId);
          clearSceneProgressFromStorage(videoId);
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

  const initSceneProgress = useCallback((videoId: string, scene: SceneProgressData) => {
    setJobs((prev) => {
      const existing = prev.get(videoId);
      if (!existing) return prev;

      const next = new Map(prev);
      const sceneProgress = { ...(existing.sceneProgress ?? {}) };
      // Safe: sceneId is a validated number from our own data structures
      if (!Number.isSafeInteger(scene.sceneId) || scene.sceneId < 0) return prev;
      const key = String(scene.sceneId);
      Reflect.set(sceneProgress as Record<string, SceneProgressData>, key, scene);

      next.set(videoId, { ...existing, sceneProgress });
      return next;
    });
  }, []);

  const updateSceneProgress = useCallback(
    (videoId: string, sceneId: number, updates: Partial<SceneProgressData>) => {
      setJobs((prev) => {
        if (!Number.isSafeInteger(sceneId) || sceneId < 0) return prev;
        const existing = prev.get(videoId);
        const existingSceneProgress = existing?.sceneProgress;
        if (!existingSceneProgress) return prev;
        const key = String(sceneId);
        if (!Object.prototype.hasOwnProperty.call(existingSceneProgress, key))
          return prev;
        const currentScene = Reflect.get(
          existingSceneProgress as Record<string, SceneProgressData>,
          key
        ) as SceneProgressData | undefined;
        if (!currentScene) return prev;

        const next = new Map(prev);
        const sceneProgress = { ...existing.sceneProgress };
        // Safe: sceneId is a validated number from our own data structures
        Reflect.set(sceneProgress as Record<string, SceneProgressData>, key, {
          ...currentScene,
          ...updates,
        });

        next.set(videoId, { ...existing, sceneProgress });
        return next;
      });
    },
    []
  );

  const getSceneProgress = useCallback(
    (videoId: string) => {
      return jobs.get(videoId)?.sceneProgress;
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
        setJobId,
        startJobPolling,
        stopJobPolling,
        updateSceneProgress,
        initSceneProgress,
        getSceneProgress,
        checkAndRecoverJobs,
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
