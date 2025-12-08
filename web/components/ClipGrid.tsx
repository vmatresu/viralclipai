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
import { frontendLogger } from "@/lib/logger";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL ?? "";

function cacheBustToken(clip: Clip): string {
  return clip.completed_at ?? clip.updated_at ?? clip.name ?? Date.now().toString();
}

function buildDownloadUrl(clip: Clip): string {
  // Build the base URL (absolute) then append cache-busting param
  const baseUrl = clip.url.startsWith("/")
    ? API_BASE_URL.replace(/\/$/, "") // Remove trailing slash if present
    : "";

  const raw = clip.url.startsWith("/") && baseUrl ? `${baseUrl}${clip.url}` : clip.url;

  const token = cacheBustToken(clip);

  try {
    const url = new URL(
      raw,
      typeof window !== "undefined" ? window.location.origin : undefined
    );
    url.searchParams.set("t", token);
    return url.toString();
  } catch {
    const sep = raw.includes("?") ? "&" : "?";
    return `${raw}${sep}t=${encodeURIComponent(token)}`;
  }
}

// Style name mapping for display
// Note: Audio-based styles (intelligent_audio, intelligent_speaker, etc.) are not available in frontend
// due to compatibility issues with duplicated audio channels (e.g., podcasts)
const STYLE_LABELS: Record<string, string> = {
  split: "Split View",
  split_fast: "Split View (Fast)",
  left_focus: "Left Focus",
  right_focus: "Right Focus",
  intelligent: "Intelligent Crop",
  intelligent_motion: "Intelligent (Motion)",
  intelligent_activity: "Intelligent (Activity)",
  intelligent_split: "Smart Split",
  intelligent_split_motion: "Smart Split (Motion)",
  intelligent_split_activity: "Smart Split (Activity)",
  original: "Original",
};

function getStyleLabel(style?: string): string | null {
  if (!style) return null;
  return STYLE_LABELS[style] || style;
}

interface VideoPlayerProps {
  id: string;
  clip: Clip;
  videoId: string;
  onRef: (el: HTMLVideoElement | null) => void;
  onPlay: () => void;
  getVideoUrl: (clip: Clip) => Promise<string>;
}

function VideoPlayer({ id, clip, onRef, onPlay, getVideoUrl }: VideoPlayerProps) {
  const [videoUrl, setVideoUrl] = useState<string>("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function loadVideo() {
      try {
        setLoading(true);
        const url = await getVideoUrl(clip);
        if (!cancelled) {
          setVideoUrl(url);
        }
      } catch (error) {
        frontendLogger.error("Failed to load video URL", error);
        if (!cancelled) {
          setVideoUrl(""); // Set empty to show error state
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
  }, [clip, getVideoUrl]);

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
      poster={clip.thumbnail ?? undefined}
      src={videoUrl}
      onPlay={onPlay}
    >
      <track kind="captions" />
    </video>
  );
}

export interface Clip {
  name: string;
  title: string;
  description: string;
  url: string;
  direct_url?: string | null; // Presigned R2 URL for faster loading
  thumbnail?: string | null;
  size: string;
  style?: string;
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
  const blobUrls = useRef<{ [key: string]: string }>({});

