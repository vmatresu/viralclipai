"use client";

import {
  CheckSquare,
  Download,
  Film,
  Link2,
  Play,
  Share2,
  Square,
  Trash,
  Trash2,
  UploadCloud,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

// ESLint disable for false positive security warnings
// These functions are used safely with controlled, sanitized data
/* eslint-disable security/detect-object-injection */

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { analyticsEvents } from "@/lib/analytics";
import { apiFetch, bulkDeleteClips, deleteAllClips, deleteClip } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { invalidateClipsCache } from "@/lib/cache";
import { copyShareUrl, downloadClip, getPlaybackUrl } from "@/lib/clipDelivery";
import { frontendLogger } from "@/lib/logger";
import { getStyleLabel, getStyleTier, getTierBadgeClasses } from "@/lib/styleTiers";
import { cn } from "@/lib/utils";

/**
 * Get a unique key for a clip using its clip_id.
 * This is the stable identifier for all UI state management.
 */
function getClipKey(clip: Clip): string {
  return clip.clip_id;
}

interface VideoPlayerProps {
  id: string;
  onRef: (el: HTMLVideoElement | null) => void;
  onPlay: () => void;
  getVideoUrl: () => Promise<string>;
  thumbnailUrl?: string;
}

function VideoPlayer({
  id,
  onRef,
  onPlay,
  getVideoUrl,
  thumbnailUrl,
}: VideoPlayerProps) {
  const [videoUrl, setVideoUrl] = useState<string>("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function loadVideo() {
      try {
        setLoading(true);
        const url = await getVideoUrl();
        if (!cancelled) {
          setVideoUrl(url);
        }
      } catch (error) {
        frontendLogger.error("Failed to load video URL", error);
        if (!cancelled) {
          setVideoUrl("");
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadVideo();

    return () => {
      cancelled = true;
    };
  }, [getVideoUrl]);

  if (loading) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <div className="text-muted-foreground">Loading video...</div>
      </div>
    );
  }

  if (!videoUrl) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <div className="text-destructive">Failed to load video</div>
      </div>
    );
  }

  return (
    <video
      id={id}
      ref={onRef}
      controls
      preload="metadata"
      className="w-full h-full object-contain"
      poster={thumbnailUrl}
      src={videoUrl}
      onPlay={onPlay}
    >
      <track kind="captions" />
    </video>
  );
}

export interface Clip {
  clip_id: string;
  name: string;
  title: string;
  description: string;
  size: string;
  style?: string;
  has_thumbnail?: boolean;
  completed_at?: string | null;
  updated_at?: string | null;
}

interface ClipGridProps {
  videoId: string;
  clips: Clip[];
  log: (msg: string, type?: "info" | "error" | "success") => void;
  onClipDeleted?: (clipName: string) => void;
}

