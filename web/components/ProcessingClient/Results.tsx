/**
 * Results Display Component
 *
 * Displays processing results and clips.
 */

import { Sparkles } from "lucide-react";
import { useState, useEffect } from "react";
import { toast } from "sonner";

import { EditableTitle } from "@/components/EditableTitle";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { updateVideoTitle } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";

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
      if (
        pathParts.length >= 2 &&
        pathParts[0] &&
        ["shorts", "embed", "v"].includes(pathParts[0])
      ) {
        return pathParts[1] || null;
      }
    }
  } catch {
    // Invalid URL
  }
  return null;
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
              <EditableTitle title={currentTitle} onSave={handleTitleSave} />
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
            <CardTitle className="text-base font-semibold">
              Custom Prompt Used
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm whitespace-pre-wrap leading-relaxed">
              {customPromptUsed}
            </p>
          </CardContent>
        </Card>
      )}

      <ClipGrid
        videoId={videoId}
        clips={clips}
        log={log}
        onClipDeleted={onClipDeleted}
      />
    </section>
  );
}
