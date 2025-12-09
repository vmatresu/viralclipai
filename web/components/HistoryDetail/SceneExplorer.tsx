"use client";

import { AlertCircle, Download, ExternalLink, Link2, Play } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useAuth } from "@/lib/auth";
import {
  getStyleLabel,
  getStyleTier,
  getTierBadgeClasses,
  normalizeStyleForSelection,
  type TierColor,
} from "@/lib/styleTiers";
import { cn } from "@/lib/utils";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_BASE_URL ?? "";

export type HistoryClip = {
  id: string;
  sceneId: number;
  sceneTitle?: string;
  startSec: number;
  endSec: number;
  style: string;
  thumbnailUrl: string;
  videoUrl: string;
  directUrl?: string | null;
  title?: string;
};

export type SceneGroup = {
  sceneId: number;
  sceneTitle: string;
  startSec: number;
  endSec: number;
  clips: HistoryClip[];
};

const tierWeight: Record<TierColor, number> = {
  static: 0,
  motion: 1,
  basic: 2,
  premium: 3,
  legacy: 4,
};

function formatSeconds(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

function formatRange(scene: SceneGroup): string {
  const start = scene.startSec ? formatSeconds(scene.startSec) : "0:00";
  const end = scene.endSec ? formatSeconds(scene.endSec) : "";
  return end ? `${start} – ${end}` : start;
}

export function groupClipsByScene(clips: HistoryClip[]): SceneGroup[] {
  const groups = new Map<number, SceneGroup>();

  clips.forEach((clip) => {
    const existing = groups.get(clip.sceneId);
    const sceneTitle = clip.sceneTitle ?? `Scene ${clip.sceneId}`;
    if (!existing) {
      groups.set(clip.sceneId, {
        sceneId: clip.sceneId,
        sceneTitle,
        startSec: clip.startSec,
        endSec: clip.endSec,
        clips: [clip],
      });
      return;
    }

    existing.clips.push(clip);
    existing.startSec = Math.min(existing.startSec || clip.startSec, clip.startSec);
    existing.endSec = Math.max(existing.endSec || clip.endSec, clip.endSec);
  });

  return Array.from(groups.values()).sort(
    (a, b) => a.startSec - b.startSec || a.sceneId - b.sceneId
  );
}

interface HistorySceneExplorerProps {
  scenes: SceneGroup[];
}

export function HistorySceneExplorer({ scenes }: HistorySceneExplorerProps) {
  const { getIdToken } = useAuth();
  const blobUrls = useRef<Record<string, string>>({});

  useEffect(() => {
    const current = blobUrls.current;
    return () => {
      Object.values(current).forEach((url) => URL.revokeObjectURL(url));
    };
  }, []);

  const resolvePlaybackUrl = useCallback(
    async (clip: HistoryClip): Promise<string> => {
      const cached = blobUrls.current[clip.id];
      if (cached !== undefined) {
        return cached;
      }

      const rawUrl = clip.directUrl ?? clip.videoUrl;
      if (!rawUrl) throw new Error("Missing video URL");

      // If already an absolute URL, return as-is
      if (/^https?:\/\//.test(rawUrl)) {
        return rawUrl;
      }

      const token = await getIdToken();
      const baseUrl = API_BASE_URL.endsWith("/")
        ? API_BASE_URL.slice(0, -1)
        : API_BASE_URL;
      const fullUrl = rawUrl.startsWith("/")
        ? `${baseUrl}${rawUrl}`
        : `${baseUrl}/${rawUrl}`;

      try {
        const response = await fetch(fullUrl, {
          headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        });

        if (!response.ok) {
          throw new Error(`Failed to load video (${response.status})`);
        }

        const blob = await response.blob();
        const blobUrl = URL.createObjectURL(blob);
        blobUrls.current[clip.id] = blobUrl;
        return blobUrl;
      } catch (error) {
        console.error("Failed to resolve playback URL", error);
        return fullUrl;
      }
    },
    [getIdToken]
  );

  const sortedScenes = useMemo(
    () => [...scenes].sort((a, b) => a.startSec - b.startSec || a.sceneId - b.sceneId),
    [scenes]
  );

  return (
    <div className="space-y-4">
      {sortedScenes.length === 0 ? (
        <Card>
          <CardContent className="flex items-center gap-3 py-8">
            <AlertCircle className="h-5 w-5 text-muted-foreground" />
            <div className="space-y-1">
              <p className="font-medium">No clips available</p>
              <p className="text-sm text-muted-foreground">
                Generate clips to explore them by scene and style.
              </p>
            </div>
          </CardContent>
        </Card>
      ) : (
        <Accordion type="multiple" className="space-y-3">
          {sortedScenes.map((scene, index) => (
            <HistorySceneItem
              key={scene.sceneId}
              scene={scene}
              index={index}
              resolvePlaybackUrl={resolvePlaybackUrl}
            />
          ))}
        </Accordion>
      )}
    </div>
  );
}

interface HistorySceneItemProps {
  scene: SceneGroup;
  index: number;
  resolvePlaybackUrl: (clip: HistoryClip) => Promise<string>;
}

function HistorySceneItem({ scene, index, resolvePlaybackUrl }: HistorySceneItemProps) {
  const canonicalizeStyle = useCallback((style?: string) => {
    const trimmed = style?.trim() ?? "";
    const normalized = normalizeStyleForSelection(trimmed);
    return (normalized ?? trimmed).toLowerCase();
  }, []);

  const clipsByStyle = useMemo(() => {
    const map = new Map<string, HistoryClip>();
    scene.clips.forEach((clip) => {
      const key = canonicalizeStyle(clip.style) || "unknown";
      if (!map.has(key)) {
        map.set(key, clip);
      }
    });
    return map;
  }, [scene.clips, canonicalizeStyle]);

  const styles = useMemo(() => {
    return Array.from(clipsByStyle.keys()).sort((a, b) => {
      const tierA = getStyleTier(a)?.color ?? "legacy";
      const tierB = getStyleTier(b)?.color ?? "legacy";
      if (tierWeight[tierA] === tierWeight[tierB]) {
        return a.localeCompare(b);
      }
      return tierWeight[tierA] - tierWeight[tierB];
    });
  }, [clipsByStyle]);

  const [activeStyle, setActiveStyle] = useState<string>(styles[0] ?? "");

  useEffect(() => {
    if (styles.length > 0) {
      setActiveStyle(styles[0] ?? "");
    } else {
      setActiveStyle("");
    }
  }, [styles]);

  const tierSummaries = useMemo(() => {
    const seen = new Map<TierColor, string>();
    scene.clips.forEach((clip) => {
      const meta = getStyleTier(clip.style);
      if (meta && !seen.has(meta.color)) {
        seen.set(meta.color, meta.label);
      }
    });
    return Array.from(seen.entries()).map(([color, label]) => ({ color, label }));
  }, [scene.clips]);

  const firstThumbnail = scene.clips.find((c) => c.thumbnailUrl)?.thumbnailUrl;

  return (
    <AccordionItem
      value={`scene-${scene.sceneId}`}
      className="rounded-lg border bg-muted/30 px-3"
    >
      <AccordionTrigger className="py-3">
        <div className="flex w-full items-center gap-4">
          <div className="flex-1 space-y-2 text-left">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="outline">Scene {index + 1}</Badge>
              <span className="text-sm text-muted-foreground">
                {formatRange(scene)}
              </span>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <p className="font-semibold text-base leading-tight flex-1">
                {scene.sceneTitle}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary">{styles.length} styles</Badge>
              {tierSummaries.map((tier) => (
                <Badge
                  key={tier.color}
                  className={cn("border", getTierBadgeClasses(tier.color))}
                  variant="outline"
                >
                  {tier.label}
                </Badge>
              ))}
            </div>
          </div>
          {firstThumbnail ? (
            <div className="hidden sm:block">
              <img
                src={firstThumbnail}
                alt={`Scene ${scene.sceneId} thumbnail`}
                className="h-16 w-28 rounded-md object-cover shadow-sm ring-1 ring-border"
              />
            </div>
          ) : null}
        </div>
      </AccordionTrigger>
      <AccordionContent>
        <div className="rounded-lg border bg-background/60 p-4 shadow-sm">
          <Tabs value={activeStyle} onValueChange={setActiveStyle}>
            <TabsList className="w-full flex flex-wrap gap-2">
              {styles.map((style) => {
                const meta = getStyleTier(style);
                return (
                  <TabsTrigger
                    key={style}
                    value={style}
                    className={cn(
                      "flex items-center gap-2 rounded-full px-0 py-0 bg-transparent shadow-none hover:bg-transparent",
                      "data-[state=active]:bg-transparent data-[state=active]:shadow-none"
                    )}
                  >
                    <Badge
                      className={cn(
                        "border",
                        getTierBadgeClasses(meta?.color ?? "legacy")
                      )}
                      variant="outline"
                    >
                      {meta?.label ?? "Legacy"}
                    </Badge>
                  </TabsTrigger>
                );
              })}
            </TabsList>

            {styles.map((style) => {
              const clip = clipsByStyle.get(style);
              if (!clip) return null;
              const meta = getStyleTier(style);

              return (
                <TabsContent key={style} value={style} className="mt-4 space-y-4">
                  <div className="grid gap-4 md:grid-cols-[2fr_1.2fr]">
                    <Card className="overflow-hidden">
                      <CardContent className="space-y-3 p-3 md:p-4">
                        <SceneClipPlayer
                          clip={clip}
                          resolvePlaybackUrl={resolvePlaybackUrl}
                        />
                        <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                          <Badge
                            className={cn(
                              "border",
                              getTierBadgeClasses(meta?.color ?? "legacy")
                            )}
                            variant="outline"
                          >
                            {meta?.label ?? "Legacy"}
                          </Badge>
                          <span className="font-medium text-foreground">
                            {getStyleLabel(style) ?? style}
                          </span>
                          <span>• Scene {index + 1}</span>
                          <span>• {formatRange(scene)}</span>
                        </div>
                        <div className="flex flex-wrap gap-2">
                          <ActionButton
                            icon={<Download className="h-4 w-4" />}
                            label="Download"
                            onClick={() => window.open(clip.videoUrl, "_blank")}
                          />
                          <ActionButton
                            icon={<Link2 className="h-4 w-4" />}
                            label="Copy link"
                            onClick={async () => {
                              try {
                                await navigator.clipboard.writeText(clip.videoUrl);
                                toast.success("Clip link copied");
                              } catch {
                                toast.error("Failed to copy link");
                              }
                            }}
                          />
                          <ActionButton
                            icon={<ExternalLink className="h-4 w-4" />}
                            label="Open"
                            onClick={() => window.open(clip.videoUrl, "_blank")}
                          />
                          <ActionButton
                            icon={<Play className="h-4 w-4" />}
                            label="Play in new tab"
                            onClick={async () => {
                              try {
                                const url = await resolvePlaybackUrl(clip);
                                window.open(url, "_blank");
                              } catch {
                                window.open(clip.videoUrl, "_blank");
                              }
                            }}
                          />
                        </div>
                      </CardContent>
                    </Card>

                    <Card className="border-dashed">
                      <CardHeader className="pb-2">
                        <CardTitle className="text-sm">Styles in this scene</CardTitle>
                        <p className="text-xs text-muted-foreground">
                          Switch styles quickly or jump to a thumbnail.
                        </p>
                      </CardHeader>
                      <CardContent className="space-y-3">
                        <div className="grid gap-3 sm:grid-cols-2">
                          {styles.map((s) => {
                            const thumbClip = clipsByStyle.get(s);
                            const thumbMeta = getStyleTier(s);
                            if (!thumbClip) return null;
                            return (
                              <button
                                key={s}
                                onClick={() => setActiveStyle(s)}
                                className={cn(
                                  "group relative overflow-hidden rounded-md border text-left transition",
                                  activeStyle === s
                                    ? "border-primary ring-2 ring-primary/40"
                                    : "hover:border-primary/50"
                                )}
                              >
                                {thumbClip.thumbnailUrl ? (
                                  <img
                                    src={thumbClip.thumbnailUrl}
                                    alt={`${getStyleLabel(s) ?? s} thumbnail`}
                                    className="h-28 w-full object-cover"
                                  />
                                ) : (
                                  <div className="flex h-28 w-full items-center justify-center bg-muted">
                                    <span className="text-xs text-muted-foreground">
                                      No thumbnail
                                    </span>
                                  </div>
                                )}
                                <div className="absolute left-2 top-2">
                                  <Badge
                                    className={cn(
                                      "border",
                                      getTierBadgeClasses(thumbMeta?.color ?? "legacy")
                                    )}
                                    variant="outline"
                                  >
                                    {thumbMeta?.label ?? "Legacy"}
                                  </Badge>
                                </div>
                                <div className="space-y-1 p-2">
                                  <p className="text-xs font-semibold leading-tight">
                                    {getStyleLabel(s) ?? s}
                                  </p>
                                  <p className="text-[11px] text-muted-foreground">
                                    {formatRange(scene)}
                                  </p>
                                </div>
                              </button>
                            );
                          })}
                        </div>
                      </CardContent>
                    </Card>
                  </div>
                </TabsContent>
              );
            })}
          </Tabs>
        </div>
      </AccordionContent>
    </AccordionItem>
  );
}

interface SceneClipPlayerProps {
  clip: HistoryClip;
  resolvePlaybackUrl: (clip: HistoryClip) => Promise<string>;
}

function SceneClipPlayer({ clip, resolvePlaybackUrl }: SceneClipPlayerProps) {
  const [videoUrl, setVideoUrl] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setError(null);
      try {
        const url = await resolvePlaybackUrl(clip);
        if (!cancelled) setVideoUrl(url);
      } catch (err) {
        console.error("Failed to load clip", err);
        if (!cancelled) setError("Unable to load video");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [clip, resolvePlaybackUrl]);

  if (loading) {
    return (
      <div className="flex h-[280px] items-center justify-center rounded-md bg-muted/50 text-sm text-muted-foreground">
        Loading video...
      </div>
    );
  }

  if (error || !videoUrl) {
    return (
      <div className="flex h-[280px] flex-col items-center justify-center gap-2 rounded-md bg-muted/50 text-sm text-muted-foreground">
        <AlertCircle className="h-5 w-5" />
        <span>{error ?? "Video unavailable"}</span>
      </div>
    );
  }

  return (
    <div className="relative overflow-hidden rounded-lg border">
      <video
        src={videoUrl}
        controls
        className="h-full w-full bg-black"
        poster={clip.thumbnailUrl || undefined}
        preload="metadata"
      >
        <track kind="captions" />
      </video>
    </div>
  );
}

interface ActionButtonProps {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}

function ActionButton({ icon, label, onClick }: ActionButtonProps) {
  return (
    <Button variant="outline" size="sm" className="gap-2" onClick={onClick}>
      {icon}
      <span>{label}</span>
    </Button>
  );
}