export function ClipGrid({ videoId, clips, log, onClipDeleted }: ClipGridProps) {
  const { getIdToken } = useAuth();
  const [publishing, setPublishing] = useState<string | null>(null);
  const [playingClip, setPlayingClip] = useState<string | null>(null);
  const [deletingClip, setDeletingClip] = useState<string | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [clipToDelete, setClipToDelete] = useState<Clip | null>(null);
  const videoRefs = useRef<{ [key: string]: HTMLVideoElement | null }>({});
  const [downloading, setDownloading] = useState<string | null>(null);

  // Bulk selection state
  const [selectedClips, setSelectedClips] = useState<Set<string>>(new Set());
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkDeleteDialogOpen, setBulkDeleteDialogOpen] = useState(false);
  const [bulkDeleteAllDialogOpen, setBulkDeleteAllDialogOpen] = useState(false);

  // Function to get video URL via delivery endpoint
  const getVideoUrl = async (clipId: string): Promise<string> => {
    const token = await getIdToken();
    if (!token) {
      throw new Error("Authentication required");
    }
    const response = await getPlaybackUrl(clipId, token);
    return response.url;
  };

  // Function to handle download via delivery endpoint
  const handleDownload = async (clip: Clip) => {
    const clipKey = getClipKey(clip);
    setDownloading(clipKey);
    try {
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to download clips.");
        return;
      }
      await downloadClip(clip.clip_id, token, clip.name);
      const styleMatch = clip.name.match(/_([^_]+)\.(mp4|jpg)$/);
      const clipStyle = styleMatch?.[1] ?? "unknown";
      void analyticsEvents.clipDownloaded({
        clipId: clip.clip_id,
        clipName: clip.name,
        style: clipStyle,
      });
    } catch (error) {
      frontendLogger.error("Download failed", error);
      toast.error("Failed to download clip");
    } finally {
      setDownloading(null);
    }
  };

  const handlePlay = (clipKey: string) => {
    if (playingClip && playingClip !== clipKey) {
      const prevVideo = videoRefs.current[playingClip];
      if (prevVideo) {
        prevVideo.pause();
      }
    }
    setPlayingClip(clipKey);
  };

  // Bulk selection functions
  const handleSelectAll = () => {
    if (selectedClips.size === clips.length) {
      setSelectedClips(new Set());
    } else {
      setSelectedClips(new Set(clips.map(getClipKey)));
    }
  };

  const handleClipSelect = (clipKey: string) => {
    setSelectedClips((prev) => {
      const next = new Set(prev);
      if (next.has(clipKey)) {
        next.delete(clipKey);
      } else {
        next.add(clipKey);
      }
      return next;
    });
  };

  // Bulk deletion functions
  const handleBulkDeleteSelected = () => {
    if (selectedClips.size === 0) {
      toast.error("Please select clips to delete");
      return;
    }
    setBulkDeleteDialogOpen(true);
  };

  const handleBulkDeleteAll = () => {
    setBulkDeleteAllDialogOpen(true);
  };

  const handleBulkDeleteConfirm = async () => {
    if (selectedClips.size === 0) return;

    setBulkDeleting(true);
    try {
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to delete clips.");
        return;
      }

      // Map clip_ids to filenames for API call
      const clipKeyToName = new Map(clips.map((c) => [getClipKey(c), c.name]));
      const clipNames = Array.from(selectedClips)
        .map((key) => clipKeyToName.get(key))
        .filter((name): name is string => Boolean(name));

      const result = await bulkDeleteClips(videoId, clipNames, token);

      // Invalidate cache since clips have changed
      void invalidateClipsCache(videoId);

      // Stop playing if any selected clip was playing
      if (playingClip && selectedClips.has(playingClip)) {
        const video = videoRefs.current[playingClip];
        if (video) {
          video.pause();
        }
        setPlayingClip(null);
      }

      // Remove video refs for deleted clips
      selectedClips.forEach((clipKey) => {
        delete videoRefs.current[clipKey];
      });

      const successful = result.deleted_count;
      const failed = result.failed_count;

      if (successful > 0) {
        log(`${successful} clip(s) deleted successfully.`, "success");
        toast.success(
          `Deleted ${successful} clip(s)${failed > 0 ? ` (${failed} failed)` : ""}`
        );
      }

      if (failed > 0) {
        log(`Failed to delete ${failed} clip(s).`, "error");
        toast.error(`Failed to delete ${failed} clip(s)`);
      }

      // Clear selection
      setSelectedClips(new Set());

      // Notify parent component (using filenames for backward compatibility)
      clipNames.forEach((clipName) => {
        if (onClipDeleted) {
          onClipDeleted(clipName);
        }
      });

      setBulkDeleteDialogOpen(false);
    } catch (err: unknown) {
      frontendLogger.error("Failed to bulk delete clips", err);
      const errorMessage = err instanceof Error ? err.message : "Unknown error";
      log(`Failed to delete clips: ${errorMessage}`, "error");
      toast.error("Failed to delete clips. Please try again.");
    } finally {
      setBulkDeleting(false);
    }
  };

  const handleDeleteAllConfirm = async () => {
    setBulkDeleting(true);
    try {
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to delete clips.");
        return;
      }

      const result = await deleteAllClips(videoId, token);

      // Invalidate cache since clips have changed
      void invalidateClipsCache(videoId);

      // Stop playing if any clip was playing
      if (playingClip) {
        const video = videoRefs.current[playingClip];
        if (video) {
          video.pause();
        }
        setPlayingClip(null);
      }

      // Clear all video refs
      videoRefs.current = {};

      const successful = result.deleted_count;
      const failed = result.failed_count;

      if (successful > 0) {
        log(`${successful} clip(s) deleted successfully.`, "success");
        toast.success(
          `Deleted all ${successful} clip(s)${failed > 0 ? ` (${failed} failed)` : ""}`
        );
      }

      if (failed > 0) {
        log(`Failed to delete ${failed} clip(s).`, "error");
        toast.error(`Failed to delete ${failed} clip(s)`);
      }

      // Clear selection
      setSelectedClips(new Set());

      // Notify parent component for all clips
      clips.forEach((clip) => {
        if (onClipDeleted) {
          onClipDeleted(clip.name);
        }
      });

      setBulkDeleteAllDialogOpen(false);
    } catch (err: unknown) {
      frontendLogger.error("Failed to delete all clips", err);
      const errorMessage = err instanceof Error ? err.message : "Unknown error";
      log(`Failed to delete all clips: ${errorMessage}`, "error");
      toast.error("Failed to delete all clips. Please try again.");
    } finally {
      setBulkDeleting(false);
    }
  };

  const handleDeleteClick = (clip: Clip) => {
    setClipToDelete(clip);
    setDeleteDialogOpen(true);
  };

  const handleDeleteConfirm = async () => {
    if (!clipToDelete) return;

    const clipKey = getClipKey(clipToDelete);
    setDeletingClip(clipKey);
    try {
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to delete clips.");
        return;
      }

      await deleteClip(videoId, clipToDelete.name, token);

      // Invalidate cache since clips have changed (fire and forget)
      void invalidateClipsCache(videoId);

      // Stop playing if this clip was playing
      if (playingClip === clipKey) {
        const video = videoRefs.current[clipKey];
        if (video) {
          video.pause();
        }
        setPlayingClip(null);
      }

      // Remove video ref
      delete videoRefs.current[clipKey];

      log(`Clip "${clipToDelete.title}" deleted successfully.`, "success");
      toast.success("Clip deleted successfully");

      // Notify parent component
      if (onClipDeleted) {
        onClipDeleted(clipToDelete.name);
      }

      setDeleteDialogOpen(false);
      setClipToDelete(null);
    } catch (err: unknown) {
      frontendLogger.error("Failed to delete clip", err);
      const errorMessage = err instanceof Error ? err.message : "Unknown error";
      log(`Failed to delete clip: ${errorMessage}`, "error");
      toast.error("Failed to delete clip. Please try again.");
    } finally {
      setDeletingClip(null);
    }
  };

  async function publishToTikTok(clip: Clip, title: string, description: string) {
    const clipKey = getClipKey(clip);
    try {
      setPublishing(clipKey);
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to publish clips to TikTok.");
        return;
      }
      await apiFetch(
        `/api/videos/${encodeURIComponent(videoId)}/clips/${encodeURIComponent(
          clip.name
        )}/publish/tiktok`,
        {
          method: "POST",
          token,
          body: {
            title,
            description,
          },
        }
      );
      log("Clip published to TikTok successfully.", "success");
      toast.success("Published to TikTok successfully!");

      // Track successful TikTok publish
      void analyticsEvents.clipPublishedTikTok({
        clipId: clip.clip_id,
        clipName: clip.name,
        success: true,
      });
    } catch (err: unknown) {
      frontendLogger.error("TikTok publish failed", err);
      const errorMessage = err instanceof Error ? err.message : "Unknown error";
      log(`TikTok publish failed: ${errorMessage}`, "error");
      toast.error("TikTok publish failed. Check console for details.");

      // Track failed TikTok publish
      void analyticsEvents.clipPublishedFailed({
        clipId: clip.clip_id,
        clipName: clip.name,
        errorType: errorMessage,
      });
    } finally {
      setPublishing(null);
    }
  }

  if (!clips.length) {
    return (
      <div className="col-span-full text-center text-muted-foreground py-12 flex flex-col items-center">
        <UploadCloud className="h-12 w-12 mb-4 opacity-20" />
        <p>No clips found. Check logs for errors.</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Bulk Actions Bar */}
      <div className="flex items-center justify-between p-4 bg-muted/30 rounded-lg border border-border/50">
        <div className="flex items-center gap-4">
          <Button
            variant="outline"
            size="sm"
            onClick={handleSelectAll}
            className="gap-2"
          >
            {selectedClips.size === clips.length ? (
              <CheckSquare className="h-4 w-4" />
            ) : (
              <Square className="h-4 w-4" />
            )}
            {selectedClips.size === clips.length ? "Deselect All" : "Select All"}
            {selectedClips.size > 0 && (
              <span className="ml-1 text-xs bg-primary text-primary-foreground px-2 py-0.5 rounded-full">
                {selectedClips.size}
              </span>
            )}
          </Button>

          {selectedClips.size > 0 && (
            <Button
              variant="destructive"
              size="sm"
              onClick={handleBulkDeleteSelected}
              disabled={bulkDeleting}
              className="gap-2"
            >
              <Trash className="h-4 w-4" />
              Delete Selected ({selectedClips.size})
            </Button>
          )}
        </div>

        <Button
          variant="destructive"
          size="sm"
          onClick={handleBulkDeleteAll}
          disabled={bulkDeleting}
          className="gap-2"
        >
          <Trash className="h-4 w-4" />
          Delete All Clips
        </Button>
      </div>

      {/* Clips Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
        {clips.map((clip, index) => {
          const uniqueId = `clip-${index}`;
          const clipKey = getClipKey(clip);
          const isPlaying = playingClip === clipKey;
          const isSelected = selectedClips.has(clipKey);

          return (
            <Card
              key={clipKey}
              className={`bg-card border-border/50 overflow-hidden hover:shadow-xl hover:border-primary/20 transition-all group flex flex-col rounded-xl ${
                isSelected ? "ring-2 ring-primary" : ""
              }`}
            >
              {/* Video Player Area */}
              <div className="relative aspect-[9/16] bg-black group-hover:opacity-100 transition-opacity">
                <VideoPlayer
                  id={uniqueId}
                  onRef={(el) => {
                    videoRefs.current[clipKey] = el;
                  }}
                  onPlay={() => handlePlay(clipKey)}
                  getVideoUrl={() => getVideoUrl(clip.clip_id)}
                />

                {/* Custom Play Button Overlay (only visible when paused) */}
                {!isPlaying && (
                  <div
                    className="absolute inset-0 flex items-center justify-center bg-black/20 group-hover:bg-black/10 transition-colors cursor-pointer"
                    onClick={() => {
                      const video = videoRefs.current[getClipKey(clip)];
                      if (video) void video.play();
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        const video = videoRefs.current[getClipKey(clip)];
                        if (video) void video.play();
                      }
                    }}
                    tabIndex={0}
                    role="button"
                    aria-label={`Play ${clip.title}`}
                  >
                    <div className="w-16 h-16 rounded-full bg-white/90 backdrop-blur-sm flex items-center justify-center pl-1 shadow-lg transform group-hover:scale-110 transition-transform">
                      <Play className="h-8 w-8 text-primary fill-primary" />
                    </div>
                  </div>
                )}
              </div>

              <CardContent className="p-6 flex-1 flex flex-col gap-4">
                {/* Header: Checkbox, Title & Badges */}
                <div>
                  <div className="flex items-start justify-between gap-4 mb-2">
                    <div className="flex items-start gap-3 flex-1 min-w-0">
                      {/* Selection Checkbox */}
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-6 w-6 p-0 flex-shrink-0 mt-1"
                        onClick={() => handleClipSelect(clipKey)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            handleClipSelect(clipKey);
                          }
                        }}
                        aria-label={
                          isSelected ? `Deselect ${clip.title}` : `Select ${clip.title}`
                        }
                        role="checkbox"
                        aria-checked={isSelected}
                        tabIndex={0}
                      >
                        {isSelected ? (
                          <CheckSquare className="h-4 w-4 text-primary" />
                        ) : (
                          <Square className="h-4 w-4 text-muted-foreground" />
                        )}
                      </Button>

                      <div className="flex-1 min-w-0">
                        <h4
                          className="font-bold text-xl leading-tight text-foreground group-hover:text-primary transition-colors line-clamp-2"
                          title={clip.title}
                        >
                          {clip.title}
                        </h4>
                        {/* Style Tag */}
                        {clip.style && (
                          <div
                            className={cn(
                              "mt-2 inline-flex items-center gap-1.5 px-2.5 py-1 border rounded-md text-xs font-medium",
                              getTierBadgeClasses(getStyleTier(clip.style)?.color)
                            )}
                          >
                            <Film className="h-3 w-3" />
                            <span>{getStyleLabel(clip.style) ?? clip.style}</span>
                          </div>
                        )}
                      </div>
                    </div>
                    <span className="px-2 py-1 text-xs font-medium bg-secondary text-secondary-foreground rounded-md whitespace-nowrap shrink-0">
                      {clip.size}
                    </span>
                  </div>
                </div>

                {/* Metadata Editor */}
                <div className="space-y-4 bg-muted/30 p-4 rounded-lg border border-border/50">
                  <div className="space-y-2">
                    <Label
                      htmlFor={`${uniqueId}-title-text`}
                      className="text-xs uppercase tracking-wider font-semibold text-muted-foreground"
                    >
                      Title
                    </Label>
                    <Textarea
                      id={`${uniqueId}-title-text`}
                      rows={2}
                      defaultValue={clip.title}
                      className="resize-none bg-background text-sm"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label
                      htmlFor={`${uniqueId}-desc-text`}
                      className="text-xs uppercase tracking-wider font-semibold text-muted-foreground"
                    >
                      Description
                    </Label>
                    <Textarea
                      id={`${uniqueId}-desc-text`}
                      rows={3}
                      defaultValue={clip.description}
                      className="resize-none bg-background text-sm"
                    />
                  </div>
                </div>

                {/* Action Buttons */}
                <div className="mt-auto pt-2 flex gap-3">
                  <Button
                    variant="outline"
                    className="flex-1 gap-2 hover:bg-primary hover:text-primary-foreground transition-colors"
                    onClick={() => handleDownload(clip)}
                    disabled={downloading === clipKey}
                  >
                    <Download className="h-4 w-4" />
                    {downloading === clipKey ? "Downloading..." : "Download"}
                  </Button>

                  <Button
                    variant="secondary"
                    size="icon"
                    className="shrink-0"
                    onClick={async () => {
                      try {
                        const token = await getIdToken();
                        if (token) {
                          await copyShareUrl(clip.clip_id, token);
                          void analyticsEvents.clipCopiedLink({
                            clipId: clip.clip_id,
                            clipName: clip.name,
                          });
                        } else {
                          toast.error("Please sign in to share clips.");
                        }
                      } catch (error) {
                        frontendLogger.error("Failed to copy share link", error);
                        toast.error("Failed to create share link");
                      }
                    }}
                    title="Copy Share Link"
                  >
                    <Link2 className="h-4 w-4" />
                  </Button>

                  <Button
                    variant="brand"
                    className="flex-1 gap-2 shadow-md hover:shadow-lg hover:scale-[1.02] transition-all"
                    onClick={() => {
                      const titleEl = document.getElementById(
                        `${uniqueId}-title-text`
                      ) as HTMLTextAreaElement | null;
                      const descEl = document.getElementById(
                        `${uniqueId}-desc-text`
                      ) as HTMLTextAreaElement | null;
                      void publishToTikTok(
                        clip,
                        titleEl?.value ?? clip.title,
                        descEl?.value ?? clip.description
                      );
                    }}
                    disabled={publishing === clip.name}
                    title="Publish to TikTok"
                  >
                    <Share2 className="h-4 w-4" />
                    {publishing === clip.name ? "Publishing..." : "Share"}
                  </Button>

                  <Button
                    variant="ghost"
                    size="icon"
                    className="shrink-0 text-destructive hover:text-destructive hover:bg-destructive/10"
                    onClick={() => handleDeleteClick(clip)}
                    disabled={deletingClip === clip.name}
                    title="Delete clip"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Clip</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete &quot;{clipToDelete?.title}&quot;? This
              action cannot be undone and will delete the clip file and thumbnail.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setDeleteDialogOpen(false);
                setClipToDelete(null);
              }}
              disabled={deletingClip !== null}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDeleteConfirm}
              disabled={deletingClip !== null}
            >
              {deletingClip ? "Deleting..." : "Delete"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Bulk Delete Selected Dialog */}
      <Dialog open={bulkDeleteDialogOpen} onOpenChange={setBulkDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Selected Clips</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete {selectedClips.size} selected clip(s)?
              This action cannot be undone and will delete the clip files and
              thumbnails.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setBulkDeleteDialogOpen(false)}
              disabled={bulkDeleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleBulkDeleteConfirm}
              disabled={bulkDeleting}
            >
              {bulkDeleting ? "Deleting..." : `Delete ${selectedClips.size} Clip(s)`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Bulk Delete All Dialog */}
      <Dialog open={bulkDeleteAllDialogOpen} onOpenChange={setBulkDeleteAllDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete All Clips</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete ALL {clips.length} clips for this video?
              This action cannot be undone and will delete all clip files and
              thumbnails. The video highlights will be preserved.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setBulkDeleteAllDialogOpen(false)}
              disabled={bulkDeleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDeleteAllConfirm}
              disabled={bulkDeleting}
            >
              {bulkDeleting ? "Deleting..." : `Delete All ${clips.length} Clips`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
