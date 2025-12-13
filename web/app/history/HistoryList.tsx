"use client";

import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  Check,
  CheckSquare,
  Clock,
  Copy,
  Film,
  MoreHorizontal,
  Square,
  Trash2,
  TrendingUp,
  Zap,
} from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { EditableTitle } from "@/components/EditableTitle";
import { SignInDialog } from "@/components/SignInDialog";
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
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { VideoStatusBadge } from "@/components/VideoStatusBadge";
import { useVideoPolling } from "@/hooks/useVideoPolling";
import {
  apiFetch,
  bulkDeleteVideos,
  deleteVideo,
  updateVideoTitle,
} from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { invalidateClipsCache, invalidateClipsCacheMany } from "@/lib/cache";
import { usePageView } from "@/lib/usePageView";
import { cn } from "@/lib/utils";

type SortField = "title" | "status" | "size" | "date";
type SortDirection = "asc" | "desc";

interface UserVideo {
  id?: string;
  video_id?: string;
  video_title?: string;
  video_url?: string;
  created_at?: string;
  custom_prompt?: string;
  status?: "processing" | "analyzed" | "completed" | "failed";
  clips_count?: number;
  /** Total size of all clips in bytes. */
  total_size_bytes?: number;
  /** Human-readable total size. */
  total_size_formatted?: string;
}

interface StorageInfo {
  used_bytes: number;
  limit_bytes: number;
  total_clips: number;
  percentage: number;
  used_formatted: string;
  limit_formatted: string;
  remaining_formatted: string;
}

interface PlanUsage {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
  storage?: StorageInfo;
}

// Helper to parse size string to bytes for sorting
function parseSizeToBytes(sizeStr?: string): number {
  if (!sizeStr) return 0;
  const match = sizeStr.match(/^([\d.]+)\s*(B|KB|MB|GB|TB)?$/i);
  if (!match) return 0;
  const value = parseFloat(match[1] ?? "0");
  const unit = (match[2] ?? "B").toUpperCase();
  const multipliers: Record<string, number> = {
    B: 1,
    KB: 1024,
    MB: 1024 * 1024,
    GB: 1024 * 1024 * 1024,
    TB: 1024 * 1024 * 1024 * 1024,
  };
  return value * (multipliers[unit] ?? 1);
}

