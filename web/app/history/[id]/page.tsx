"use client";

import {
  AlertCircle,
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  Copy,
  Play,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useParams, useRouter } from "next/navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { type Clip } from "@/components/ClipGrid";
import {
  OverwriteConfirmationDialog,
  type OverwriteTarget,
} from "@/components/HistoryDetail/OverwriteConfirmationDialog";
import { SceneCard, type Highlight } from "@/components/HistoryDetail/SceneCard";
import {
  HistorySceneExplorer,
  groupClipsByScene,
  type HistoryClip,
} from "@/components/HistoryDetail/SceneExplorer";
import { DetailedProcessingStatus } from "@/components/shared/DetailedProcessingStatus";
import { StyleQualitySelector } from "@/components/style-quality/StyleQualitySelector";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useReprocessing } from "@/hooks/useReprocessing";
import {
  apiFetch,
  bulkDeleteClips,
  deleteAllClips,
  deleteClip as deleteClipApi,
  getProcessingStatuses,
  getVideoDetails,
  getVideoHighlights,
  getVideoSceneStyles,
} from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { useProcessing } from "@/lib/processing-context";
import { normalizeStyleForSelection } from "@/lib/styleTiers";

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

interface SceneStyleEntryDto {
  scene_id: number;
  scene_title?: string;
  styles: string[];
}

interface ReprocessPlan {
  sceneIds: number[];
  styles: string[];
  conflicts: OverwriteTarget[];
  fresh: OverwriteTarget[];
}

function parseClipIdentifier(clip: Clip): { sceneId: number; style: string } | null {
  const style = clip.style?.toLowerCase();
  const baseName = (clip.name || clip.title || "").replace(/\.(mp4|mov|mkv)$/i, "");
  const match = baseName.match(/^clip_\d+_(\d+)_.*_([a-z0-9_]+)$/i);

  if (match) {
    const sceneId = Number(match[1]);
    const inferredStyleSource = style ?? match[2];
    if (!inferredStyleSource) return null;
    const inferredStyle = inferredStyleSource.toLowerCase();
    if (!Number.isNaN(sceneId)) {
      return { sceneId, style: inferredStyle };
    }
  }

  // If we have an explicit style but could not parse scene id, skip (cannot map to selection)
  return null;
}

function timeToSeconds(timeStr?: string): number {
  if (!timeStr) return 0;
  const parts = timeStr.split(":").map((p) => parseFloat(p) || 0);
  if (parts.length === 3) {
    const [h = 0, m = 0, s = 0] = parts;
    return h * 3600 + m * 60 + s;
  }
  if (parts.length === 2) {
    const [m = 0, s = 0] = parts;
    return m * 60 + s;
  }
  const numeric = parseFloat(timeStr);
  return Number.isFinite(numeric) ? numeric : 0;
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-lg border bg-muted/30 px-3 py-2">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="text-lg font-semibold text-foreground">{value}</p>
    </div>
  );
}

