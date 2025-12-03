"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import { useParams, useRouter } from "next/navigation";
import { ArrowLeft, AlertCircle, Play, Sparkles } from "lucide-react";
import Link from "next/link";

import { apiFetch, getVideoHighlights } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useReprocessing } from "@/hooks/useReprocessing";
import { SceneCard, type Highlight } from "@/components/HistoryDetail/SceneCard";
import { StyleSelector } from "@/components/HistoryDetail/StyleSelector";
import { ProcessingStatus } from "@/components/HistoryDetail/ProcessingStatus";

interface HighlightsData {
  video_id: string;
  video_url?: string;
  video_title?: string;
  highlights: Highlight[];
}

export default function HistoryDetailPage() {
  const params = useParams();
  const router = useRouter();
  const videoId = params.id as string;
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [highlightsData, setHighlightsData] = useState<HighlightsData | null>(
    null
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedScenes, setSelectedScenes] = useState<Set<number>>(new Set());
  const [selectedStyles, setSelectedStyles] = useState<Set<string>>(new Set());
  const [isProcessing, setIsProcessing] = useState(false);

  const {
    isProcessing: isReprocessing,
    reprocess,
    cancel,
  } = useReprocessing({
    videoId,
    onComplete: () => {
      setIsProcessing(false);
      void loadHighlights();
      router.push(`/?id=${encodeURIComponent(videoId)}`);
    },
    onError: () => {
      setIsProcessing(false);
    },
  });

  const loadHighlights = useCallback(async () => {
    if (authLoading || !user) {
      setLoading(false);
      return;
    }

    try {
      const token = await getIdToken();
      if (!token) {
        throw new Error("Failed to get authentication token");
      }
      const data = await getVideoHighlights(videoId, token);
      setHighlightsData(data);
      setError(null);
    } catch (err: unknown) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to load highlights";
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  }, [getIdToken, user, authLoading, videoId]);

  useEffect(() => {
    void loadHighlights();
  }, [loadHighlights]);

  // Check if video is processing with proper cleanup
  useEffect(() => {
    if (!user || !videoId) return;

    let cancelled = false;

    const checkStatus = async () => {
      if (cancelled) return;

      try {
        const token = await getIdToken();
        if (!token || cancelled) return;

        const data = await apiFetch<{
          videos: Array<{ video_id?: string; id?: string; status?: string }>;
        }>("/api/user/videos", { token });

        if (cancelled) return;

        const video = data.videos.find(
          (v) => (v.video_id ?? v.id) === videoId
        );
        setIsProcessing(video?.status === "processing");
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to check video status:", err);
        }
      }
    };

    void checkStatus();
    const interval = setInterval(checkStatus, 5000);

    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [user, videoId, getIdToken]);

  const handleSceneToggle = useCallback((sceneId: number) => {
    setSelectedScenes((prev) => {
      const next = new Set(prev);
      if (next.has(sceneId)) {
        next.delete(sceneId);
      } else {
        next.add(sceneId);
      }
      return next;
    });
  }, []);

  const handleStyleToggle = useCallback((style: string) => {
    setSelectedStyles((prev) => {
      const next = new Set(prev);
      if (next.has(style)) {
        next.delete(style);
      } else {
        next.add(style);
      }
      return next;
    });
  }, []);

  const handleReprocess = useCallback(async () => {
    if (selectedScenes.size === 0 || selectedStyles.size === 0) {
      toast.error("Please select at least one scene and one style");
      return;
    }

    if (isProcessing || isReprocessing) {
      toast.error("Video is currently processing. Please wait for it to complete.");
      return;
    }

    setIsProcessing(true);
    await reprocess(Array.from(selectedScenes), Array.from(selectedStyles));
  }, [selectedScenes, selectedStyles, isProcessing, isReprocessing, reprocess]);

  const formatTime = useCallback((timeStr: string): string => {
    // Handle HH:MM:SS format
    const parts = timeStr.split(":");
    if (parts.length === 3) {
      const [h, m, s] = parts;
      const totalSeconds =
        parseInt(h) * 3600 + parseInt(m) * 60 + parseFloat(s);
      const minutes = Math.floor(totalSeconds / 60);
      const seconds = Math.floor(totalSeconds % 60);
      return `${minutes}:${seconds.toString().padStart(2, "0")}`;
    }
    return timeStr;
  }, []);

  const totalClipsToGenerate = useMemo(() => {
    return selectedScenes.size * selectedStyles.size;
  }, [selectedScenes.size, selectedStyles.size]);

  const canReprocess = useMemo(() => {
    return (
      selectedScenes.size > 0 &&
      selectedStyles.size > 0 &&
      !isProcessing &&
      !isReprocessing
    );
  }, [selectedScenes.size, selectedStyles.size, isProcessing, isReprocessing]);

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
          <AlertCircle className="h-12 w-12 text-muted-foreground" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">Sign in required</h2>
          <p className="text-muted-foreground max-w-md">
            Please sign in to view video highlights.
          </p>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
        <p className="text-muted-foreground">Loading highlights...</p>
      </div>
    );
  }

  if (error || !highlightsData) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4 text-center">
        <AlertCircle className="h-12 w-12 text-destructive" />
        <div className="space-y-2">
          <h3 className="text-xl font-semibold">Failed to load highlights</h3>
          <p className="text-muted-foreground">{error || "Highlights not found"}</p>
        </div>
        <Button variant="outline" onClick={() => router.back()}>
          Go Back
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6 page-container">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" onClick={() => router.back()}>
          <ArrowLeft className="h-4 w-4" />
        </Button>
        <div className="flex-1">
          <h1 className="text-3xl font-bold tracking-tight">
            {highlightsData.video_title || "Video Highlights"}
          </h1>
          {highlightsData.video_url && (
            <p className="text-sm text-muted-foreground mt-1 truncate">
              {highlightsData.video_url}
            </p>
          )}
        </div>
      </div>

      {(isProcessing || isReprocessing) && (
        <ProcessingStatus videoId={videoId} />
      )}

      <Card className="glass">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            Select Scenes to Reprocess
          </CardTitle>
          <CardDescription>
            Choose scenes and styles to generate new clips. This feature is
            available for Pro and Enterprise plans.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <StyleSelector
            selectedStyles={selectedStyles}
            disabled={isProcessing || isReprocessing}
            onStyleToggle={handleStyleToggle}
          />

          <div className="space-y-3">
            <h3 className="text-sm font-semibold">
              Select Scenes ({selectedScenes.size} selected)
            </h3>
            <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
              {highlightsData.highlights.map((highlight) => (
                <SceneCard
                  key={highlight.id}
                  highlight={highlight}
                  selected={selectedScenes.has(highlight.id)}
                  disabled={isProcessing || isReprocessing}
                  onToggle={handleSceneToggle}
                  formatTime={formatTime}
                />
              ))}
            </div>
          </div>

          <div className="flex items-center justify-between pt-4 border-t">
            <p className="text-sm text-muted-foreground">
              {canReprocess
                ? `Will generate ${totalClipsToGenerate} new clip(s)`
                : "Select scenes and styles to reprocess"}
            </p>
            <Button
              onClick={handleReprocess}
              disabled={!canReprocess}
              size="lg"
            >
              <Play className="h-4 w-4 mr-2" />
              Reprocess Selected
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card className="glass">
        <CardHeader>
          <CardTitle>Existing Clips</CardTitle>
          <CardDescription>
            View all clips generated for this video
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild variant="outline">
            <Link href={`/?id=${encodeURIComponent(videoId)}`}>
              View All Clips
            </Link>
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