export default function HistoryList() {
  usePageView("history");
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [videos, setVideos] = useState<UserVideo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedVideos, setSelectedVideos] = useState<Set<string>>(new Set());
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{
    type: "single" | "bulk" | "all";
    videoId?: string;
  } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);
  const [planUsage, setPlanUsage] = useState<PlanUsage | null>(null);
  const [loadingUsage, setLoadingUsage] = useState(true);
  const [sortField, setSortField] = useState<SortField>("date");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");
  const router = useRouter();

  // Sort videos based on current sort field and direction
  const sortedVideos = useMemo(() => {
    return [...videos].sort((a, b) => {
      let comparison = 0;

      switch (sortField) {
        case "title": {
          const titleA = (a.video_title ?? "").toLowerCase();
          const titleB = (b.video_title ?? "").toLowerCase();
          comparison = titleA.localeCompare(titleB);
          break;
        }
        case "status": {
          const statusOrder = { processing: 0, analyzed: 1, completed: 2, failed: 3 };
          const statusA = statusOrder[a.status ?? "completed"] ?? 4;
          const statusB = statusOrder[b.status ?? "completed"] ?? 4;
          comparison = statusA - statusB;
          break;
        }
        case "size": {
          const sizeA = a.total_size_bytes ?? parseSizeToBytes(a.total_size_formatted);
          const sizeB = b.total_size_bytes ?? parseSizeToBytes(b.total_size_formatted);
          comparison = sizeA - sizeB;
          break;
        }
        case "date":
        default: {
          const dateA = a.created_at ? new Date(a.created_at).getTime() : 0;
          const dateB = b.created_at ? new Date(b.created_at).getTime() : 0;
          comparison = dateA - dateB;
          break;
        }
      }

      return sortDirection === "asc" ? comparison : -comparison;
    });
  }, [videos, sortField, sortDirection]);

  const handleSort = useCallback((field: SortField) => {
    setSortField((currentField) => {
      if (currentField === field) {
        setSortDirection((dir) => (dir === "asc" ? "desc" : "asc"));
        return field;
      }
      setSortDirection("desc");
      return field;
    });
  }, []);

  const SortableHeader = useCallback(
    ({ field, children }: { field: SortField; children: React.ReactNode }) => {
      const isActive = sortField === field;
      return (
        <button
          onClick={() => handleSort(field)}
          className={cn(
            "flex items-center gap-1 hover:text-foreground transition-colors",
            isActive ? "text-foreground" : "text-muted-foreground"
          )}
        >
          {children}
          {isActive ? (
            sortDirection === "asc" ? (
              <ArrowUp className="h-3.5 w-3.5" />
            ) : (
              <ArrowDown className="h-3.5 w-3.5" />
            )
          ) : (
            <ArrowUpDown className="h-3.5 w-3.5 opacity-50" />
          )}
        </button>
      );
    },
    [sortField, sortDirection, handleSort]
  );

  const loadVideos = useCallback(async () => {
    if (authLoading || !user) {
      setLoading(false);
      return;
    }

    try {
      const token = await getIdToken();
      if (!token) {
        throw new Error("Failed to get authentication token");
      }
      const data = (await apiFetch<{ videos: UserVideo[] }>("/api/user/videos", {
        token,
      })) as { videos: UserVideo[] };
      setVideos(data.videos ?? []);
      setError(null);
      setSelectedVideos(new Set()); // Clear selections when reloading
    } catch (err: unknown) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to load history";
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  }, [getIdToken, user, authLoading]);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      if (authLoading) return;

      if (!user) {
        // Not logged in - stop loading but don't error yet (let UI handle it)
        if (!cancelled) {
          setLoading(false);
          setLoadingUsage(false);
        }
        return;
      }

      try {
        const token = await getIdToken();
        if (!token) {
          throw new Error("Failed to get authentication token");
        }

        // Load videos and plan usage in parallel
        const [videosData, usageData] = await Promise.all([
          apiFetch<{ videos: UserVideo[] }>("/api/user/videos", { token }),
          apiFetch<PlanUsage>("/api/settings", { token }),
        ]);

        if (!cancelled) {
          setVideos((videosData as { videos: UserVideo[] }).videos ?? []);
          setPlanUsage(usageData);
          setError(null);
          setSelectedVideos(new Set()); // Clear selections when reloading
        }
      } catch (err: unknown) {
        if (!cancelled) {
          const errorMessage =
            err instanceof Error ? err.message : "Failed to load history";
          setError(errorMessage);
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
          setLoadingUsage(false);
        }
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [getIdToken, user, authLoading]);

  // Poll for processing videos using custom hook
  useVideoPolling({
    videos,
    enabled: !authLoading && Boolean(user),
    getIdToken,
    onVideosUpdate: setVideos,
    pollInterval: 5000,
    maxInterval: 30000,
  });

  const handleSelectVideo = (videoId: string) => {
    setSelectedVideos((prev) => {
      const next = new Set(prev);
      if (next.has(videoId)) {
        next.delete(videoId);
      } else {
        next.add(videoId);
      }
      return next;
    });
  };

  const handleSelectAll = () => {
    if (selectedVideos.size === videos.length) {
      setSelectedVideos(new Set());
    } else {
      setSelectedVideos(
        new Set(videos.map((v) => v.video_id ?? v.id ?? "").filter(Boolean))
      );
    }
  };

  const handleDeleteClick = (type: "single" | "bulk" | "all", videoId?: string) => {
    setDeleteTarget({ type, videoId });
    setDeleteDialogOpen(true);
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTarget || !user) return;

    setDeleting(true);
    try {
      const token = await getIdToken();
      if (!token) {
        throw new Error("Failed to get authentication token");
      }

      if (deleteTarget.type === "single" && deleteTarget.videoId) {
        await deleteVideo(deleteTarget.videoId, token);
        // Invalidate cache for deleted video (fire and forget)
        void invalidateClipsCache(deleteTarget.videoId);
        // Remove from local state immediately for better UX
        setVideos((prev) =>
          prev.filter((v) => (v.video_id ?? v.id) !== deleteTarget.videoId)
        );
      } else if (deleteTarget.type === "bulk" && selectedVideos.size > 0) {
        const videoIds = Array.from(selectedVideos);
        await bulkDeleteVideos(videoIds, token);
        // Invalidate cache for all deleted videos (fire and forget)
        void invalidateClipsCacheMany(videoIds);
        // Remove deleted videos from local state
        setVideos((prev) =>
          prev.filter((v) => {
            const id = v.video_id ?? v.id ?? "";
            return !selectedVideos.has(id);
          })
        );
        setSelectedVideos(new Set());
      } else if (deleteTarget.type === "all") {
        const allVideoIds = videos.map((v) => v.video_id ?? v.id ?? "").filter(Boolean);
        if (allVideoIds.length > 0) {
          await bulkDeleteVideos(allVideoIds, token);
          // Invalidate cache for all deleted videos (fire and forget)
          void invalidateClipsCacheMany(allVideoIds);
          setVideos([]);
          setSelectedVideos(new Set());
        }
      }

      setDeleteDialogOpen(false);
      setDeleteTarget(null);
    } catch (err: unknown) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to delete video(s)";
      setError(errorMessage);
      // Reload videos to sync state
      await loadVideos();
    } finally {
      setDeleting(false);
    }
  };

  const handleCopyUrl = async (url: string, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(url);
      setCopiedUrl(url);
      setTimeout(() => setCopiedUrl(null), 2000);
    } catch (_err) {
      // Fallback for older browsers
      const textArea = document.createElement("textarea");
      textArea.value = url;
      textArea.style.position = "fixed";
      textArea.style.opacity = "0";
      document.body.appendChild(textArea);
      textArea.select();
      try {
        document.execCommand("copy");
        setCopiedUrl(url);
        setTimeout(() => setCopiedUrl(null), 2000);
      } catch (fallbackErr) {
        console.error("Failed to copy URL:", fallbackErr);
      }
      document.body.appendChild(textArea);
    }
  };

  const handleTitleUpdate = async (videoId: string, newTitle: string) => {
    try {
      const token = await getIdToken();
      if (!token) {
        throw new Error("Authentication required");
      }
      await updateVideoTitle(videoId, newTitle, token);
      // Update local state
      setVideos((prev) =>
        prev.map((v) => {
          const id = v.video_id ?? v.id ?? "";
          if (id === videoId) {
            return { ...v, video_title: newTitle };
          }
          return v;
        })
      );
      toast.success("Title updated successfully");
    } catch (error) {
      toast.error("Failed to update title");
      throw error;
    }
  };

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
          <Clock className="h-12 w-12 text-muted-foreground" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">Sign in to view history</h2>
          <p className="text-muted-foreground max-w-md">
            Your processing history is stored securely in your account. Sign in to
            access your past videos.
          </p>
        </div>
        <SignInDialog>
          <Button size="lg" className="gap-2">
            Sign In
          </Button>
        </SignInDialog>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
        <p className="text-muted-foreground">Loading your processing history...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4 text-center">
        <AlertCircle className="h-12 w-12 text-destructive" />
        <div className="space-y-2">
          <h3 className="text-xl font-semibold">Failed to load history</h3>
          <p className="text-muted-foreground">{error}</p>
        </div>
        <Button variant="outline" onClick={() => window.location.reload()}>
          Try Again
        </Button>
      </div>
    );
  }

  if (videos.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-6 text-center">
        <div className="bg-muted/30 p-4 rounded-full">
          <Film className="h-12 w-12 text-muted-foreground" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">No history found</h2>
          <p className="text-muted-foreground max-w-md">
            You haven&apos;t processed any videos yet. Start by processing your first
            video on the home page.
          </p>
        </div>
        <Button asChild>
          <Link href="/">Process Video</Link>
        </Button>
      </div>
    );
  }

  const getDeleteDialogContent = () => {
    if (!deleteTarget) return { title: "", description: "" };

    if (deleteTarget.type === "single") {
      return {
        title: "Delete Video",
        description:
          "Are you sure you want to delete this video? This action cannot be undone and will delete all associated clips and files.",
      };
    } else if (deleteTarget.type === "bulk") {
      return {
        title: `Delete ${selectedVideos.size} Video${selectedVideos.size > 1 ? "s" : ""}`,
        description: `Are you sure you want to delete ${selectedVideos.size} selected video${selectedVideos.size > 1 ? "s" : ""}? This action cannot be undone and will delete all associated clips and files.`,
      };
    } else {
      return {
        title: `Delete All Videos (${videos.length})`,
        description: `Are you sure you want to delete all ${videos.length} videos? This action cannot be undone and will delete all associated clips and files.`,
      };
    }
  };

  const dialogContent = getDeleteDialogContent();

  const usagePercentage = planUsage
    ? Math.min(
        (planUsage.clips_used_this_month / planUsage.max_clips_per_month) * 100,
        100
      )
    : 0;
  const isHighUsage = usagePercentage >= 80;
  const isNearLimit = usagePercentage >= 90;
  const remainingClips = planUsage
    ? Math.max(0, planUsage.max_clips_per_month - planUsage.clips_used_this_month)
    : 0;

  // Storage usage
  const storagePercentage = planUsage?.storage?.percentage ?? 0;
  const isHighStorage = storagePercentage >= 80;
  const isNearStorageLimit = storagePercentage >= 90;

  const getProgressBarColor = () => {
    if (isNearLimit) return "bg-destructive";
    if (isHighUsage) return "bg-destructive/80";
    return "bg-primary";
  };

  const renderUsageContent = () => {
    if (!planUsage) {
      if (loadingUsage) {
        return (
          <div className="text-sm text-muted-foreground">
            Loading usage information...
          </div>
        );
      }
      return (
        <div className="text-sm text-muted-foreground">
          Unable to load usage information.
        </div>
      );
    }

    return (
      <>
        {/* Monthly Clips Usage */}
        <div className="space-y-2">
          <div className="flex justify-between text-sm">
            <span className="text-muted-foreground">Monthly Clips</span>
            <span
              className={
                isHighUsage ? "text-destructive font-semibold" : "text-muted-foreground"
              }
            >
              {planUsage.clips_used_this_month} / {planUsage.max_clips_per_month}
            </span>
          </div>
          <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
            <div
              className={`h-full transition-all duration-500 ${getProgressBarColor()}`}
              style={{ width: `${usagePercentage}%` }}
            />
          </div>
        </div>

        {/* Storage Usage */}
        {planUsage.storage && (
          <div className="space-y-2 pt-2 border-t border-muted">
            <div className="flex justify-between text-sm">
              <span className="text-muted-foreground">Storage</span>
              <span
                className={
                  isHighStorage
                    ? "text-destructive font-semibold"
                    : "text-muted-foreground"
                }
              >
                {planUsage.storage.used_formatted} / {planUsage.storage.limit_formatted}
              </span>
            </div>
            <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
              <div
                className={`h-full transition-all duration-500 ${(() => {
                  if (isNearStorageLimit) {
                    return "bg-destructive";
                  }
                  if (isHighStorage) {
                    return "bg-destructive/80";
                  }
                  return "bg-primary";
                })()}`}
                style={{ width: `${Math.min(storagePercentage, 100)}%` }}
              />
            </div>
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{planUsage.storage.total_clips} total clips</span>
              <span>{planUsage.storage.remaining_formatted} remaining</span>
            </div>
          </div>
        )}

        {/* Over Limit Warning */}
        {(usagePercentage >= 100 || storagePercentage >= 100) && (
          <div className="rounded-lg border border-destructive bg-destructive/20 p-4 space-y-3">
            <div className="flex items-start gap-3">
              <AlertCircle className="h-5 w-5 text-destructive mt-0.5 flex-shrink-0" />
              <div className="flex-1 space-y-2">
                <p className="font-semibold text-destructive">
                  You&apos;ve exceeded your plan limits!
                </p>
                <p className="text-sm text-muted-foreground">
                  {usagePercentage >= 100 &&
                    `You've used ${planUsage?.clips_used_this_month} of ${planUsage?.max_clips_per_month} monthly clips. `}
                  {storagePercentage >= 100 &&
                    `Storage is full (${planUsage?.storage?.used_formatted} / ${planUsage?.storage?.limit_formatted}). `}
                  You cannot create new clips until you upgrade or delete existing
                  clips.
                </p>
                <div className="flex gap-2 mt-2">
                  <Button asChild variant="default" size="sm">
                    <Link href="/pricing">
                      <TrendingUp className="h-4 w-4 mr-2" />
                      Upgrade Plan
                    </Link>
                  </Button>
                </div>
              </div>
            </div>
          </div>
        )}
        {/* High Usage Warnings (not over limit) */}
        {(isHighUsage || isHighStorage) &&
          usagePercentage < 100 &&
          storagePercentage < 100 && (
            <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4 space-y-3">
              <div className="flex items-start gap-3">
                <Zap className="h-5 w-5 text-destructive mt-0.5 flex-shrink-0" />
                <div className="flex-1 space-y-2">
                  <p className="font-semibold text-destructive">
                    {isNearLimit || isNearStorageLimit
                      ? "You're almost at your limit!"
                      : "You're running low on capacity"}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    {isNearLimit &&
                      `Only ${remainingClips} clip${remainingClips !== 1 ? "s" : ""} remaining this month. `}
                    {isNearStorageLimit &&
                      `Only ${planUsage.storage?.remaining_formatted} storage remaining. `}
                    Upgrade to Pro for more capacity!
                  </p>
                  <Button asChild variant="default" size="sm" className="mt-2">
                    <Link href="/pricing">
                      <TrendingUp className="h-4 w-4 mr-2" />
                      Upgrade to Pro
                    </Link>
                  </Button>
                </div>
              </div>
            </div>
          )}
        {!isHighUsage &&
          !isHighStorage &&
          remainingClips < 10 &&
          remainingClips > 0 && (
            <div className="rounded-lg border border-primary/20 bg-primary/5 p-3">
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold text-foreground">
                  {remainingClips} clips remaining
                </span>{" "}
                this month.{" "}
                <Link href="/pricing" className="text-primary hover:underline">
                  Upgrade to Pro
                </Link>{" "}
                for more capacity.
              </p>
            </div>
          )}
      </>
    );
  };

  return (
    <div className="space-y-6 page-container">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold tracking-tight">History</h1>
        <div className="flex items-center gap-4">
          <p className="text-muted-foreground text-sm">
            {videos.length} videos processed
          </p>
          {videos.length > 0 && (
            <div className="flex items-center gap-2">
              {selectedVideos.size > 0 && (
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={() => handleDeleteClick("bulk")}
                  disabled={deleting}
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete Selected ({selectedVideos.size})
                </Button>
              )}
              <Button
                variant="destructive"
                size="sm"
                onClick={() => handleDeleteClick("all")}
                disabled={deleting}
              >
                <Trash2 className="h-4 w-4 mr-2" />
                Delete All
              </Button>
            </div>
          )}
        </div>
      </div>

      {/* Plan Usage Card */}
      {user && (
        <Card
          className={`glass ${isHighUsage ? "border-destructive/50 bg-destructive/5" : ""}`}
        >
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  {isHighUsage ? (
                    <>
                      <AlertCircle className="h-5 w-5 text-destructive" />
                      Monthly Plan Usage
                    </>
                  ) : (
                    "Monthly Plan Usage"
                  )}
                </CardTitle>
                <CardDescription>
                  {planUsage ? (
                    <>
                      {planUsage.clips_used_this_month} of{" "}
                      {planUsage.max_clips_per_month} clips used this month
                      {remainingClips > 0 && ` • ${remainingClips} remaining`}
                    </>
                  ) : (
                    "Loading usage information..."
                  )}
                </CardDescription>
              </div>
              {planUsage && (
                <div className="text-right">
                  <div
                    className={`text-2xl font-bold ${isHighUsage ? "text-destructive" : "text-primary"}`}
                  >
                    {Math.round(usagePercentage)}%
                  </div>
                  <div className="text-xs text-muted-foreground uppercase">
                    {planUsage.plan} Plan
                  </div>
                </div>
              )}
            </div>
          </CardHeader>
          <CardContent className="space-y-4">{renderUsageContent()}</CardContent>
        </Card>
      )}

      <div className="rounded-md border bg-background/50 backdrop-blur-sm">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[50px]">
                <button
                  onClick={handleSelectAll}
                  className="flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
                  type="button"
                  aria-label="Select all videos"
                >
                  {selectedVideos.size === videos.length && videos.length > 0 ? (
                    <CheckSquare className="h-4 w-4 text-primary" />
                  ) : (
                    <Square className="h-4 w-4" />
                  )}
                </button>
              </TableHead>
              <TableHead className="w-[400px]">
                <SortableHeader field="title">Video Details</SortableHeader>
              </TableHead>
              <TableHead>
                <SortableHeader field="status">Status</SortableHeader>
              </TableHead>
              <TableHead>
                <SortableHeader field="size">Size</SortableHeader>
              </TableHead>
              <TableHead>
                <SortableHeader field="date">Date</SortableHeader>
              </TableHead>
              <TableHead className="w-[100px] text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sortedVideos.map((v) => {
              const id = v.video_id ?? v.id ?? "";
              const isSelected = selectedVideos.has(id);
              let dateStr = v.created_at ?? "";
              try {
                if (dateStr) {
                  const dateObj = new Date(dateStr);
                  dateStr = dateObj.toLocaleDateString(undefined, {
                    year: "numeric",
                    month: "short",
                    day: "numeric",
                  });
                }
              } catch (_e) {
                // keep original string if parse fails
              }

              return (
                <TableRow
                  key={id}
                  className={`group cursor-pointer hover:bg-muted/50 transition-colors ${
                    isSelected ? "bg-muted/50" : ""
                  }`}
                  onClick={() => router.push(`/history/${encodeURIComponent(id)}`)}
                >
                  <TableCell>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleSelectVideo(id);
                      }}
                      className="flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
                      type="button"
                      aria-label={isSelected ? "Deselect video" : "Select video"}
                    >
                      {isSelected ? (
                        <CheckSquare className="h-4 w-4 text-primary" />
                      ) : (
                        <Square className="h-4 w-4" />
                      )}
                    </button>
                  </TableCell>
                  <TableCell>
                    <div className="space-y-1.5">
                      <EditableTitle
                        title={v.video_title ?? "Untitled Video"}
                        onSave={(newTitle) => handleTitleUpdate(id, newTitle)}
                        renderTitle={(title) => (
                          <Link
                            href={`/history/${encodeURIComponent(id)}`}
                            className="font-medium hover:underline block max-w-[300px] truncate"
                            title={title}
                            onClick={(e) => e.stopPropagation()}
                          >
                            {title}
                          </Link>
                        )}
                      />

                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <span className="font-mono bg-muted px-1.5 py-0.5 rounded">
                          {id.substring(0, 8)}
                        </span>
                        {v.video_url && (
                          <div className="flex items-center gap-1 group/url max-w-[200px]">
                            <span className="truncate">{v.video_url}</span>
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-4 w-4 opacity-0 group-hover/url:opacity-100 transition-opacity"
                              onClick={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                if (v.video_url) {
                                  void handleCopyUrl(v.video_url, e);
                                }
                              }}
                              aria-label="Copy URL"
                            >
                              {copiedUrl === v.video_url ? (
                                <Check className="h-3 w-3 text-green-500" />
                              ) : (
                                <Copy className="h-3 w-3" />
                              )}
                            </Button>
                          </div>
                        )}
                      </div>

                      {v.custom_prompt && (
                        <p
                          className="text-xs text-muted-foreground italic truncate max-w-[350px]"
                          title={v.custom_prompt}
                        >
                          Prompt: {v.custom_prompt}
                        </p>
                      )}
                    </div>
                  </TableCell>
                  <TableCell>
                    <VideoStatusBadge
                      videoId={id}
                      status={v.status}
                      clipsCount={v.clips_count}
                    />
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {v.total_size_formatted ?? "—"}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {dateStr}
                  </TableCell>
                  <TableCell className="text-right">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8"
                          onClick={(e) => e.stopPropagation()}
                        >
                          <MoreHorizontal className="h-4 w-4" />
                          <span className="sr-only">Actions</span>
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem asChild>
                          <Link href={`/history/${encodeURIComponent(id)}`}>
                            View Details
                          </Link>
                        </DropdownMenuItem>
                        {v.video_url && (
                          <DropdownMenuItem
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              window.open(v.video_url, "_blank", "noopener,noreferrer");
                            }}
                          >
                            Open Original Video
                          </DropdownMenuItem>
                        )}
                        <DropdownMenuItem
                          className="text-destructive focus:text-destructive"
                          onClick={() => handleDeleteClick("single", id)}
                        >
                          Delete Video
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </div>

      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{dialogContent.title}</DialogTitle>
            <DialogDescription>{dialogContent.description}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setDeleteDialogOpen(false);
                setDeleteTarget(null);
              }}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDeleteConfirm}
              disabled={deleting}
            >
              {deleting ? "Deleting..." : "Delete"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
