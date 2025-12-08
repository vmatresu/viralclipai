"use client";

import {
  AlertCircle,
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  Copy,
  Play,
  Sparkles,
} from "lucide-react";
import { useParams, useRouter } from "next/navigation";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { type Clip } from "@/components/ClipGrid";
import { SceneCard, type Highlight } from "@/components/HistoryDetail/SceneCard";
import { StyleSelector } from "@/components/HistoryDetail/StyleSelector";
import { Results } from "@/components/ProcessingClient/Results";
import { DetailedProcessingStatus } from "@/components/shared/DetailedProcessingStatus";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useReprocessing } from "@/hooks/useReprocessing";
import { apiFetch, getVideoDetails, getVideoHighlights } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { useProcessing } from "@/lib/processing-context";

interface UserSettings {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
  role?: string;
  settings: Record<string, unknown>;
}

interface HighlightsData {
  video_id: string;
  video_url?: string;
  video_title?: string;
  highlights: Highlight[];
}

export default function HistoryDetailPage() {
  const params = useParams();
  const router = useRouter();
  const videoId = params.id as string;
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [highlightsData, setHighlightsData] = useState<HighlightsData | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [customPrompt, setCustomPrompt] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedScenes, setSelectedScenes] = useState<Set<number>>(new Set());
  const [selectedStyles, setSelectedStyles] = useState<Set<string>>(new Set());
  const [isProcessing, setIsProcessing] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [userSettings, setUserSettings] = useState<UserSettings | null>(null);
  const {
    getJob,
    completeJob: contextCompleteJob,
    failJob: contextFailJob,
  } = useProcessing();

  const {
    isProcessing: isReprocessing,
    reprocess,
    progress: reprocessProgress,
    logs: reprocessLogs,
    sceneProgress: reprocessSceneProgress,
  } = useReprocessing({
    videoId,
    videoTitle: highlightsData?.video_title,
    onComplete: () => {
      setIsProcessing(false);
      void loadData();
    },
    onError: () => {
      setIsProcessing(false);
    },
  });

  // Get job from global context for resuming/monitoring
  const contextJob = getJob(videoId);
  const effectiveProgress = isReprocessing
    ? reprocessProgress
    : (contextJob?.progress ?? 0);
  const effectiveLogs = isReprocessing ? reprocessLogs : (contextJob?.logs ?? []);

  const loadData = useCallback(async () => {
    if (authLoading || !user) {
      setLoading(false);
      return;
    }

    try {
      const token = await getIdToken();
      if (!token) {
        throw new Error("Failed to get authentication token");
      }

      // Load highlights and video details (including clips) in parallel
      const [highlights, details] = await Promise.all([
        getVideoHighlights(videoId, token).catch(() => null),
        getVideoDetails(videoId, token).catch(() => null),
      ]);

      if (!highlights) {
        throw new Error("Failed to load highlights");
      }

      setHighlightsData(highlights);
      setClips(details?.clips ?? []);
      setCustomPrompt(details?.custom_prompt ?? null);

      // If highlights doesn't have title/url but details does, use details
      if ((!highlights.video_title || !highlights.video_url) && details) {
        setHighlightsData((prev) =>
          prev
            ? {
                ...prev,
                video_title: prev.video_title || details.video_title,
                video_url: prev.video_url || details.video_url,
              }
            : null
        );
      }

      setError(null);
    } catch (err: unknown) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to load video data";
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  }, [getIdToken, user, authLoading, videoId]);

  const loadUserSettings = useCallback(async () => {
    if (authLoading || !user) {
      return;
    }

    try {
      const token = await getIdToken();
      if (!token) {
        return;
      }
      const settings = await apiFetch<UserSettings>("/api/settings", { token });
      setUserSettings(settings);
    } catch (err) {
      console.error("Failed to load user settings:", err);
    }
  }, [getIdToken, user, authLoading]);

  useEffect(() => {
    void loadData();
    void loadUserSettings();
  }, [loadData, loadUserSettings]);

  // Check if video is processing with proper cleanup
  useEffect(() => {
    if (!user || !videoId) {
      return undefined;
    }

    let cancelled = false;

    const checkStatus = async () => {
      if (cancelled) return;

      try {
        const token = await getIdToken();
        if (!token || cancelled) return;

        const data = await apiFetch<{
          videos: Array<{ video_id?: string; id?: string; status?: string }>;
        }>("/api/user/videos", { token });

        if (cancelled) return;

        const video = data.videos.find((v) => (v.video_id ?? v.id) === videoId);

        // Set processing status based on API
        // We trust the API status. If it says processing, we show the status window.
        // This allows monitoring to persist across refreshes for both initial processing and reprocessing.
        const statusIsProcessing = video?.status === "processing";

        // Auto-refresh data when processing completes
        if (isProcessing && !statusIsProcessing && video?.status === "completed") {
          void loadData();
        }

        // Sync processing context with API status
        // If API says completed/failed but context still says processing, update context
        // This handles the case where WebSocket disconnects before 'done' message is received
        const job = contextJob;
        if (job && (job.status === "pending" || job.status === "processing")) {
          if (video?.status === "completed") {
            console.log("API shows completed, syncing processing context");
            contextCompleteJob(videoId);
          } else if (video?.status === "failed") {
            console.log("API shows failed, syncing processing context");
            contextFailJob(videoId, "Processing failed");
          }
        }

        setIsProcessing(statusIsProcessing);
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to check video status:", err);
          // On error, assume not processing to avoid false positives
          setIsProcessing(false);
        }
      }
    };

    void checkStatus();
    const interval = setInterval(checkStatus, 5000);

    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [
    user,
    videoId,
    getIdToken,
    isProcessing,
    loadData,
    contextJob,
    contextCompleteJob,
    contextFailJob,
  ]);

  const handleSceneToggle = useCallback((sceneId: number) => {
    setSelectedScenes((prev) => {
      const next = new Set(prev);
      if (next.has(sceneId)) {
        next.delete(sceneId);
      } else {
        next.add(sceneId);
      }
      return next;
    });
  }, []);

  const handleStyleToggle = useCallback((style: string) => {
    const ALL_STYLES = [
      "split",
      "left_focus",
      "right_focus",
      "intelligent",
      "intelligent_split",
      "original",
    ];

    setSelectedStyles((prev) => {
      const next = new Set(prev);
      if (style === "all") {
        // "All Styles" is a special case - toggle all available styles
        if (ALL_STYLES.every((s) => next.has(s))) {
          // If all are selected, deselect all
          ALL_STYLES.forEach((s) => next.delete(s));
        } else {
          // Otherwise, select all
          ALL_STYLES.forEach((s) => next.add(s));
        }
      } else {
        // Toggle individual style
        if (next.has(style)) {
          next.delete(style);
        } else {
          next.add(style);
        }
      }
      return next;
    });
  }, []);

  const handleCopyUrl = useCallback(async () => {
    if (highlightsData?.video_url) {
      try {
        await navigator.clipboard.writeText(highlightsData.video_url);
        toast.success("URL copied to clipboard");
      } catch (_err) {
        toast.error("Failed to copy URL");
      }
    }
  }, [highlightsData?.video_url]);

  const handleReprocess = useCallback(async () => {
    if (selectedScenes.size === 0 || selectedStyles.size === 0) {
      toast.error("Please select at least one scene and one style");
      return;
    }

    if (isProcessing || isReprocessing) {
      toast.error("Video is currently processing. Please wait for it to complete.");
      return;
    }

    setIsProcessing(true);
    await reprocess(Array.from(selectedScenes), Array.from(selectedStyles));
  }, [selectedScenes, selectedStyles, isProcessing, isReprocessing, reprocess]);

  const formatTime = useCallback((timeStr: string): string => {
    // Handle HH:MM:SS format
    const parts = timeStr.split(":");
    if (parts.length === 3) {
      const [h, m, s] = parts;
      const totalSeconds =
        parseInt(h ?? "0") * 3600 + parseInt(m ?? "0") * 60 + parseFloat(s ?? "0");
      const minutes = Math.floor(totalSeconds / 60);
      const seconds = Math.floor(totalSeconds % 60);
      return `${minutes}:${seconds.toString().padStart(2, "0")}`;
    }
    return timeStr;
  }, []);

  const totalClipsToGenerate = useMemo(() => {
    return selectedScenes.size * selectedStyles.size;
  }, [selectedScenes.size, selectedStyles.size]);

  const canReprocess = useMemo(() => {
    return (
      selectedScenes.size > 0 &&
      selectedStyles.size > 0 &&
      !isProcessing &&
      !isReprocessing
    );
  }, [selectedScenes.size, selectedStyles.size, isProcessing, isReprocessing]);

  const hasProOrStudioPlan = useMemo(() => {
    return userSettings?.plan === "pro" || userSettings?.plan === "studio";
  }, [userSettings?.plan]);

  const log = useCallback((msg: string, type?: "info" | "error" | "success") => {
    if (type === "error") {
      toast.error(msg);
    } else if (type === "success") {
      toast.success(msg);
    } else {
      console.log(msg);
    }
  }, []);

  const handleClipDeleted = useCallback(
    async (clipName: string) => {
      try {
        const token = await getIdToken();
        if (!token) return;
        const details = await getVideoDetails(videoId, token);
        setClips(details.clips ?? []);
      } catch (err) {
        console.error("Failed to reload clips:", err);
        // Optimistic update
        setClips((prev) => prev.filter((c) => c.name !== clipName));
      }
    },
    [getIdToken, videoId]
  );

  if (authLoading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
        <p className="text-muted-foreground">Checking authentication...</p>
      </div>
    );
  }

  if (!user) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-6 text-center">
        <div className="bg-muted/30 p-4 rounded-full">
          <AlertCircle className="h-12 w-12 text-muted-foreground" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">Sign in required</h2>
          <p className="text-muted-foreground max-w-md">
            Please sign in to view video highlights.
          </p>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
        <p className="text-muted-foreground">Loading highlights...</p>
      </div>
    );
  }

  if (error || !highlightsData) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4 text-center">
        <AlertCircle className="h-12 w-12 text-destructive" />
        <div className="space-y-2">
          <h3 className="text-xl font-semibold">Failed to load highlights</h3>
          <p className="text-muted-foreground">{error ?? "Highlights not found"}</p>
        </div>
        <Button variant="outline" onClick={() => router.back()}>
          Go Back
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6 page-container">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" onClick={() => router.back()}>
          <ArrowLeft className="h-4 w-4" />
        </Button>
        <div className="flex-1">
          <h1 className="text-3xl font-bold tracking-tight">
            {highlightsData.video_title ?? "Video Highlights"}
          </h1>
          {highlightsData.video_url && (
            <div className="flex items-center gap-2 mt-1">
              <p className="text-sm text-muted-foreground truncate flex-1">
                {highlightsData.video_url}
              </p>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 flex-shrink-0"
                onClick={handleCopyUrl}
                title="Copy URL"
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
          )}
        </div>
      </div>

      {(isProcessing || isReprocessing) && (
        <DetailedProcessingStatus
          progress={effectiveProgress}
          logs={effectiveLogs}
          sceneProgress={isReprocessing ? reprocessSceneProgress : undefined}
          isResuming={!isReprocessing && isProcessing}
        />
      )}

      {hasProOrStudioPlan && (
        <Card className="glass">
          <CardHeader
            className="cursor-pointer hover:bg-accent/50 transition-colors"
            onClick={() => setIsCollapsed(!isCollapsed)}
          >
            <CardTitle className="flex items-center gap-2">
              <Sparkles className="h-5 w-5 text-primary" />
              Select Scenes to Reprocess
              <Button
                variant="ghost"
                size="sm"
                className="ml-auto h-6 w-6 p-0"
                onClick={(e) => {
                  e.stopPropagation();
                  setIsCollapsed(!isCollapsed);
                }}
              >
                {isCollapsed ? (
                  <ChevronRight className="h-4 w-4" />
                ) : (
                  <ChevronDown className="h-4 w-4" />
                )}
              </Button>
            </CardTitle>
            <CardDescription>
              Choose scenes and styles to generate new clips. This feature is available
              for Pro and Studio plans.
            </CardDescription>
          </CardHeader>
          {!isCollapsed && (
            <CardContent className="space-y-6">
              <StyleSelector
                selectedStyles={selectedStyles}
                disabled={isProcessing || isReprocessing}
                onStyleToggle={handleStyleToggle}
              />

              <div className="space-y-3">
                <h3 className="text-sm font-semibold">
                  Select Scenes ({selectedScenes.size} selected)
                </h3>
                <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
                  {highlightsData.highlights.map((highlight) => (
                    <SceneCard
                      key={highlight.id}
                      highlight={highlight}
                      selected={selectedScenes.has(highlight.id)}
                      disabled={isProcessing || isReprocessing}
                      onToggle={handleSceneToggle}
                      formatTime={formatTime}
                    />
                  ))}
                </div>
              </div>

              <div className="flex items-center justify-between pt-4 border-t">
                <p className="text-sm text-muted-foreground">
                  {canReprocess
                    ? `Will generate ${totalClipsToGenerate} new clip(s)`
                    : "Select scenes and styles to reprocess"}
                </p>
                <Button onClick={handleReprocess} disabled={!canReprocess} size="lg">
                  <Play className="h-4 w-4 mr-2" />
                  Reprocess Selected
                </Button>
              </div>
            </CardContent>
          )}
        </Card>
      )}

      <Results
        videoId={videoId}
        clips={clips}
        customPromptUsed={customPrompt}
        videoTitle={highlightsData.video_title ?? null}
        videoUrl={highlightsData.video_url ?? null}
        log={log}
        onReset={() => router.push("/")}
        onClipDeleted={handleClipDeleted}
        onTitleUpdated={(newTitle) => {
          setHighlightsData((prev) =>
            prev ? { ...prev, video_title: newTitle } : null
          );
        }}
      />
    </div>
  );
}
