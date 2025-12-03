"use client";

import { useEffect, useState, useCallback } from "react";
import { Clock, Film, AlertCircle, Trash2, CheckSquare, Square, Copy, Check, TrendingUp, Zap } from "lucide-react";
import Link from "next/link";

import { apiFetch, deleteVideo, bulkDeleteVideos, updateVideoTitle } from "@/lib/apiClient";
import { EditableTitle } from "@/components/EditableTitle";
import { toast } from "sonner";
import { useAuth } from "@/lib/auth";
import { usePageView } from "@/lib/usePageView";
import { Button } from "@/components/ui/button";
import { SignInDialog } from "@/components/SignInDialog";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

interface UserVideo {
  id?: string;
  video_id?: string;
  video_title?: string;
  video_url?: string;
  created_at?: string;
  custom_prompt?: string;
}

interface PlanUsage {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
}

export default function HistoryPage() {
  usePageView("history");
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [videos, setVideos] = useState<UserVideo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedVideos, setSelectedVideos] = useState<Set<string>>(new Set());
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ type: "single" | "bulk" | "all"; videoId?: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);
  const [planUsage, setPlanUsage] = useState<PlanUsage | null>(null);
  const [loadingUsage, setLoadingUsage] = useState(true);

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
          setPlanUsage(usageData as PlanUsage);
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
      setSelectedVideos(new Set(videos.map((v) => v.video_id ?? v.id ?? "").filter(Boolean)));
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
        // Remove from local state immediately for better UX
        setVideos((prev) => prev.filter((v) => (v.video_id ?? v.id) !== deleteTarget.videoId));
      } else if (deleteTarget.type === "bulk" && selectedVideos.size > 0) {
        const videoIds = Array.from(selectedVideos);
        await bulkDeleteVideos(videoIds, token);
        // Remove deleted videos from local state
        setVideos((prev) => prev.filter((v) => {
          const id = v.video_id ?? v.id ?? "";
          return !selectedVideos.has(id);
        }));
        setSelectedVideos(new Set());
      } else if (deleteTarget.type === "all") {
        const allVideoIds = videos.map((v) => v.video_id ?? v.id ?? "").filter(Boolean);
        if (allVideoIds.length > 0) {
          await bulkDeleteVideos(allVideoIds, token);
          setVideos([]);
          setSelectedVideos(new Set());
        }
      }

      setDeleteDialogOpen(false);
      setDeleteTarget(null);
    } catch (err: unknown) {
      const errorMessage = err instanceof Error ? err.message : "Failed to delete video(s)";
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
    } catch (err) {
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
      document.body.removeChild(textArea);
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

  const getDisplayTitle = (title: string | undefined, videoUrl: string | undefined): string => {
    // Check for placeholder or empty titles
    if (!title || title.trim() === "") {
      return "Untitled Video";
    }
    
    // Check for common placeholder patterns
    const placeholderPatterns = [
      /^the main title of the/i,
      /^main title/i,
      /^video title/i,
      /^title/i,
      /^untitled/i,
      /^generated clips/i,
    ];
    
    const trimmedTitle = title.trim();
    if (placeholderPatterns.some(pattern => pattern.test(trimmedTitle))) {
      // Try to extract YouTube ID from URL as fallback
      if (videoUrl) {
        try {
          const urlObj = new URL(videoUrl);
          const videoId = urlObj.searchParams.get("v") || urlObj.pathname.split("/").pop();
          if (videoId && videoId.length > 5) {
            return `YouTube Video (${videoId.substring(0, 8)}...)`;
          }
        } catch {
          // URL parsing failed, use generic fallback
        }
      }
      return "Untitled Video";
    }
    
    return trimmedTitle;
  };

  if (authLoading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
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
            Your processing history is stored securely in your account. Sign in to access your past videos.
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
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
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
            You haven't processed any videos yet. Start by processing your first video on the home page.
          </p>
        </div>
        <Button asChild>
          <a href="/">Process Video</a>
        </Button>
      </div>
    );
  }

  const getDeleteDialogContent = () => {
    if (!deleteTarget) return { title: "", description: "" };

    if (deleteTarget.type === "single") {
      return {
        title: "Delete Video",
        description: "Are you sure you want to delete this video? This action cannot be undone and will delete all associated clips and files.",
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
    ? Math.min((planUsage.clips_used_this_month / planUsage.max_clips_per_month) * 100, 100)
    : 0;
  const isHighUsage = usagePercentage >= 80;
  const isNearLimit = usagePercentage >= 90;
  const remainingClips = planUsage
    ? Math.max(0, planUsage.max_clips_per_month - planUsage.clips_used_this_month)
    : 0;

  return (
    <div className="space-y-6 page-container">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold tracking-tight">History</h1>
        <div className="flex items-center gap-4">
          <p className="text-muted-foreground text-sm">{videos.length} videos processed</p>
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
        <Card className={`glass ${isHighUsage ? "border-destructive/50 bg-destructive/5" : ""}`}>
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
                      {planUsage.clips_used_this_month} of {planUsage.max_clips_per_month} clips used this month
                      {remainingClips > 0 && ` • ${remainingClips} remaining`}
                    </>
                  ) : (
                    "Loading usage information..."
                  )}
                </CardDescription>
              </div>
              {planUsage && (
                <div className="text-right">
                  <div className={`text-2xl font-bold ${isHighUsage ? "text-destructive" : "text-primary"}`}>
                    {Math.round(usagePercentage)}%
                  </div>
                  <div className="text-xs text-muted-foreground uppercase">
                    {planUsage.plan} Plan
                  </div>
                </div>
              )}
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {planUsage ? (
              <>
                <div className="space-y-2">
                  <div className="flex justify-between text-sm">
                    <span className="text-muted-foreground">Usage</span>
                    <span className={isHighUsage ? "text-destructive font-semibold" : "text-muted-foreground"}>
                      {planUsage.clips_used_this_month} / {planUsage.max_clips_per_month}
                    </span>
                  </div>
                  <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      className={`h-full transition-all duration-500 ${
                        isNearLimit
                          ? "bg-destructive"
                          : isHighUsage
                          ? "bg-destructive/80"
                          : "bg-primary"
                      }`}
                      style={{ width: `${usagePercentage}%` }}
                    />
                  </div>
                </div>
                {isHighUsage && (
                  <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4 space-y-3">
                    <div className="flex items-start gap-3">
                      <Zap className="h-5 w-5 text-destructive mt-0.5 flex-shrink-0" />
                      <div className="flex-1 space-y-2">
                        <p className="font-semibold text-destructive">
                          {isNearLimit
                            ? "You're almost out of clips this month!"
                            : "You're running low on clips this month"}
                        </p>
                        <p className="text-sm text-muted-foreground">
                          {isNearLimit
                            ? `Only ${remainingClips} clip${remainingClips !== 1 ? "s" : ""} remaining. Upgrade to Pro for more clips and unlock unlimited potential!`
                            : `You've used ${Math.round(usagePercentage)}% of your monthly limit. Upgrade to Pro to get more clips and never worry about limits again.`}
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
                {!isHighUsage && remainingClips < 10 && (
                  <div className="rounded-lg border border-primary/20 bg-primary/5 p-3">
                    <p className="text-sm text-muted-foreground">
                      <span className="font-semibold text-foreground">{remainingClips} clips remaining</span> this month.{" "}
                      <Link href="/pricing" className="text-primary hover:underline">
                        Upgrade to Pro
                      </Link>{" "}
                      for more capacity.
                    </p>
                  </div>
                )}
              </>
            ) : loadingUsage ? (
              <div className="text-sm text-muted-foreground">Loading usage information...</div>
            ) : (
              <div className="text-sm text-muted-foreground">Unable to load usage information.</div>
            )}
          </CardContent>
        </Card>
      )}

      {videos.length > 0 && (
        <div className="flex items-center gap-2 pb-2 border-b">
          <button
            onClick={handleSelectAll}
            className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
            type="button"
          >
            {selectedVideos.size === videos.length ? (
              <CheckSquare className="h-4 w-4" />
            ) : (
              <Square className="h-4 w-4" />
            )}
            <span>Select All</span>
          </button>
          {selectedVideos.size > 0 && (
            <span className="text-sm text-muted-foreground">
              {selectedVideos.size} selected
            </span>
          )}
        </div>
      )}
      
      <div className="grid gap-4">
        {videos.map((v) => {
          const id = v.video_id ?? v.id ?? "";
          const isSelected = selectedVideos.has(id);
          // Format date if possible
          let dateStr = v.created_at ?? "";
          try {
            if (dateStr) {
              dateStr = new Date(dateStr).toLocaleDateString(undefined, {
                year: 'numeric',
                month: 'long',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit'
              });
            }
          } catch (e) {
            // keep original string if parse fails
          }

          return (
            <Link
              key={id}
              href={`/?id=${encodeURIComponent(id)}`}
              className={`block glass p-6 rounded-xl hover:shadow-md transition-all border ${
                isSelected ? "border-primary/50 bg-primary/5" : "border-border/50"
              } hover:bg-muted/30`}
            >
              <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-4">
                  <div className="flex items-start gap-3 flex-1 min-w-0">
                    <button
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        handleSelectVideo(id);
                      }}
                      className="mt-1 flex-shrink-0"
                      type="button"
                      aria-label={isSelected ? "Deselect video" : "Select video"}
                    >
                      {isSelected ? (
                        <CheckSquare className="h-5 w-5 text-primary" />
                      ) : (
                        <Square className="h-5 w-5 text-muted-foreground" />
                      )}
                    </button>
                    <div className="flex-1 min-w-0 space-y-2 group">
                      <div className="flex items-center gap-2 flex-wrap">
                        <div
                          onClick={(e) => {
                            // Prevent navigation when clicking on editable title
                            e.preventDefault();
                            e.stopPropagation();
                          }}
                          className="flex-1 min-w-0"
                        >
                          <EditableTitle
                            title={v.video_title || "Untitled Video"}
                            onSave={(newTitle) => handleTitleUpdate(id, newTitle)}
                            className="truncate"
                          />
                        </div>
                        <div
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                          }}
                          className="inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 border-transparent bg-secondary text-secondary-foreground"
                        >
                          ID: {id.substring(0, 8)}...
                        </div>
                      </div>
                    
                    {v.video_url && (
                      <div className="flex items-center gap-2 group/url">
                        <a
                          href={v.video_url}
                          target="_blank"
                          rel="noopener noreferrer"
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            window.open(v.video_url, '_blank', 'noopener,noreferrer');
                          }}
                          className="text-sm text-muted-foreground truncate font-mono bg-muted/30 px-2 py-1 rounded hover:text-primary hover:bg-muted/50 transition-colors flex-1 min-w-0"
                        >
                          {v.video_url}
                        </a>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8 flex-shrink-0"
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            handleCopyUrl(v.video_url!, e);
                          }}
                          aria-label="Copy URL"
                        >
                          {copiedUrl === v.video_url ? (
                            <Check className="h-4 w-4 text-green-500" />
                          ) : (
                            <Copy className="h-4 w-4" />
                          )}
                        </Button>
                      </div>
                    )}
                    
                    {v.custom_prompt && (
                      <div className="mt-2 text-xs text-muted-foreground bg-muted/20 p-2 rounded border border-border/30">
                        <span className="font-semibold mr-1">Custom Prompt:</span>
                        <span className="italic">{v.custom_prompt}</span>
                      </div>
                    )}
                  </div>
                </div>
                
                <div className="flex items-center gap-3 sm:flex-row flex-row-reverse justify-end">
                  <div className="text-xs text-muted-foreground whitespace-nowrap flex items-center gap-1 sm:text-right">
                    <Clock className="h-3 w-3" />
                    {dateStr}
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      handleDeleteClick("single", id);
                    }}
                    disabled={deleting}
                    className="text-destructive hover:text-destructive hover:bg-destructive/10"
                    aria-label="Delete video"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </Link>
          );
        })}
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