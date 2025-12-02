"use client";

import { useState } from "react";

import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";

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
      analyticsEvents.clipPublishedTikTok({
        clipId: clip.name,
        clipName: clip.name,
        success: true,
      });
    } catch (err: any) {
      frontendLogger.error("TikTok publish failed", err);
      const errorMessage = err.message || "Unknown error";
      log(`TikTok publish failed: ${errorMessage}`, "error");
      alert("TikTok publish failed. Check console for details.");

      // Track failed TikTok publish
      analyticsEvents.clipPublishedFailed({
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
      <div className="col-span-full text-center text-gray-500">
        No clips found. Check logs for errors.
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
      {clips.map((clip, index) => {
        const uniqueId = `clip-${index}`;
        return (
          <div
            key={clip.name}
            className="glass rounded-xl overflow-hidden hover:bg-gray-800 transition-colors group flex flex-col"
          >
            <div className="aspect-[9/16] bg-black relative group-hover:opacity-100 transition-opacity cursor-pointer">
              <video
                id={uniqueId}
                controls
                preload="none"
                className="w-full h-full object-contain"
                poster={clip.thumbnail || undefined}
                src={clip.url}
              />
            </div>
            <div className="p-5 flex-1 flex flex-col">
              <div className="flex items-start justify-between mb-2">
                <h4
                  className="font-bold text-lg leading-tight text-white group-hover:text-blue-400 transition-colors pr-4 break-words line-clamp-2"
                  title={clip.title}
                >
                  {clip.title}
                </h4>
              </div>

              <div className="space-y-3 mb-4 bg-gray-900/50 p-3 rounded-lg border border-gray-700/50">
                <div className="space-y-1">
                  <div className="flex justify-between items-center">
                    <label className="text-[10px] uppercase tracking-wider text-gray-500 font-semibold">
                      Title
                    </label>
                  </div>
                  <textarea
                    id={`${uniqueId}-title-text`}
                    className="w-full bg-gray-800 border border-gray-700 rounded p-2 text-sm text-gray-300 focus:border-blue-500 outline-none"
                    rows={2}
                    defaultValue={clip.title}
                  />
                </div>
                <div className="space-y-1">
                  <div className="flex justify-between items-center">
                    <label className="text-[10px] uppercase tracking-wider text-gray-500 font-semibold">
                      Description
                    </label>
                  </div>
                  <textarea
                    id={`${uniqueId}-desc-text`}
                    className="w-full bg-gray-800 border border-gray-700 rounded p-2 text-sm text-gray-300 focus:border-blue-500 outline-none"
                    rows={4}
                    defaultValue={clip.description}
                  />
                </div>
              </div>

              <div className="mt-auto pt-2 flex gap-3 flex-wrap">
                <a
                  href={clip.url}
                  download
                  onClick={() => {
                    // Extract style from clip name (e.g., clip_01_01_title_split.mp4 -> split)
                    const styleMatch = clip.name.match(/_([^_]+)\.(mp4|jpg)$/);
                    const clipStyle = styleMatch?.[1] ?? "unknown";
                    analyticsEvents.clipDownloaded({
                      clipId: clip.name,
                      clipName: clip.name,
                      style: clipStyle,
                    });
                  }}
                  className="flex-1 bg-blue-600 hover:bg-blue-500 text-white text-center py-2 rounded-lg text-sm font-semibold transition-colors flex items-center justify-center gap-2"
                >
                  <span>⬇️ Download</span>
                  <span className="text-xs opacity-75">({clip.size})</span>
                </a>
                <button
                  onClick={() => {
                    navigator.clipboard.writeText(clip.url);
                    analyticsEvents.clipCopiedLink({
                      clipId: clip.name,
                      clipName: clip.name,
                    });
                  }}
                  className="px-3 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-lg transition-colors"
                  title="Copy Link"
                >
                  🔗
                </button>
                <button
                  onClick={() => {
                    const titleEl = document.getElementById(
                      `${uniqueId}-title-text`
                    ) as HTMLTextAreaElement | null;
                    const descEl = document.getElementById(
                      `${uniqueId}-desc-text`
                    ) as HTMLTextAreaElement | null;
                    publishToTikTok(
                      clip,
                      titleEl?.value || clip.title,
                      descEl?.value || clip.description
                    );
                  }}
                  disabled={publishing === clip.name}
                  className="px-3 bg-purple-600 hover:bg-purple-500 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-lg text-sm font-semibold transition-colors"
                  title="Publish to TikTok"
                >
                  {publishing === clip.name ? "Publishing..." : "▶️ TikTok"}
                </button>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