export default function HistoryDetailPage() {
  const params = useParams();
  const router = useRouter();
  const videoId = params.id as string;
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [highlightsData, setHighlightsData] = useState<HighlightsData | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedScenes, setSelectedScenes] = useState<Set<number>>(new Set());
  const [selectedStyles, setSelectedStyles] = useState<string[]>([]);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [userSettings, setUserSettings] = useState<UserSettings | null>(null);
  const [customPrompt, setCustomPrompt] = useState<string>("");
  const [promptOpen, setPromptOpen] = useState(false);
  const [deleteAllDialogOpen, setDeleteAllDialogOpen] = useState(false);
  const [deletingAll, setDeletingAll] = useState(false);
  const [overwriteDialogOpen, setOverwriteDialogOpen] = useState(false);
  const [pendingPlan, setPendingPlan] = useState<ReprocessPlan | null>(null);
  const [sceneStylesData, setSceneStylesData] = useState<SceneStyleEntryDto[] | null>(
    null
  );
  const [overwritePromptEnabled, setOverwritePromptEnabled] = useState<boolean>(true);
  const {
    getJob,
    completeJob: contextCompleteJob,
    failJob: contextFailJob,
  } = useProcessing();
  const selectionInitializedRef = useRef(false);

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

  const sceneTitleById = useMemo(() => {
    const map = new Map<number, string>();
    highlightsData?.highlights.forEach((h) => map.set(h.id, h.title));
    sceneStylesData?.forEach((entry) => {
      if (entry.scene_title && !map.has(entry.scene_id)) {
        map.set(entry.scene_id, entry.scene_title);
      }
    });
    return map;
  }, [highlightsData?.highlights, sceneStylesData]);

  const highlightTimingById = useMemo(() => {
    const map = new Map<number, { start: number; end: number }>();
    highlightsData?.highlights.forEach((h) => {
      map.set(h.id, { start: timeToSeconds(h.start), end: timeToSeconds(h.end) });
    });
    return map;
  }, [highlightsData?.highlights]);

  const historyClips = useMemo<HistoryClip[]>(() => {
    if (!clips || clips.length === 0) return [];
    return clips.flatMap((clip) => {
      // Use backend scene_id if available (from clip metadata)
      // For backward compatibility, fall back to parsing from filename
      const parsed = parseClipIdentifier(clip);
      if (!parsed) {
        // If we can't parse scene info, skip this clip
        return [];
      }
      const timing = highlightTimingById.get(parsed.sceneId);
      return [
        {
          id: clip.clip_id,
          sceneId: parsed.sceneId,
          sceneTitle: sceneTitleById.get(parsed.sceneId),
          startSec: timing?.start ?? 0,
          endSec: timing?.end ?? 0,
          style: parsed.style,
          size: clip.size,
          clipName: clip.name,
          title: clip.title,
        },
      ];
    });
  }, [clips, highlightTimingById, sceneTitleById]);

  const sceneGroups = useMemo(
    () => groupClipsByScene(historyClips, highlightsData?.highlights),
    [historyClips, highlightsData?.highlights]
  );

  const uniqueStyleCount = useMemo(() => {
    const styles = new Set<string>();
    historyClips.forEach((clip) => styles.add(clip.style));
    return styles.size;
  }, [historyClips]);

  const existingClipIndex = useMemo(() => {
    const map = new Map<number, Set<string>>();

    if (sceneStylesData) {
      sceneStylesData.forEach((entry) => {
        const styles = map.get(entry.scene_id) ?? new Set<string>();
        entry.styles.forEach((s) => styles.add(s.toLowerCase()));
        map.set(entry.scene_id, styles);
      });
      return map;
    }

    clips.forEach((clip) => {
      const parsed = parseClipIdentifier(clip);
      if (!parsed) return;
      const styles = map.get(parsed.sceneId) ?? new Set<string>();
      styles.add(parsed.style.toLowerCase());
      map.set(parsed.sceneId, styles);
    });
    return map;
  }, [clips, sceneStylesData]);

  const buildReprocessPlan = useCallback(
    (sceneIds: number[], styles: string[]): ReprocessPlan => {
      const normalizedStyles = styles.map((s) => s.toLowerCase());
      const conflicts: OverwriteTarget[] = [];
      const fresh: OverwriteTarget[] = [];

      sceneIds.forEach((sceneId) => {
        normalizedStyles.forEach((style) => {
          const target: OverwriteTarget = {
            sceneId,
            sceneTitle: sceneTitleById.get(sceneId) ?? undefined,
            style,
          };
          const existingStyles = existingClipIndex.get(sceneId);
          if (existingStyles?.has(style)) {
            conflicts.push(target);
          } else {
            fresh.push(target);
          }
        });
      });

      return { sceneIds, styles: normalizedStyles, conflicts, fresh };
    },
    [existingClipIndex, sceneTitleById]
  );

  const startReprocess = useCallback(
    async (plan: ReprocessPlan) => {
      setIsProcessing(true);
      await reprocess(plan.sceneIds, plan.styles);
    },
    [reprocess]
  );

  const handleConfirmOverwrite = useCallback(async () => {
    if (!pendingPlan) return;
    setOverwriteDialogOpen(false);
    await startReprocess(pendingPlan);
  }, [pendingPlan, startReprocess]);

  const handleCancelOverwrite = useCallback(() => {
    setOverwriteDialogOpen(false);
    setPendingPlan(null);
  }, []);

  const handleTogglePrompt = useCallback((value: boolean) => {
    setOverwritePromptEnabled(value);
    sessionStorage.setItem("overwritePromptEnabled", value ? "true" : "false");
  }, []);

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

      // Load highlights, video details, and existing scene/style index in parallel
      const [highlights, details, sceneStyles] = await Promise.all([
        getVideoHighlights(videoId, token).catch(() => null),
        getVideoDetails(videoId, token).catch(() => null),
        getVideoSceneStyles(videoId, token).catch(() => null),
      ]);

      if (!highlights) {
        throw new Error("Failed to load highlights");
      }

      setHighlightsData(highlights);
      setClips(details?.clips ?? []);
      setSceneStylesData(sceneStyles?.scene_styles ?? null);
      setCustomPrompt(details?.custom_prompt ?? "");

      // If highlights doesn't have title/url but details does, use details
      if ((!highlights.video_title || !highlights.video_url) && details) {
        setHighlightsData((prev) =>
          prev
            ? {
                ...prev,
                video_title: prev.video_title ?? details.video_title,
                video_url: prev.video_url ?? details.video_url,
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

  // Load persisted overwrite prompt preference (session-scoped)
  useEffect(() => {
    const stored = sessionStorage.getItem("overwritePromptEnabled");
    if (stored === "false") {
      setOverwritePromptEnabled(false);
    }
  }, []);

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

        const data = await getProcessingStatuses(token, [videoId]);

        if (cancelled) return;

        const video = data.videos.find((v) => v.video_id === videoId);

        // Set processing status based on API
        // We trust the API status. If it says processing, we show the status window.
        // This allows monitoring to persist across refreshes for both initial processing and reprocessing.
        const statusIsProcessing = video?.status === "processing";

        // Auto-refresh data when processing completes (either analyzed or fully completed)
        if (
          isProcessing &&
          !statusIsProcessing &&
          (video?.status === "completed" || video?.status === "analyzed")
        ) {
          void loadData();
        }

        // Sync processing context with API status
        // If API says completed/failed/analyzed but context still says processing, update context
        // This handles the case where WebSocket disconnects before 'done' message is received
        const job = contextJob;
        if (job && (job.status === "pending" || job.status === "processing")) {
          if (video?.status === "completed" || video?.status === "analyzed") {
            contextCompleteJob(videoId);
          } else if (video?.status === "failed") {
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

  const handleStylesChange = useCallback((styles: string[]) => {
    selectionInitializedRef.current = true;
    setSelectedStyles(styles);
  }, []);

  useEffect(() => {
    if (selectionInitializedRef.current) return;

    const collected = new Set<string>();

    if (sceneStylesData && sceneStylesData.length > 0) {
      sceneStylesData.forEach((entry) => {
        entry.styles.forEach((style) => {
          const normalized = normalizeStyleForSelection(style);
          if (normalized) {
            collected.add(normalized);
          }
        });
      });
    } else if (clips.length > 0) {
      clips.forEach((clip) => {
        if (!clip.style) return;
        const normalized = normalizeStyleForSelection(clip.style);
        if (normalized) {
          collected.add(normalized);
        }
      });
    }

    if (sceneStylesData || clips.length > 0) {
      selectionInitializedRef.current = true;
      if (collected.size > 0) {
        setSelectedStyles(Array.from(collected));
      } else {
        setSelectedStyles(["intelligent_split"]);
      }
    }
  }, [sceneStylesData, clips]);

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

  const handleCopyPrompt = useCallback(async () => {
    if (!customPrompt) {
      toast.info("No custom prompt available for this video.");
      return;
    }
    try {
      await navigator.clipboard.writeText(customPrompt);
      toast.success("Custom prompt copied");
    } catch (_err) {
      toast.error("Failed to copy prompt");
    }
  }, [customPrompt]);

  const handleDeleteClip = useCallback(
    async (clip: HistoryClip) => {
      const clipName = clip.clipName ?? clip.id;
      if (!clipName) {
        toast.error("Missing clip identifier.");
        return;
      }

      try {
        const token = await getIdToken();
        if (!token) {
          toast.error("Please sign in to delete clips.");
          return;
        }
        await deleteClipApi(videoId, clipName, token);
        setClips((prev) => prev.filter((c) => c.name !== clipName));
        toast.success("Clip deleted");
      } catch (err) {
        const message = err instanceof Error ? err.message : "Failed to delete clip";
        toast.error(message);
      }
    },
    [getIdToken, videoId]
  );

  const handleDeleteScene = useCallback(
    async (sceneId: number) => {
      const clipNames = clips
        .filter((clip) => parseClipIdentifier(clip)?.sceneId === sceneId)
        .map((clip) => clip.name)
        .filter(Boolean);

      if (clipNames.length === 0) {
        toast.error("No clips found for this scene.");
        return;
      }

      try {
        const token = await getIdToken();
        if (!token) {
          toast.error("Please sign in to delete scenes.");
          return;
        }
        await bulkDeleteClips(videoId, clipNames, token);
        setClips((prev) => prev.filter((clip) => !clipNames.includes(clip.name)));
        setSelectedScenes((prev) => {
          const next = new Set(prev);
          next.delete(sceneId);
          return next;
        });
        toast.success("Scene deleted");
      } catch (err) {
        const message = err instanceof Error ? err.message : "Failed to delete scene";
        toast.error(message);
      }
    },
    [clips, getIdToken, videoId]
  );

  const handleDeleteAllScenes = useCallback(async () => {
    try {
      setDeletingAll(true);
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to delete scenes.");
        setDeletingAll(false);
        return;
      }
      await deleteAllClips(videoId, token);
      setClips([]);
      setSelectedScenes(new Set());
      toast.success("All scenes deleted");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to delete scenes";
      toast.error(message);
    } finally {
      setDeletingAll(false);
      setDeleteAllDialogOpen(false);
    }
  }, [getIdToken, videoId]);

  const handleReprocess = useCallback(async () => {
    if (selectedScenes.size === 0 || selectedStyles.length === 0) {
      toast.error("Please select at least one scene and one style");
      return;
    }

    if (isProcessing || isReprocessing) {
      toast.error("Video is currently processing. Please wait for it to complete.");
      return;
    }

    const sceneIds = Array.from(selectedScenes);
    const plan = buildReprocessPlan(sceneIds, selectedStyles);
    setPendingPlan(plan);

    if (plan.conflicts.length > 0 && overwritePromptEnabled) {
      setOverwriteDialogOpen(true);
      return;
    }

    await startReprocess(plan);
  }, [
    selectedScenes,
    selectedStyles,
    isProcessing,
    isReprocessing,
    buildReprocessPlan,
    startReprocess,
    overwritePromptEnabled,
  ]);

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
    return selectedScenes.size * selectedStyles.length;
  }, [selectedScenes.size, selectedStyles.length]);

  const canReprocess = useMemo(() => {
    return (
      selectedScenes.size > 0 &&
      selectedStyles.length > 0 &&
      !isProcessing &&
      !isReprocessing
    );
  }, [selectedScenes.size, selectedStyles.length, isProcessing, isReprocessing]);

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
          <p className="text-sm text-muted-foreground">History · {videoId}</p>
          <h1 className="text-3xl font-bold tracking-tight">
            {highlightsData.video_title ?? "Video Highlights"}
          </h1>
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

      <Card className="glass">
        <CardHeader
          className="cursor-pointer hover:bg-accent/50 transition-colors"
          onClick={() => setIsCollapsed(!isCollapsed)}
        >
          <CardTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            Select Scenes to Process
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
            Choose scenes and styles to generate new clips. Smart Face requires Pro,
            Active Speaker requires Studio.
          </CardDescription>
        </CardHeader>
        {!isCollapsed && (
          <CardContent className="space-y-6">
            <StyleQualitySelector
              selectedStyles={selectedStyles}
              onChange={handleStylesChange}
              disabled={isProcessing || isReprocessing}
              userPlan={userSettings?.plan}
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
                Process Selected Scenes
              </Button>
            </div>
          </CardContent>
        )}
      </Card>

      <Card className="shadow-sm">
        <CardHeader className="pb-4 space-y-3">
          <div className="flex items-start gap-3">
            <div className="flex-1 space-y-1">
              <CardTitle className="flex items-center gap-2 text-xl">
                <Sparkles className="h-5 w-5 text-primary" />
                Scene-centric explorer
              </CardTitle>
              <CardDescription>
                Browse generated clips by scene, switch between styles, and curate or
                delete them.
              </CardDescription>
              <p className="text-xs text-muted-foreground">Video ID · {videoId}</p>
            </div>
            <Dialog open={deleteAllDialogOpen} onOpenChange={setDeleteAllDialogOpen}>
              <DialogTrigger asChild>
                <Button
                  variant="destructive"
                  size="sm"
                  disabled={sceneGroups.length === 0}
                  className="shrink-0"
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete all scenes
                </Button>
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Delete all scenes?</DialogTitle>
                  <DialogDescription>
                    This will permanently delete all scenes and clips for this video.
                    This cannot be undone.
                  </DialogDescription>
                </DialogHeader>
                <DialogFooter className="gap-2 sm:justify-end">
                  <DialogClose asChild>
                    <Button variant="outline" disabled={deletingAll}>
                      Cancel
                    </Button>
                  </DialogClose>
                  <Button
                    variant="destructive"
                    onClick={handleDeleteAllScenes}
                    disabled={deletingAll}
                  >
                    {deletingAll ? "Deleting..." : "Delete all"}
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </div>

          {highlightsData.video_url && (
            <div className="flex flex-wrap items-center gap-2 text-sm">
              <Badge variant="outline">Source</Badge>
              <p className="truncate text-muted-foreground flex-1 min-w-0">
                {highlightsData.video_url}
              </p>
              <Button
                variant="outline"
                size="sm"
                className="gap-2"
                onClick={handleCopyUrl}
              >
                <Copy className="h-4 w-4" />
                Copy link
              </Button>
            </div>
          )}
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-4">
            <Stat
              label="Scenes"
              value={sceneGroups.length || highlightsData.highlights.length}
            />
            <Stat label="Total clips" value={historyClips.length} />
            <Stat label="Unique styles" value={uniqueStyleCount} />
            <Stat
              label="Processing status"
              value={isProcessing || isReprocessing ? "Processing" : "Idle"}
            />
          </div>

          <div className="rounded-lg border bg-card shadow-sm">
            <button
              type="button"
              onClick={() => setPromptOpen((prev) => !prev)}
              className="flex w-full items-center justify-between gap-2 px-4 py-3 text-left hover:bg-muted/50"
            >
              <div className="space-y-1">
                <p className="text-sm font-semibold">
                  Custom prompt used for this video
                </p>
                <p className="text-xs text-muted-foreground line-clamp-1">
                  {customPrompt ? customPrompt : "No custom prompt was provided."}
                </p>
              </div>
              {promptOpen ? (
                <ChevronDown className="h-4 w-4" />
              ) : (
                <ChevronRight className="h-4 w-4" />
              )}
            </button>
            {promptOpen && (
              <div className="border-t">
                <ScrollArea className="max-h-48 px-4 py-3">
                  <p className="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">
                    {customPrompt || "No custom prompt was provided for this video."}
                  </p>
                </ScrollArea>
                <div className="flex justify-end gap-2 border-t bg-muted/40 px-4 py-3">
                  <Button
                    variant="outline"
                    size="sm"
                    className="gap-2"
                    onClick={handleCopyPrompt}
                    disabled={!customPrompt}
                  >
                    <Copy className="h-4 w-4" />
                    Copy prompt
                  </Button>
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <Card className="shadow-sm">
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            Scenes & styles
          </CardTitle>
          <CardDescription>
            Clips are grouped by scene. Switch styles, play inline, or delete clips and
            scenes.
          </CardDescription>
        </CardHeader>
        <CardContent className="px-4 py-4">
          <HistorySceneExplorer
            scenes={sceneGroups}
            onDeleteClip={handleDeleteClip}
            onDeleteScene={handleDeleteScene}
          />
        </CardContent>
      </Card>

      <OverwriteConfirmationDialog
        open={overwriteDialogOpen}
        conflicts={pendingPlan?.conflicts ?? []}
        fresh={pendingPlan?.fresh ?? []}
        onCancel={handleCancelOverwrite}
        onConfirm={handleConfirmOverwrite}
        promptEnabled={overwritePromptEnabled}
        onTogglePrompt={handleTogglePrompt}
      />
    </div>
  );
}
