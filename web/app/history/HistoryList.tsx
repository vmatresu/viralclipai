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
  RefreshCw,
  Square,
  Trash2,
  TrendingUp,
  Zap,
} from "lucide-react";
import Link from "next/link";
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
import {
  apiFetch,
  bulkDeleteVideos,
  deleteVideo,
  getUserVideos,
  updateVideoTitle,
} from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { invalidateClipsCache, invalidateClipsCacheMany } from "@/lib/cache";
import { usePageView } from "@/lib/usePageView";
import { cn } from "@/lib/utils";

import { DeleteConfirmDialog } from "./components";
import {
  type DeleteTarget,
  type PlanUsage,
  type SortDirection,
  type SortField,
  type UserVideo,
  parseSizeToBytes,
} from "./types";

export default function HistoryList() {
  usePageView("history");
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [videos, setVideos] = useState<UserVideo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nextPageToken, setNextPageToken] = useState<string | null>(null);
  const [pageTokens, setPageTokens] = useState<(string | null)[]>([null]);
  const [currentPage, setCurrentPage] = useState(0);
  const [selectedVideos, setSelectedVideos] = useState<Set<string>>(new Set());
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);
  const [planUsage, setPlanUsage] = useState<PlanUsage | null>(null);
  const [loadingUsage, setLoadingUsage] = useState(true);
  const [sortField, setSortField] = useState<SortField>("date");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");

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

  const handleSort = useCallback(
    (field: SortField) => {
      if (field === sortField) {
        setSortDirection((prev) => (prev === "asc" ? "desc" : "asc"));
      } else {
        setSortField(field);
        setSortDirection("asc");
      }
    },
    [sortField]
  );

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
          {isActive && sortDirection === "asc" && <ArrowUp className="h-3.5 w-3.5" />}
          {isActive && sortDirection === "desc" && (
            <ArrowDown className="h-3.5 w-3.5" />
          )}
          {!isActive && <ArrowUpDown className="h-3.5 w-3.5 opacity-50" />}
        </button>
      );
    },
    [sortField, sortDirection, handleSort]
  );

  const fetchVideos = useCallback(
    async (pageToken: string | null) => {
      if (!user) return;
      setLoading(true);
      try {
        const token = await getIdToken();
        if (!token) throw new Error("Failed to get authentication token");

        const data = await getUserVideos<UserVideo>(token, {
          limit: 10, // Reduced from 25 for better pagination UX
          pageToken,
        });
        setVideos(data.videos ?? []);
        setNextPageToken((data.next_page_token as string | null) ?? null);
        setError(null);
        setSelectedVideos(new Set());
      } catch (err: unknown) {
        const errorMessage =
          err instanceof Error ? err.message : "Failed to load history";
        setError(errorMessage);
      } finally {
        setLoading(false);
      }
    },
    [getIdToken, user]
  );

  const handleNextPage = async () => {
    if (!nextPageToken) return;
    const nextTokens = [...pageTokens, nextPageToken];
    setPageTokens(nextTokens);
    setCurrentPage(currentPage + 1);
    await fetchVideos(nextPageToken);
  };

  const handlePrevPage = async () => {
    if (currentPage === 0) return;
    const prevPage = currentPage - 1;
    setCurrentPage(prevPage);
    await fetchVideos(pageTokens.at(prevPage) ?? null);
  };

  useEffect(() => {
    let cancelled = false;

    async function load() {
      if (authLoading) return;

      if (!user) {
        if (!cancelled) {
          setLoading(false);
          setLoadingUsage(false);
        }
        return;
      }

      // Initial load
      void fetchVideos(null);

      // Load usage separately
      try {
        const token = await getIdToken();
        if (token) {
          const usageData = await apiFetch<PlanUsage>("/api/settings", { token });
          if (!cancelled) {
            setPlanUsage(usageData);
          }
        }
      } catch (e) {
        console.error("Failed to load usage", e);
      } finally {
        if (!cancelled) {
          setLoadingUsage(false);
        }
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [getIdToken, user, authLoading, fetchVideos]);

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
      await fetchVideos(pageTokens.at(currentPage) ?? null);
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

  const usagePercentage = planUsage
    ? Math.min(
        (planUsage.credits_used_this_month / planUsage.monthly_credits_limit) *
          100,
        100
      )
    : 0;
  const isHighUsage = usagePercentage >= 80;
  const isNearLimit = usagePercentage >= 90;
  const remainingCredits = planUsage
    ? Math.max(
        0,
        planUsage.monthly_credits_limit - planUsage.credits_used_this_month
      )
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
        {/* Monthly Credits Usage */}
        <div className="space-y-2">
          <div className="flex justify-between text-sm">
            <span className="text-muted-foreground">Monthly Credits</span>
            <span
              className={
                isHighUsage
                  ? "text-destructive font-semibold"
                  : "text-muted-foreground"
              }
            >
              {planUsage.credits_used_this_month.toLocaleString()} /{" "}
              {planUsage.monthly_credits_limit.toLocaleString()}
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
                    `You've used ${planUsage?.credits_used_this_month?.toLocaleString()} of ${planUsage?.monthly_credits_limit?.toLocaleString()} monthly credits. `}
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
                      `Only ${remainingCredits.toLocaleString()} credits remaining this month. `}
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
          remainingCredits < 100 &&
          remainingCredits > 0 && (
            <div className="rounded-lg border border-primary/20 bg-primary/5 p-3">
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold text-foreground">
                  {remainingCredits.toLocaleString()} credits remaining
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
    <div className="w-full max-w-7xl mx-auto space-y-8 p-6 md:p-8 rounded-3xl glass-card relative overflow-hidden">
      <div className="flex items-center justify-between mb-8">
        <h1 className="text-3xl font-bold tracking-tight bg-clip-text text-transparent bg-gradient-to-r from-white to-white/70">
          History
        </h1>
        <div className="flex items-center gap-4">
          <p className="text-muted-foreground text-sm font-medium">
            {videos.length} videos processed
          </p>
          <Button
            variant="outline"
            size="sm"
            onClick={() => fetchVideos(pageTokens.at(currentPage) ?? null)}
            disabled={loading}
          >
            <RefreshCw className={cn("h-4 w-4 mr-2", loading && "animate-spin")} />
            Refresh
          </Button>
          {videos.length > 0 && (
            <div className="flex items-center gap-2">
              {selectedVideos.size > 0 && (
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={() => handleDeleteClick("bulk")}
                  disabled={deleting}
                  className="bg-destructive/10 text-destructive border-destructive/20 hover:bg-destructive/20"
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
                className="bg-destructive/10 text-destructive border-destructive/20 hover:bg-destructive/20"
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
          className={`glass border-white/5 bg-white/5 ${isHighUsage ? "border-destructive/50 bg-destructive/5" : ""}`}
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
                      {planUsage.credits_used_this_month.toLocaleString()} of{" "}
                      {planUsage.monthly_credits_limit.toLocaleString()} credits
                      used this month
                      {remainingCredits > 0 &&
                        ` • ${remainingCredits.toLocaleString()} remaining`}
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

      <div className="rounded-xl border border-white/10 bg-white/[0.02] backdrop-blur-sm overflow-hidden">
        <Table>
          <TableHeader className="bg-white/[0.02]">
            <TableRow className="border-white/5 hover:bg-transparent">
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
              <TableHead className="w-[280px]">
                <SortableHeader field="title">Video Details</SortableHeader>
              </TableHead>
              <TableHead className="w-[140px]">
                <SortableHeader field="status">Status</SortableHeader>
              </TableHead>
              <TableHead className="w-[120px]">
                <SortableHeader field="size">Size</SortableHeader>
              </TableHead>
              <TableHead className="w-[140px]">
                <SortableHeader field="date">Date</SortableHeader>
              </TableHead>
              <TableHead className="w-[50px] text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sortedVideos.map((video) => {
              const videoId = video.video_id ?? video.id ?? "";
              const isSelected = selectedVideos.has(videoId);
              const isProcessing = video.status === "processing";

              return (
                <TableRow
                  key={videoId}
                  className={cn(
                    "border-white/5 transition-colors",
                    isSelected
                      ? "bg-primary/5 hover:bg-primary/10"
                      : "hover:bg-white/[0.02]"
                  )}
                >
                  <TableCell>
                    <button
                      onClick={() => handleSelectVideo(videoId)}
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
                    <div className="flex flex-col gap-1">
                      <div className="flex items-center gap-2">
                        {isProcessing ? (
                          <span className="font-medium text-foreground max-w-[240px] truncate block">
                            {video.video_title?.trim()
                              ? video.video_title
                              : "Untitled Video"}
                          </span>
                        ) : (
                          <div className="w-full max-w-[240px] group/title">
                            <EditableTitle
                              title={
                                video.video_title?.trim()
                                  ? video.video_title
                                  : "Untitled Video"
                              }
                              onSave={(newTitle) =>
                                handleTitleUpdate(videoId, newTitle)
                              }
                              className="w-full"
                              renderTitle={(title) => (
                                <Link
                                  href={`/history/${encodeURIComponent(videoId)}`}
                                  className="font-medium text-foreground hover:text-primary transition-colors hover:underline truncate block"
                                  onClick={(e) => e.stopPropagation()}
                                >
                                  {title}
                                </Link>
                              )}
                            />
                          </div>
                        )}
                      </div>
                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        {video.video_url && (
                          <>
                            <a
                              href={video.video_url}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="hover:text-foreground transition-colors flex items-center gap-1 max-w-[200px] truncate"
                              onClick={(e) => e.stopPropagation()}
                            >
                              {video.video_url}
                            </a>
                            <button
                              onClick={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                if (video.video_url) {
                                  void handleCopyUrl(video.video_url, e);
                                }
                              }}
                              className="opacity-0 group-hover:opacity-100 transition-opacity hover:text-foreground"
                              title="Copy URL"
                            >
                              {copiedUrl === video.video_url ? (
                                <Check className="h-3 w-3 text-green-500" />
                              ) : (
                                <Copy className="h-3 w-3" />
                              )}
                            </button>
                          </>
                        )}
                      </div>
                      {video.custom_prompt && (
                        <p
                          className="text-xs text-muted-foreground italic truncate max-w-[350px]"
                          title={video.custom_prompt}
                        >
                          Prompt: {video.custom_prompt}
                        </p>
                      )}
                    </div>
                  </TableCell>
                  <TableCell>
                    <VideoStatusBadge
                      videoId={videoId}
                      status={video.status}
                      clipsCount={video.clips_count}
                    />
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {video.total_size_formatted ?? "—"}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {video.created_at
                      ? new Date(video.created_at).toLocaleDateString("en-GB", {
                          day: "numeric",
                          month: "short",
                          year: "numeric",
                        })
                      : "—"}
                  </TableCell>
                  <TableCell className="text-right">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8 hover:bg-white/5"
                          onClick={(e) => e.stopPropagation()}
                        >
                          <MoreHorizontal className="h-4 w-4 text-muted-foreground" />
                          <span className="sr-only">Open menu</span>
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent
                        align="end"
                        className="bg-[#0B0E1A] border-white/10 text-foreground"
                      >
                        <DropdownMenuItem asChild>
                          <Link
                            href={`/history/${encodeURIComponent(videoId)}`}
                            className="cursor-pointer"
                          >
                            View Details
                          </Link>
                        </DropdownMenuItem>
                        {video.video_url && (
                          <DropdownMenuItem
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              window.open(
                                video.video_url,
                                "_blank",
                                "noopener,noreferrer"
                              );
                            }}
                            className="cursor-pointer"
                          >
                            Open Original Video
                          </DropdownMenuItem>
                        )}
                        <DropdownMenuItem
                          className="text-destructive focus:text-destructive cursor-pointer"
                          onClick={() => handleDeleteClick("single", videoId)}
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

      {/* Pagination Controls */}
      <div className="flex items-center justify-between mt-6">
        <div className="text-sm text-muted-foreground">Page {currentPage + 1}</div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handlePrevPage}
            disabled={currentPage === 0 || loading}
            className="border-white/10 hover:bg-white/5"
          >
            Previous
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleNextPage}
            disabled={!nextPageToken || loading}
            className="border-white/10 hover:bg-white/5"
          >
            Next
          </Button>
        </div>
      </div>

      <DeleteConfirmDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        target={deleteTarget}
        deleting={deleting}
        selectedCount={selectedVideos.size}
        totalCount={videos.length}
        onConfirm={handleDeleteConfirm}
      />
    </div>
  );
}
