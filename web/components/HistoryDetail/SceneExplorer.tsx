"use client";

import * as AccordionPrimitive from "@radix-ui/react-accordion";
import {
  AlertCircle,
  ChevronDown,
  Copy,
  Download,
  ExternalLink,
  ImageIcon,
  Link2,
  Pencil,
  Share2,
  Trash,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { Accordion, AccordionContent, AccordionItem } from "@/components/ui/accordion";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { updateClipTitle } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import {
  copyShareUrl,
  downloadClip,
  getPlaybackUrl,
  getThumbnailUrl,
} from "@/lib/clipDelivery";
import {
  getStyleLabel,
  getStyleTier,
  getTierBadgeClasses,
  normalizeStyleForSelection,
  type TierColor,
} from "@/lib/styleTiers";
import { cn } from "@/lib/utils";

import { formatBytes, parseSizeToBytes } from "../../types/storage";

import { buildHighlightCopyText, type Highlight } from "./SceneCard";

export type HistoryClip = {
  id: string;
  sceneId: number;
  sceneTitle?: string;
  startSec: number;
  endSec: number;
  style: string;
  clipName?: string;
  title?: string;
  size?: string;
};

export type SceneGroup = {
  sceneId: number;
  sceneTitle: string;
  startSec: number;
  endSec: number;
  clips: HistoryClip[];
  /** Total size of all clips in this scene in bytes. */
  totalSizeBytes?: number;
  /** Full highlight info for this scene (title, description, reason, etc.) */
  highlight?: Highlight;
};

function getTierWeight(color: TierColor): number {
  switch (color) {
    case "static":
      return 0;
    case "motion":
      return 1;
    case "basic":
      return 2;
    case "premium":
      return 3;
    case "legacy":
    default:
      return 4;
  }
}

function formatSeconds(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  if (hours > 0) {
    return `${hours}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  }
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

function formatRange(scene: SceneGroup): string {
  const start = scene.startSec ? formatSeconds(scene.startSec) : "0:00";
  const end = scene.endSec ? formatSeconds(scene.endSec) : "";
  return end ? `${start} – ${end}` : start;
}

export function groupClipsByScene(
  clips: HistoryClip[],
  highlights?: Highlight[]
): SceneGroup[] {
  const groups = new Map<number, SceneGroup>();
  const highlightMap = new Map<number, Highlight>();

  // Build highlight lookup by id
  highlights?.forEach((h) => highlightMap.set(h.id, h));

  clips.forEach((clip) => {
    const existing = groups.get(clip.sceneId);
    // Use "Compilations" for scene_id=0, otherwise use scene title from clip or fallback
    const sceneTitle =
      clip.sceneId === 0
        ? (clip.sceneTitle ?? "Compilations")
        : (clip.sceneTitle ?? `Scene ${clip.sceneId}`);
    const clipSizeBytes = parseSizeToBytes(clip.size);

    if (!existing) {
      groups.set(clip.sceneId, {
        sceneId: clip.sceneId,
        sceneTitle,
        startSec: clip.startSec,
        endSec: clip.endSec,
        clips: [clip],
        totalSizeBytes: clipSizeBytes,
        highlight: highlightMap.get(clip.sceneId),
      });
      return;
    }

    existing.clips.push(clip);
    existing.startSec = Math.min(existing.startSec || clip.startSec, clip.startSec);
    existing.endSec = Math.max(existing.endSec || clip.endSec, clip.endSec);
    existing.totalSizeBytes = (existing.totalSizeBytes ?? 0) + clipSizeBytes;
  });

  // Separate compilations (scene_id=0) from regular scenes
  const regularScenes = Array.from(groups.values()).filter((g) => g.sceneId !== 0);
  const compilationScene = groups.get(0);

  // Sort regular scenes by time, then add compilations at the end
  regularScenes.sort((a, b) => a.startSec - b.startSec || a.sceneId - b.sceneId);

  return compilationScene ? [...regularScenes, compilationScene] : regularScenes;
}

interface HistorySceneExplorerProps {
  scenes: SceneGroup[];
  videoId: string;
  onDeleteClip?: (clip: HistoryClip) => Promise<void>;
  onDeleteScene?: (sceneId: number) => Promise<void>;
  onClipTitleUpdated?: (clipId: string, newTitle: string) => void;
}

export function HistorySceneExplorer({
  scenes,
  videoId,
  onDeleteClip,
  onDeleteScene,
  onClipTitleUpdated,
}: HistorySceneExplorerProps) {
  const { getIdToken } = useAuth();

  const resolvePlaybackUrl = useCallback(
    async (clip: HistoryClip): Promise<string> => {
      const token = await getIdToken();
      if (!token) throw new Error("Authentication required");
      const response = await getPlaybackUrl(clip.id, token);
      return response.url;
    },
    [getIdToken]
  );

  const handleDownload = useCallback(
    async (clip: HistoryClip) => {
      try {
        const token = await getIdToken();
        if (!token) {
          toast.error("Please sign in to download clips.");
          return;
        }
        await downloadClip(clip.id, token, clip.clipName);
        toast.success("Download started");
      } catch (error) {
        console.error("Download failed", error);
        toast.error("Failed to download clip");
      }
    },
    [getIdToken]
  );

  const handleCopyShareLink = useCallback(
    async (clip: HistoryClip) => {
      try {
        const token = await getIdToken();
        if (!token) {
          toast.error("Please sign in to share clips.");
          return;
        }
        await copyShareUrl(clip.id, token);
      } catch (error) {
        console.error("Failed to copy share link", error);
        toast.error("Failed to create share link");
      }
    },
    [getIdToken]
  );

  const handleUpdateTitle = useCallback(
    async (clip: HistoryClip, newTitle: string) => {
      try {
        const token = await getIdToken();
        if (!token) {
          toast.error("Please sign in to edit clip title.");
          return;
        }
        await updateClipTitle(videoId, clip.id, newTitle, token);
        toast.success("Title updated");
        onClipTitleUpdated?.(clip.id, newTitle);
      } catch (error) {
        console.error("Failed to update title", error);
        toast.error("Failed to update title");
      }
    },
    [getIdToken, videoId, onClipTitleUpdated]
  );

  // Sort scenes by sceneId to match the "Select Scenes" section ordering
  const sortedScenes = useMemo(
    () => [...scenes].sort((a, b) => a.sceneId - b.sceneId),
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
              resolvePlaybackUrl={resolvePlaybackUrl}
              onDownload={handleDownload}
              onCopyShareLink={handleCopyShareLink}
              onDeleteClip={onDeleteClip}
              onDeleteScene={onDeleteScene}
              onUpdateTitle={handleUpdateTitle}
              displayNumber={index + 1}
            />
          ))}
        </Accordion>
      )}
    </div>
  );
}

interface HistorySceneItemProps {
  scene: SceneGroup;
  resolvePlaybackUrl: (clip: HistoryClip) => Promise<string>;
  onDownload: (clip: HistoryClip) => Promise<void>;
  onCopyShareLink: (clip: HistoryClip) => Promise<void>;
  onDeleteClip?: (clip: HistoryClip) => Promise<void>;
  onDeleteScene?: (sceneId: number) => Promise<void>;
  onUpdateTitle?: (clip: HistoryClip, newTitle: string) => Promise<void>;
  /** 1-indexed display number for this scene */
  displayNumber: number;
}

function HistorySceneItem({
  scene,
  resolvePlaybackUrl,
  onDownload,
  onCopyShareLink,
  onDeleteClip,
  onDeleteScene,
  onUpdateTitle,
  displayNumber,
}: HistorySceneItemProps) {
  const sceneNumber = displayNumber;

  const canonicalizeStyle = useCallback((style?: string) => {
    const trimmed = style?.trim() ?? "";
    const normalized = normalizeStyleForSelection(trimmed);
    return (normalized ?? trimmed).toLowerCase();
  }, []);

  // For compilation clips (sceneId === 0), use clip ID as key to show all clips separately
  // For regular scenes, use style as key (one clip per style)
  const clipsByKey = useMemo(() => {
    const map = new Map<string, HistoryClip>();
    scene.clips.forEach((clip) => {
      if (scene.sceneId === 0) {
        // Compilation: show each clip separately (use clip ID as key)
        map.set(clip.id, clip);
      } else {
        // Regular scene: group by style (one clip per style)
        const key = canonicalizeStyle(clip.style) || "unknown";
        if (!map.has(key)) {
          map.set(key, clip);
        }
      }
    });
    return map;
  }, [scene.clips, scene.sceneId, canonicalizeStyle]);

  const styles = useMemo(() => {
    return Array.from(clipsByKey.keys()).sort((a, b) => {
      const clipA = clipsByKey.get(a);
      const clipB = clipsByKey.get(b);
      const tierA = getStyleTier(clipA?.style)?.color ?? "legacy";
      const tierB = getStyleTier(clipB?.style)?.color ?? "legacy";
      const weightA = getTierWeight(tierA);
      const weightB = getTierWeight(tierB);
      if (weightA === weightB) {
        // For compilations, sort by clip name/title
        if (scene.sceneId === 0) {
          const nameA = clipA?.clipName ?? clipA?.title ?? a;
          const nameB = clipB?.clipName ?? clipB?.title ?? b;
          return nameA.localeCompare(nameB);
        }
        return a.localeCompare(b);
      }
      return weightA - weightB;
    });
  }, [clipsByKey, scene.sceneId]);

  const [activeStyle, setActiveStyle] = useState<string>(styles[0] ?? "");
  const [clipToDelete, setClipToDelete] = useState<HistoryClip | null>(null);
  const [sceneDeleteOpen, setSceneDeleteOpen] = useState(false);
  const [deletingClipId, setDeletingClipId] = useState<string | null>(null);
  const [deletingScene, setDeletingScene] = useState(false);
  const [editingClip, setEditingClip] = useState<HistoryClip | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [savingTitle, setSavingTitle] = useState(false);

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

  const handleConfirmDeleteClip = useCallback(async () => {
    if (!clipToDelete || !onDeleteClip) {
      setClipToDelete(null);
      return;
    }
    setDeletingClipId(clipToDelete.id);
    try {
      await onDeleteClip(clipToDelete);
    } finally {
      setDeletingClipId(null);
      setClipToDelete(null);
    }
  }, [clipToDelete, onDeleteClip]);

  const handleSaveTitle = useCallback(async () => {
    if (!editingClip || !onUpdateTitle || !editTitle.trim()) {
      setEditingClip(null);
      return;
    }
    setSavingTitle(true);
    try {
      await onUpdateTitle(editingClip, editTitle.trim());
      setEditingClip(null);
    } finally {
      setSavingTitle(false);
    }
  }, [editingClip, editTitle, onUpdateTitle]);

  const handleConfirmDeleteScene = useCallback(async () => {
    if (!onDeleteScene) {
      setSceneDeleteOpen(false);
      return;
    }
    setDeletingScene(true);
    try {
      await onDeleteScene(scene.sceneId);
    } finally {
      setDeletingScene(false);
      setSceneDeleteOpen(false);
    }
  }, [onDeleteScene, scene.sceneId]);

  // Thumbnails are now fetched via delivery endpoints, not stored in clip data

  return (
    <Dialog open={sceneDeleteOpen} onOpenChange={setSceneDeleteOpen}>
      <AccordionItem
        value={`scene-${scene.sceneId}`}
        className="rounded-lg border bg-muted/30 px-3"
      >
        <AccordionPrimitive.Header className="flex items-center gap-3 sm:gap-4">
          <AccordionPrimitive.Trigger className="group flex w-full items-center gap-3 py-3 text-left">
            <div className="flex w-full items-center gap-3 sm:gap-4">
              <div className="flex-1 space-y-2 text-left">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="outline">
                    {scene.sceneId === 0 ? "Compilation" : `Scene ${sceneNumber}`}
                  </Badge>
                  <span className="text-sm text-muted-foreground">
                    {formatRange(scene)}
                  </span>
                  <Badge variant="secondary">{styles.length} styles</Badge>
                  {scene.totalSizeBytes && scene.totalSizeBytes > 0 && (
                    <Badge variant="outline" className="text-muted-foreground">
                      {formatBytes(scene.totalSizeBytes)}
                    </Badge>
                  )}
                </div>
                <div className="space-y-1">
                  <p className="font-semibold text-base leading-tight">
                    {scene.sceneTitle}
                  </p>
                  {scene.highlight?.reason && (
                    <p className="text-xs text-muted-foreground leading-snug">
                      {scene.highlight.reason}
                    </p>
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-2">
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

              <div className="flex items-center gap-3 sm:gap-4">
                {/* Thumbnails are fetched via delivery endpoints on demand */}
              </div>
            </div>
            <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200 group-data-[state=open]:rotate-180" />
          </AccordionPrimitive.Trigger>
          {onDeleteScene ? (
            <DialogTrigger asChild>
              <Button
                variant="destructive"
                size="sm"
                className="whitespace-nowrap text-white shadow-none"
                onClick={(e) => {
                  e.stopPropagation();
                  setSceneDeleteOpen(true);
                }}
                disabled={deletingScene}
              >
                Delete
              </Button>
            </DialogTrigger>
          ) : null}
        </AccordionPrimitive.Header>
        <AccordionContent>
          <div className="rounded-lg border bg-background/60 p-4 shadow-sm">
            <Tabs value={activeStyle} onValueChange={setActiveStyle}>
              <TabsList className="w-full flex flex-wrap gap-2">
                {styles.map((styleKey) => {
                  const tabClip = clipsByKey.get(styleKey);
                  const meta = getStyleTier(tabClip?.style);
                  // For compilations, show clip name; for regular scenes, show style label
                  const displayLabel =
                    scene.sceneId === 0
                      ? (tabClip?.sceneTitle ??
                        tabClip?.clipName ??
                        getStyleLabel(tabClip?.style) ??
                        styleKey)
                      : (getStyleLabel(tabClip?.style) ?? meta?.label ?? styleKey);
                  return (
                    <TabsTrigger
                      key={styleKey}
                      value={styleKey}
                      className={cn(
                        "flex items-center rounded-full px-0 py-0 bg-transparent shadow-none",
                        "data-[state=active]:bg-transparent data-[state=active]:text-primary"
                      )}
                    >
                      <Badge
                        className={cn(
                          "border",
                          getTierBadgeClasses(meta?.color ?? "legacy")
                        )}
                        variant="outline"
                      >
                        {displayLabel}
                      </Badge>
                    </TabsTrigger>
                  );
                })}
              </TabsList>

              {styles.map((styleKey) => {
                const clip = clipsByKey.get(styleKey);
                if (!clip) return null;
                const meta = getStyleTier(clip.style);

                return (
                  <TabsContent
                    key={styleKey}
                    value={styleKey}
                    className="mt-4 space-y-4"
                  >
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
                              {scene.sceneId === 0
                                ? (clip.sceneTitle ??
                                  clip.clipName ??
                                  getStyleLabel(clip.style) ??
                                  clip.style)
                                : (getStyleLabel(clip.style) ?? clip.style)}
                            </span>
                            <span>• Scene {sceneNumber}</span>
                            <span>• {formatRange(scene)}</span>
                            {clip.size ? <span>• {clip.size}</span> : null}
                          </div>

                          {/* Highlight title & description for social media */}
                          {scene.highlight && (
                            <HighlightInfoPanel highlight={scene.highlight} />
                          )}

                          <div className="flex flex-wrap gap-2">
                            <ActionButton
                              icon={<Download className="h-4 w-4" />}
                              label="Download"
                              onClick={() => onDownload(clip)}
                            />
                            <ActionButton
                              icon={<Link2 className="h-4 w-4" />}
                              label="Copy link"
                              onClick={() => onCopyShareLink(clip)}
                            />
                            <ActionButton
                              icon={<ExternalLink className="h-4 w-4" />}
                              label="Open"
                              onClick={async () => {
                                try {
                                  const url = await resolvePlaybackUrl(clip);
                                  window.open(url, "_blank");
                                } catch {
                                  toast.error("Failed to open clip");
                                }
                              }}
                            />
                            <ActionButton
                              icon={<Share2 className="h-4 w-4" />}
                              label="Share to Socials"
                              onClick={() => onCopyShareLink(clip)}
                            />
                            {onUpdateTitle && (
                              <Dialog
                                open={editingClip?.id === clip.id}
                                onOpenChange={(open) => {
                                  if (!open) {
                                    setEditingClip(null);
                                  } else {
                                    setEditingClip(clip);
                                    setEditTitle(clip.sceneTitle ?? clip.title ?? "");
                                  }
                                }}
                              >
                                <DialogTrigger asChild>
                                  <Button
                                    variant="outline"
                                    size="sm"
                                    className="gap-2"
                                    onClick={() => {
                                      setEditingClip(clip);
                                      setEditTitle(clip.sceneTitle ?? clip.title ?? "");
                                    }}
                                  >
                                    <Pencil className="h-4 w-4" />
                                    Edit title
                                  </Button>
                                </DialogTrigger>
                                <DialogContent>
                                  <DialogHeader>
                                    <DialogTitle>Edit clip title</DialogTitle>
                                    <DialogDescription>
                                      Change the title for this clip.
                                    </DialogDescription>
                                  </DialogHeader>
                                  <Input
                                    value={editTitle}
                                    onChange={(e) => setEditTitle(e.target.value)}
                                    placeholder="Enter new title"
                                    disabled={savingTitle}
                                  />
                                  <DialogFooter className="gap-2 sm:justify-end">
                                    <DialogClose asChild>
                                      <Button variant="outline" disabled={savingTitle}>
                                        Cancel
                                      </Button>
                                    </DialogClose>
                                    <Button
                                      onClick={handleSaveTitle}
                                      disabled={savingTitle || !editTitle.trim()}
                                    >
                                      {savingTitle ? "Saving..." : "Save"}
                                    </Button>
                                  </DialogFooter>
                                </DialogContent>
                              </Dialog>
                            )}
                            {onDeleteClip ? (
                              <Dialog
                                open={clipToDelete?.id === clip.id}
                                onOpenChange={(open) => {
                                  if (!open) setClipToDelete(null);
                                  else setClipToDelete(clip);
                                }}
                              >
                                <DialogTrigger asChild>
                                  <Button
                                    variant="destructive"
                                    size="sm"
                                    className="gap-2"
                                    onClick={() => setClipToDelete(clip)}
                                    disabled={deletingClipId === clip.id}
                                  >
                                    <Trash className="h-4 w-4" />
                                    Delete clip
                                  </Button>
                                </DialogTrigger>
                                <DialogContent>
                                  <DialogHeader>
                                    <DialogTitle>Delete this clip?</DialogTitle>
                                    <DialogDescription>
                                      This will delete only the{" "}
                                      {getStyleLabel(clip.style) ?? clip.style} clip for
                                      this scene. This cannot be undone.
                                    </DialogDescription>
                                  </DialogHeader>
                                  <DialogFooter className="gap-2 sm:justify-end">
                                    <DialogClose asChild>
                                      <Button
                                        variant="outline"
                                        disabled={deletingClipId === clip.id}
                                      >
                                        Cancel
                                      </Button>
                                    </DialogClose>
                                    <Button
                                      variant="destructive"
                                      onClick={handleConfirmDeleteClip}
                                      disabled={deletingClipId === clip.id}
                                    >
                                      {deletingClipId === clip.id
                                        ? "Deleting..."
                                        : "Delete clip"}
                                    </Button>
                                  </DialogFooter>
                                </DialogContent>
                              </Dialog>
                            ) : null}
                          </div>
                        </CardContent>
                      </Card>

                      <Card className="border-dashed">
                        <CardHeader className="pb-2">
                          <CardTitle className="text-sm">
                            Styles in this scene
                          </CardTitle>
                          <p className="text-xs text-muted-foreground">
                            Switch styles quickly or jump to a thumbnail.
                          </p>
                        </CardHeader>
                        <CardContent className="space-y-3">
                          <div className="grid gap-3 sm:grid-cols-2">
                            {styles.map((s) => {
                              const thumbClip = clipsByKey.get(s);
                              const thumbMeta = getStyleTier(thumbClip?.style);
                              if (!thumbClip) return null;
                              // For compilations, show clip title; for regular scenes, show style
                              const thumbLabel =
                                scene.sceneId === 0
                                  ? (thumbClip.sceneTitle ??
                                    thumbClip.clipName ??
                                    getStyleLabel(thumbClip.style) ??
                                    s)
                                  : (getStyleLabel(thumbClip.style) ?? s);
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
                                  <ClipThumbnail
                                    clipId={thumbClip.id}
                                    alt={`${thumbLabel} thumbnail`}
                                    className="h-28 w-full"
                                  />
                                  <div className="absolute left-2 top-2">
                                    <Badge
                                      className={cn(
                                        "border",
                                        getTierBadgeClasses(
                                          thumbMeta?.color ?? "legacy"
                                        )
                                      )}
                                      variant="outline"
                                    >
                                      {thumbMeta?.label ?? "Legacy"}
                                    </Badge>
                                  </div>
                                  <div className="space-y-1 p-2">
                                    <p className="text-xs font-semibold leading-tight">
                                      {thumbLabel}
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
                    {onDeleteScene ? (
                      <div className="flex items-center justify-between rounded-md border bg-muted/40 px-4 py-3">
                        <div className="text-sm text-muted-foreground">
                          Delete this scene and all {scene.clips.length} clip
                          {scene.clips.length === 1 ? "" : "s"}.
                        </div>
                        <DialogTrigger asChild>
                          <Button
                            variant="destructive"
                            size="sm"
                            className="gap-2"
                            onClick={() => setSceneDeleteOpen(true)}
                            disabled={deletingScene}
                          >
                            <Trash2 className="h-4 w-4" />
                            Delete scene
                          </Button>
                        </DialogTrigger>
                      </div>
                    ) : null}
                  </TabsContent>
                );
              })}
            </Tabs>
          </div>
        </AccordionContent>
      </AccordionItem>
      {onDeleteScene ? (
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete scene?</DialogTitle>
            <DialogDescription>
              This will delete all clips for this scene. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="gap-2 sm:justify-end">
            <DialogClose asChild>
              <Button variant="outline" disabled={deletingScene}>
                Cancel
              </Button>
            </DialogClose>
            <Button
              variant="destructive"
              onClick={handleConfirmDeleteScene}
              disabled={deletingScene}
            >
              {deletingScene ? "Deleting..." : "Delete scene"}
            </Button>
          </DialogFooter>
        </DialogContent>
      ) : null}
    </Dialog>
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
    <div className="relative overflow-hidden rounded-lg border bg-black">
      <div className="aspect-[9/16] max-h-[500px] mx-auto">
        <video
          src={videoUrl}
          controls
          className="h-full w-full object-contain"
          poster={undefined}
          preload="metadata"
        >
          <track kind="captions" />
        </video>
      </div>
    </div>
  );
}

interface HighlightInfoPanelProps {
  highlight: Highlight;
}

/**
 * Displays highlight title and description with a copy button for social media.
 */
function HighlightInfoPanel({ highlight }: HighlightInfoPanelProps) {
  const handleCopy = useCallback(async () => {
    const text = buildHighlightCopyText(highlight);
    try {
      await navigator.clipboard.writeText(text);
      toast.success("Copied title & description for social media");
    } catch {
      toast.error("Failed to copy");
    }
  }, [highlight]);

  return (
    <div className="rounded-md border bg-muted/40 p-3 space-y-2">
      <div className="flex items-start justify-between gap-2">
        <div className="space-y-1 flex-1 min-w-0">
          <p className="text-sm font-semibold leading-tight">{highlight.title}</p>
          {highlight.description && (
            <p className="text-xs text-muted-foreground">{highlight.description}</p>
          )}
        </div>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5 shrink-0"
          onClick={handleCopy}
        >
          <Copy className="h-3.5 w-3.5" />
          Copy for social
        </Button>
      </div>
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

interface ClipThumbnailProps {
  clipId: string;
  alt: string;
  className?: string;
}

/**
 * Displays a clip thumbnail, fetching the URL on mount.
 * Shows a placeholder while loading or on error.
 */
function ClipThumbnail({ clipId, alt, className }: ClipThumbnailProps) {
  const { getIdToken } = useAuth();
  const [thumbnailUrl, setThumbnailUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function loadThumbnail() {
      try {
        const token = await getIdToken();
        if (!token || cancelled) return;

        const response = await getThumbnailUrl(clipId, token);
        if (!cancelled) {
          setThumbnailUrl(response.url);
          setError(false);
        }
      } catch (err) {
        console.error("Failed to load thumbnail:", err);
        if (!cancelled) {
          setError(true);
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadThumbnail();
    return () => {
      cancelled = true;
    };
  }, [clipId, getIdToken]);

  if (loading) {
    return (
      <div
        className={cn(
          "flex items-center justify-center bg-muted animate-pulse",
          className
        )}
      >
        <ImageIcon className="h-6 w-6 text-muted-foreground/50" />
      </div>
    );
  }

  if (error || !thumbnailUrl) {
    return (
      <div className={cn("flex items-center justify-center bg-muted", className)}>
        <ImageIcon className="h-6 w-6 text-muted-foreground/50" />
      </div>
    );
  }

  return (
    // Using img for dynamic external thumbnails with presigned URLs
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={thumbnailUrl}
      alt={alt}
      className={cn("object-cover", className)}
      loading="lazy"
    />
  );
}