  // Bulk selection state
  const [selectedClips, setSelectedClips] = useState<Set<string>>(new Set());
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkDeleteDialogOpen, setBulkDeleteDialogOpen] = useState(false);
  const [bulkDeleteAllDialogOpen, setBulkDeleteAllDialogOpen] = useState(false);

  // Cleanup blob URLs on unmount
  useEffect(() => {
    const currentBlobUrls = blobUrls.current;
    return () => {
      Object.values(currentBlobUrls).forEach((url) => {
        URL.revokeObjectURL(url);
      });
    };
  }, []);

  // Function to get video URL - prefer direct_url (presigned R2) for faster loading
  const getVideoUrl = async (clip: Clip): Promise<string> => {
    const clipName = clip.name;

    // If it's already a blob URL, return it
    if (blobUrls.current[clipName]) {
      return blobUrls.current[clipName];
    }

    // Prefer direct_url (presigned R2 URL) for much faster loading
    if (clip.direct_url) {
      return clip.direct_url;
    }

    // Fallback: fetch through backend proxy with auth (slower but works)
    if (clip.url.startsWith("/")) {
      try {
        const token = await getIdToken();
        if (!token) {
          throw new Error("Authentication required");
        }

        const baseUrl = API_BASE_URL.endsWith("/")
          ? API_BASE_URL.slice(0, -1)
          : API_BASE_URL;
        const fullUrl = baseUrl ? `${baseUrl}${clip.url}` : clip.url;

        const response = await fetch(fullUrl, {
          headers: {
            Authorization: `Bearer ${token}`,
          },
        });

        if (!response.ok) {
          throw new Error(`Failed to load video: ${response.statusText}`);
        }

        const blob = await response.blob();
        const blobUrl = URL.createObjectURL(blob);
        blobUrls.current[clipName] = blobUrl;
        return blobUrl;
      } catch (error) {
        frontendLogger.error("Failed to load video", error);
        throw error;
      }
    }

    // If it's already a full URL (presigned URL), return as-is
    return clip.url;
  };

  const handlePlay = (clipName: string) => {
    if (playingClip && playingClip !== clipName) {
      const prevVideo = videoRefs.current[playingClip];
      if (prevVideo) {
        prevVideo.pause();
      }
    }
    setPlayingClip(clipName);
  };

  // Bulk selection functions
  const handleSelectAll = () => {
    if (selectedClips.size === clips.length) {
      setSelectedClips(new Set());
    } else {
      setSelectedClips(new Set(clips.map((clip) => clip.name)));
    }
  };

  const handleClipSelect = (clipName: string) => {
    setSelectedClips((prev) => {
      const next = new Set(prev);
      if (next.has(clipName)) {
        next.delete(clipName);
      } else {
        next.add(clipName);
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

      const result = await bulkDeleteClips(videoId, Array.from(selectedClips), token);

      // Invalidate cache since clips have changed
      void invalidateClipsCache(videoId);

      // Clean up blob URLs for deleted clips
      selectedClips.forEach((clipName) => {
        const blobUrl = blobUrls.current[clipName];
        if (blobUrl) {
          URL.revokeObjectURL(blobUrl);
          delete blobUrls.current[clipName];
        }
      });

      // Stop playing if any selected clip was playing
      if (playingClip && selectedClips.has(playingClip)) {
        const video = videoRefs.current[playingClip];
        if (video) {
          video.pause();
        }
        setPlayingClip(null);
      }

      // Remove video refs for deleted clips
      selectedClips.forEach((clipName) => {
        delete videoRefs.current[clipName];
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

      // Notify parent component
      selectedClips.forEach((clipName) => {
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

      // Clean up blob URLs for all clips
      clips.forEach((clip) => {
        const blobUrl = blobUrls.current[clip.name];
        if (blobUrl) {
          URL.revokeObjectURL(blobUrl);
          delete blobUrls.current[clip.name];
        }
      });

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

    setDeletingClip(clipToDelete.name);
    try {
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in to delete clips.");
        return;
      }

      await deleteClip(videoId, clipToDelete.name, token);

      // Invalidate cache since clips have changed (fire and forget)
      void invalidateClipsCache(videoId);

      // Clean up blob URL if it exists
      const blobUrl = blobUrls.current[clipToDelete.name];
      if (blobUrl) {
        URL.revokeObjectURL(blobUrl);
        delete blobUrls.current[clipToDelete.name];
      }

      // Stop playing if this clip was playing
      if (playingClip === clipToDelete.name) {
        const video = videoRefs.current[clipToDelete.name];
        if (video) {
          video.pause();
        }
        setPlayingClip(null);
      }

      // Remove video ref
      delete videoRefs.current[clipToDelete.name];

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
    try {
      setPublishing(clip.name);
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
        clipId: clip.name,
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
        clipId: clip.name,
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
          const isPlaying = playingClip === clip.name;
          const isSelected = selectedClips.has(clip.name);

          return (
            <Card
              key={clip.name}
              className={`bg-card border-border/50 overflow-hidden hover:shadow-xl hover:border-primary/20 transition-all group flex flex-col rounded-xl ${
                isSelected ? "ring-2 ring-primary" : ""
              }`}
            >
              {/* Video Player Area */}
              <div className="relative aspect-[9/16] bg-black group-hover:opacity-100 transition-opacity">
                <VideoPlayer
                  id={uniqueId}
                  clip={clip}
                  videoId={videoId}
                  onRef={(el) => {
                    videoRefs.current[clip.name] = el;
                  }}
                  onPlay={() => handlePlay(clip.name)}
                  getVideoUrl={getVideoUrl}
                />

                {/* Custom Play Button Overlay (only visible when paused) */}
                {!isPlaying && (
                  <div
                    className="absolute inset-0 flex items-center justify-center bg-black/20 group-hover:bg-black/10 transition-colors cursor-pointer"
                    onClick={() => {
                      const video = videoRefs.current[clip.name];
                      if (video) void video.play();
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        const video = videoRefs.current[clip.name];
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
                        onClick={() => handleClipSelect(clip.name)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            handleClipSelect(clip.name);
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
                          <div className="mt-2 inline-flex items-center gap-1.5 px-2.5 py-1 bg-primary/10 text-primary border border-primary/20 rounded-md">
                            <Film className="h-3 w-3" />
                            <span className="text-xs font-medium">
                              {getStyleLabel(clip.style) ?? clip.style}
                            </span>
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
                    asChild
                    variant="outline"
                    className="flex-1 gap-2 hover:bg-primary hover:text-primary-foreground transition-colors"
                    onClick={() => {
                      const styleMatch = clip.name.match(/_([^_]+)\.(mp4|jpg)$/);
                      const clipStyle = styleMatch?.[1] ?? "unknown";
                      void analyticsEvents.clipDownloaded({
                        clipId: clip.name,
                        clipName: clip.name,
                        style: clipStyle,
                      });
                    }}
                  >
                    <a
                      href={buildDownloadUrl(clip)}
                      download
                      onClick={async (e) => {
                        // For relative URLs, we need to fetch with auth first
                        if (clip.url.startsWith("/")) {
                          e.preventDefault();
                          try {
                            const token = await getIdToken();
                            if (!token) {
                              toast.error("Please sign in to download clips.");
                              return;
                            }
                            const baseUrl = API_BASE_URL.endsWith("/")
                              ? API_BASE_URL.slice(0, -1)
                              : API_BASE_URL;
                            const fullUrl = baseUrl
                              ? `${baseUrl}${clip.url}`
                              : clip.url;
                            const response = await fetch(
                              buildDownloadUrl({ ...clip, url: fullUrl }),
                              {
                                headers: {
                                  Authorization: `Bearer ${token}`,
                                },
                              }
                            );
                            if (!response.ok) {
                              throw new Error("Failed to download");
                            }
                            const blob = await response.blob();
                            const blobUrl = URL.createObjectURL(blob);
                            const a = document.createElement("a");
                            a.href = blobUrl;
                            a.download = clip.name;
                            document.body.appendChild(a);
                            a.click();
                            document.body.removeChild(a);
                            URL.revokeObjectURL(blobUrl);
                          } catch (error) {
                            frontendLogger.error("Download failed", error);
                            toast.error("Failed to download clip");
                          }
                        }
                      }}
                    >
                      <Download className="h-4 w-4" />
                      Download
                    </a>
                  </Button>

                  <Button
                    variant="secondary"
                    size="icon"
                    className="shrink-0"
                    onClick={() => {
                      const urlToCopy = buildDownloadUrl(clip);
                      void navigator.clipboard.writeText(urlToCopy);
                      toast.success("Link copied to clipboard");
                      void analyticsEvents.clipCopiedLink({
                        clipId: clip.name,
                        clipName: clip.name,
                      });
                    }}
                    title="Copy Link"
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
