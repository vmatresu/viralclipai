/**
 * Results Display Component
 *
 * Displays processing results and clips.
 */

import { useState, useEffect } from "react";
import { Sparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { EditableTitle } from "@/components/EditableTitle";
import { updateVideoTitle } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { toast } from "sonner";

import { ClipGrid, type Clip } from "../ClipGrid";

interface ResultsProps {
  videoId: string;
  clips: Clip[];
  customPromptUsed: string | null;
  videoTitle: string | null;
  videoUrl: string | null;
  log: (msg: string, type?: "info" | "error" | "success") => void;
  onReset: () => void;
  onClipDeleted?: (clipName: string) => void;
  onTitleUpdated?: (newTitle: string) => void;
}

function extractYouTubeId(url: string): string | null {
  try {
    const urlObj = new URL(url);
    // Handle youtu.be short URLs
    if (urlObj.hostname === "youtu.be") {
      return urlObj.pathname.slice(1);
    }
    // Handle youtube.com URLs
    if (urlObj.hostname.includes("youtube.com")) {
      const videoId = urlObj.searchParams.get("v");
      if (videoId) return videoId;
      // Handle /shorts/, /embed/, /v/ paths
      const pathParts = urlObj.pathname.split("/").filter(Boolean);
      if (pathParts.length >= 2 && ["shorts", "embed", "v"].includes(pathParts[0])) {
        return pathParts[1];
      }
    }
  } catch {
    // Invalid URL
  }
  return null;
}

function getDisplayTitle(title: string | null | undefined, videoUrl: string | null | undefined): string {
  // Check for placeholder or empty titles
  if (!title || title.trim() === "") {
    return "Untitled Video";
  }
  
  // Check for common placeholder patterns
  const placeholderPatterns = [
    /^the main title of the/i,
    /^main title/i,
    /^video title/i,
    /^title$/i,
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
}

export function Results({
  videoId,
  clips,
  customPromptUsed,
  videoTitle,
  videoUrl,
  log,
  onReset,
  onClipDeleted,
  onTitleUpdated,
}: ResultsProps) {
  const { getIdToken } = useAuth();
  const [currentTitle, setCurrentTitle] = useState(videoTitle);
  const youtubeId = videoUrl ? extractYouTubeId(videoUrl) : null;
  const displayTitle = getDisplayTitle(currentTitle, videoUrl);

  useEffect(() => {
    setCurrentTitle(videoTitle);
  }, [videoTitle]);

  const handleTitleSave = async (newTitle: string) => {
    try {
      const token = await getIdToken();
      if (!token) {
        throw new Error("Authentication required");
      }
      await updateVideoTitle(videoId, newTitle, token);
      setCurrentTitle(newTitle);
      onTitleUpdated?.(newTitle);
      toast.success("Title updated successfully");
    } catch (error) {
      toast.error("Failed to update title");
      throw error;
    }
  };

  return (
    <section className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold flex items-center gap-2">
          <Sparkles className="h-6 w-6 text-primary" />
          Results
        </h2>
        <Button onClick={onReset} variant="ghost" size="sm">
          Process Another Video
        </Button>
      </div>

      {/* Video Title, URL and Embed */}
      {(videoTitle || videoUrl) && (
        <Card className="glass">
          <CardHeader>
            <CardTitle className="text-sm">Original Video</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {currentTitle && (
              <EditableTitle
                title={currentTitle}
                onSave={handleTitleSave}
              />
            )}
            {videoUrl && (
              <>
                <a
                  href={videoUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-sm text-muted-foreground hover:text-primary transition-colors break-all"
                >
                  {videoUrl}
                </a>
                {youtubeId && (
                  <div className="aspect-video w-full max-w-2xl mx-auto rounded-lg overflow-hidden">
                    <iframe
                      width="100%"
                      height="100%"
                      src={`https://www.youtube.com/embed/${youtubeId}?rel=0`}
                      title="YouTube video player"
                      frameBorder="0"
                      allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                      allowFullScreen
                      className="w-full h-full"
                    />
                  </div>
                )}
              </>
            )}
          </CardContent>
        </Card>
      )}

      {/* Custom Prompt - Made Bigger */}
      {customPromptUsed && (
        <Card className="glass">
          <CardHeader>
            <CardTitle className="text-base font-semibold">Custom Prompt Used</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm whitespace-pre-wrap leading-relaxed">
              {customPromptUsed}
            </p>
          </CardContent>
        </Card>
      )}

      <ClipGrid videoId={videoId} clips={clips} log={log} onClipDeleted={onClipDeleted} />
    </section>
  );
}
