"use client";

import { Download, Link2, Play, Share2, UploadCloud } from "lucide-react";
import { useState, useRef } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { toast } from "sonner";

export interface Clip {
  name: string;
  title: string;
  description: string;
  url: string;
  thumbnail?: string | null;
  size: string;
}

interface ClipGridProps {
  videoId: string;
  clips: Clip[];
  log: (msg: string, type?: "info" | "error" | "success") => void;
}

export function ClipGrid({ videoId, clips, log }: ClipGridProps) {
  const { getIdToken } = useAuth();
  const [publishing, setPublishing] = useState<string | null>(null);
  const [playingClip, setPlayingClip] = useState<string | null>(null);
  const videoRefs = useRef<{ [key: string]: HTMLVideoElement | null }>({});

  const handlePlay = (clipName: string) => {
    if (playingClip && playingClip !== clipName) {
      const prevVideo = videoRefs.current[playingClip];
      if (prevVideo) {
        prevVideo.pause();
      }
    }
    setPlayingClip(clipName);
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
    <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
      {clips.map((clip, index) => {
        const uniqueId = `clip-${index}`;
        const isPlaying = playingClip === clip.name;

        return (
          <Card
            key={clip.name}
            className="bg-card border-border/50 overflow-hidden hover:shadow-xl hover:border-primary/20 transition-all group flex flex-col rounded-xl"
          >
            {/* Video Player Area */}
            <div className="relative aspect-[9/16] bg-black group-hover:opacity-100 transition-opacity">
              <video
                id={uniqueId}
                ref={(el) => { videoRefs.current[clip.name] = el; }}
                controls
                preload="metadata"
                className="w-full h-full object-contain"
                poster={clip.thumbnail ?? undefined}
                src={clip.url}
                onPlay={() => handlePlay(clip.name)}
              >
                <track kind="captions" />
              </video>
              
              {/* Custom Play Button Overlay (only visible when paused) */}
              {!isPlaying && (
                <div 
                  className="absolute inset-0 flex items-center justify-center bg-black/20 group-hover:bg-black/10 transition-colors cursor-pointer"
                  onClick={() => {
                    const video = videoRefs.current[clip.name];
                    if (video) video.play();
                  }}
                >
                  <div className="w-16 h-16 rounded-full bg-white/90 backdrop-blur-sm flex items-center justify-center pl-1 shadow-lg transform group-hover:scale-110 transition-transform">
                    <Play className="h-8 w-8 text-primary fill-primary" />
                  </div>
                </div>
              )}
            </div>

            <CardContent className="p-6 flex-1 flex flex-col gap-4">
              {/* Header: Title & Badges */}
              <div>
                <div className="flex items-start justify-between gap-4 mb-2">
                  <h4
                    className="font-bold text-xl leading-tight text-foreground group-hover:text-primary transition-colors line-clamp-2"
                    title={clip.title}
                  >
                    {clip.title}
                  </h4>
                  <span className="px-2 py-1 text-xs font-medium bg-secondary text-secondary-foreground rounded-md whitespace-nowrap">
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
                  <a href={clip.url} download>
                    <Download className="h-4 w-4" />
                    Download
                  </a>
                </Button>
                
                <Button
                  variant="secondary"
                  size="icon"
                  className="shrink-0"
                  onClick={() => {
                    void navigator.clipboard.writeText(clip.url);
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
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}