"use client";

import { useState } from "react";
import { Download, Link2, Play } from "lucide-react";

import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";

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

  async function publishToTikTok(clip: Clip, title: string, description: string) {
    try {
      setPublishing(clip.name);
      const token = await getIdToken();
      if (!token) {
        // eslint-disable-next-line no-alert
        alert("Please sign in to publish clips to TikTok.");
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
      // eslint-disable-next-line no-alert
      alert("TikTok publish failed. Check console for details.");

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
      <div className="col-span-full text-center text-muted-foreground py-8">
        No clips found. Check logs for errors.
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
      {clips.map((clip, index) => {
        const uniqueId = `clip-${index}`;
        return (
          <Card
            key={clip.name}
            className="glass overflow-hidden hover:shadow-lg transition-all group flex flex-col"
          >
            <div className="aspect-[9/16] bg-black relative group-hover:opacity-100 transition-opacity cursor-pointer">
              <video
                id={uniqueId}
                controls
                preload="none"
                className="w-full h-full object-contain"
                poster={clip.thumbnail ?? undefined}
                src={clip.url}
              >
                <track kind="captions" />
              </video>
            </div>
            <CardContent className="p-5 flex-1 flex flex-col">
              <div className="flex items-start justify-between mb-4">
                <h4
                  className="font-bold text-lg leading-tight group-hover:text-primary transition-colors pr-4 break-words line-clamp-2"
                  title={clip.title}
                >
                  {clip.title}
                </h4>
              </div>

              <div className="space-y-3 mb-4 bg-muted/50 p-3 rounded-lg border">
                <div className="space-y-2">
                  <Label
                    htmlFor={`${uniqueId}-title-text`}
                    className="text-xs uppercase tracking-wider"
                  >
                    Title
                  </Label>
                  <Textarea
                    id={`${uniqueId}-title-text`}
                    rows={2}
                    defaultValue={clip.title}
                    className="resize-none"
                  />
                </div>
                <div className="space-y-2">
                  <Label
                    htmlFor={`${uniqueId}-desc-text`}
                    className="text-xs uppercase tracking-wider"
                  >
                    Description
                  </Label>
                  <Textarea
                    id={`${uniqueId}-desc-text`}
                    rows={4}
                    defaultValue={clip.description}
                    className="resize-none"
                  />
                </div>
              </div>

              <div className="mt-auto pt-2 flex gap-2 flex-wrap">
                <Button
                  asChild
                  variant="default"
                  className="flex-1 gap-2"
                  onClick={() => {
                    // Extract style from clip name (e.g., clip_01_01_title_split.mp4 -> split)
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
                    <span>Download</span>
                    <span className="text-xs opacity-75">({clip.size})</span>
                  </a>
                </Button>
                <Button
                  variant="secondary"
                  size="icon"
                  onClick={() => {
                    void navigator.clipboard.writeText(clip.url);
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
                  className="gap-2"
                  title="Publish to TikTok"
                >
                  <Play className="h-4 w-4" />
                  {publishing === clip.name ? "Publishing..." : "TikTok"}
                </Button>
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
