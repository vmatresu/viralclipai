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
import {
  DEFAULT_STREAMER_SPLIT_CONFIG,
  StyleQualitySelector,
  type StreamerSplitConfig,
} from "@/components/style-quality/StyleQualitySelector";
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
import { calculateProgressPercentage, useVideoStatus } from "@/hooks/useVideoStatus";
import {
  apiFetch,
  bulkDeleteClips,
  deleteAllClips,
  deleteClip as deleteClipApi,
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
  settings: {
    cut_silent_parts_default?: boolean;
    [key: string]: unknown;
  };
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

  // Check for compilation clips first (e.g., "top_5_scenes_title_streamer_top_scenes")
  const compilationMatch = baseName.match(/^top_(\d+)_scenes_.*_([a-z0-9_]+)$/i);
  if (compilationMatch) {
    const inferredStyleSource = style ?? compilationMatch[2];
    if (!inferredStyleSource) return null;
    return { sceneId: 0, style: inferredStyleSource.toLowerCase() };
  }

  // Check for regular clips (e.g., "clip_01_1_title_style")
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
  const [enableObjectDetection, setEnableObjectDetection] = useState<boolean>(false);
  const [streamerSplitConfig, setStreamerSplitConfig] = useState<StreamerSplitConfig>(
    DEFAULT_STREAMER_SPLIT_CONFIG
  );
  /** Track whether Top Scenes compilation mode is enabled */
  const [topScenesEnabled, setTopScenesEnabled] = useState<boolean>(false);
  /** Track whether to cut silent parts from clips */
  const [cutSilentParts, setCutSilentParts] = useState<boolean>(false);
  /** Track the order of selected scenes for Top Scenes compilation */
  const [compilationSceneOrder, setCompilationSceneOrder] = useState<number[]>([]);
  const { startProcessing } = useProcessing();

  // Fetch video status from Firebase (with caching)
  const { status: videoStatus, refresh: refreshStatus } = useVideoStatus(videoId);
  const selectionInitializedRef = useRef(false);

  // Get progress from Firebase status
  const effectiveProgress = videoStatus?.processing_progress
    ? calculateProgressPercentage(videoStatus.processing_progress)
    : 0;

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
    async (plan: ReprocessPlan, overwrite: boolean = false) => {
      setIsProcessing(true);

      try {
        const token = await getIdToken();
        if (!token) {
          toast.error("Please sign in to reprocess scenes.");
          setIsProcessing(false);
          return;
        }

        // Pass enableObjectDetection only if Cinematic style is selected
        const hasCinematic = plan.styles.some((s) =>
          s.toLowerCase().includes("cinematic")
        );
        // Pass StreamerSplit params only if streamer_split style is selected
        const hasStreamerSplit = plan.styles.some(
          (s) => s.toLowerCase() === "streamer_split"
        );
        const streamerParams = hasStreamerSplit
          ? {
              position_x: streamerSplitConfig.positionX,
              position_y: streamerSplitConfig.positionY,
              zoom: streamerSplitConfig.zoom,
            }
          : undefined;
        // Check if this is a Top Scenes compilation (streamer_top_scenes style)
        const isTopScenesCompilation = plan.styles.some(
          (s) => s.toLowerCase() === "streamer_top_scenes"
        );

        // For Top Scenes compilation, use the ordered scene IDs from compilationSceneOrder
        const sceneIdsToProcess = isTopScenesCompilation
          ? compilationSceneOrder.filter((id) => plan.sceneIds.includes(id))
          : plan.sceneIds;

        // Submit reprocess job via REST API
        const { reprocessScenes } = await import("@/lib/apiClient");
        const result = await reprocessScenes(
          videoId,
          {
            scene_ids: sceneIdsToProcess,
            styles: plan.styles,
            overwrite,
            enable_object_detection: hasCinematic && enableObjectDetection,
            top_scenes_compilation: isTopScenesCompilation,
            cut_silent_parts: cutSilentParts,
            streamer_split_params: streamerParams,
          },
          token
        );

        // Mark video as processing in context
        startProcessing(videoId);

        toast.success(
          `Processing started! ${result.total_clips} clips will be generated. Refresh the page to check progress.`
        );

        // Refresh status after a short delay
        setTimeout(() => {
          void refreshStatus(true);
        }, 2000);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : "Failed to start reprocessing";
        toast.error(message);
        setIsProcessing(false);
      }
    },
    [
      getIdToken,
      videoId,
      enableObjectDetection,
      streamerSplitConfig,
      compilationSceneOrder,
      cutSilentParts,
      startProcessing,
      refreshStatus,
    ]
  );

  const handleConfirmOverwrite = useCallback(async () => {
    if (!pendingPlan) return;
    setOverwriteDialogOpen(false);
    // User confirmed overwrite - pass overwrite: true
    await startReprocess(pendingPlan, true);
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

  // Initialize cutSilentParts from user settings when loaded
  useEffect(() => {
    if (userSettings?.settings?.cut_silent_parts_default !== undefined) {
      setCutSilentParts(userSettings.settings.cut_silent_parts_default);
    }
  }, [userSettings]);

  // Load persisted overwrite prompt preference (session-scoped)
  useEffect(() => {
    const stored = sessionStorage.getItem("overwritePromptEnabled");
    if (stored === "false") {
      setOverwritePromptEnabled(false);
    }
  }, []);

  // Sync processing state with Firebase status
  useEffect(() => {
    if (videoStatus) {
      const statusIsProcessing = videoStatus.status === "processing";

      // If processing just finished, reload data
      if (isProcessing && !statusIsProcessing) {
        void loadData();
      }

      setIsProcessing(statusIsProcessing);
    }
  }, [videoStatus, isProcessing, loadData]);

  const handleSceneToggle = useCallback(
    (sceneId: number) => {
      setSelectedScenes((prev) => {
        const next = new Set(prev);
        if (next.has(sceneId)) {
          next.delete(sceneId);
        } else {
          // Enforce max 10 scenes for Top Scenes compilation
          if (topScenesEnabled && next.size >= 10) {
            return prev; // Don't add more scenes
          }
          next.add(sceneId);
        }
        return next;
      });

      // Also update compilation scene order (maintained separately for ordering)
      setCompilationSceneOrder((prev) => {
        if (prev.includes(sceneId)) {
          // Remove scene from order
          return prev.filter((id) => id !== sceneId);
        } else {
          // Enforce max 10 scenes for Top Scenes compilation
          if (topScenesEnabled && prev.length >= 10) {
            return prev; // Don't add more scenes
          }
          // Add scene to end of order
          return [...prev, sceneId];
        }
      });
    },
    [topScenesEnabled]
  );

  const handleStylesChange = useCallback((styles: string[]) => {
    selectionInitializedRef.current = true;
    setSelectedStyles(styles);
  }, []);

  /** Handle Top Scenes enabled state change from StyleQualitySelector */
  const handleTopScenesEnabledChange = useCallback(
    (enabled: boolean) => {
      setTopScenesEnabled(enabled);
      // If disabling Top Scenes, we keep the compilationSceneOrder for potential re-enabling
      // If enabling and there are selected scenes not in order, sync them
      if (enabled) {
        setCompilationSceneOrder((prev) => {
          // Add any selected scenes not already in the order
          const newOrder = [...prev];
          selectedScenes.forEach((sceneId) => {
            if (!newOrder.includes(sceneId)) {
              newOrder.push(sceneId);
            }
          });
          // Keep only scenes that are still selected
          return newOrder.filter((id) => selectedScenes.has(id));
        });
      }
    },
    [selectedScenes]
  );

  /** Handle removing a scene from the Top Scenes compilation order */
  const handleRemoveCompilationScene = useCallback((sceneId: number) => {
    // Remove from compilation order
    setCompilationSceneOrder((prev) => prev.filter((id) => id !== sceneId));
    // Also deselect the scene
    setSelectedScenes((prev) => {
      const next = new Set(prev);
      next.delete(sceneId);
      return next;
    });
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
      // Start with empty selection if no existing styles are found
      // User can select any combination of Split, Full, and Original
      setSelectedStyles(Array.from(collected));
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
      // Clear all clip-related state to prevent stale overwrite detection
      setClips([]);
      setSceneStylesData(null);
      setSelectedScenes(new Set());
      setCompilationSceneOrder([]);
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

    if (isProcessing) {
      toast.error("Video is currently processing. Please wait for it to complete.");
      return;
    }

    const sceneIds = Array.from(selectedScenes);

    // Check if this is a Top Scenes compilation
    const isTopScenesCompilation = selectedStyles.some(
      (s) => s.toLowerCase() === "streamer_top_scenes"
    );

    // For Top Scenes compilation, skip the per-scene overwrite check entirely
    // because it creates ONE compilation clip (scene_id: 0), not individual scene clips
    if (isTopScenesCompilation) {
      const plan: ReprocessPlan = {
        sceneIds,
        styles: selectedStyles.map((s) => s.toLowerCase()),
        conflicts: [], // No per-scene conflicts for compilation
        fresh: [
          {
            sceneId: 0,
            sceneTitle: `Top ${sceneIds.length} Scenes`,
            style: "streamer_top_scenes",
          },
        ],
      };
      setPendingPlan(plan);
      await startReprocess(plan);
      return;
    }

    // For regular styles, check for overwrite conflicts
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
    return selectedScenes.size > 0 && selectedStyles.length > 0 && !isProcessing;
  }, [selectedScenes.size, selectedStyles.length, isProcessing]);

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

      {isProcessing && (
        <DetailedProcessingStatus
          progress={effectiveProgress}
          logs={[]}
          sceneProgress={undefined}
          isResuming={false}
        />
      )}

      <Card className="glass">
        <CardHeader
          className="cursor-pointer hover:bg-accent/10 transition-colors"
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
              disabled={isProcessing}
              userPlan={userSettings?.plan}
              enableObjectDetection={enableObjectDetection}
              onEnableObjectDetectionChange={setEnableObjectDetection}
              streamerSplitConfig={streamerSplitConfig}
              onStreamerSplitConfigChange={setStreamerSplitConfig}
              onTopScenesEnabledChange={handleTopScenesEnabledChange}
              compilationScenes={compilationSceneOrder}
              sceneTitles={sceneTitleById}
              onRemoveCompilationScene={handleRemoveCompilationScene}
              cutSilentParts={cutSilentParts}
              onCutSilentPartsChange={setCutSilentParts}
            />

            <div className="space-y-3">
              <h3 className="text-sm font-semibold">
                Select Scenes ({selectedScenes.size} selected)
              </h3>
              <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
                {[...highlightsData.highlights]
                  .sort((a, b) => a.id - b.id)
                  .map((highlight, index) => (
                    <SceneCard
                      key={highlight.id}
                      highlight={highlight}
                      selected={selectedScenes.has(highlight.id)}
                      disabled={isProcessing}
                      onToggle={handleSceneToggle}
                      formatTime={formatTime}
                      sceneNumber={index + 1}
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
              value={isProcessing ? "Processing" : "Idle"}
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
            videoId={videoId}
            onDeleteClip={handleDeleteClip}
            onDeleteScene={handleDeleteScene}
            onClipTitleUpdated={() => {
              // Refresh clips to show updated title
              void loadData();
            }}
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
