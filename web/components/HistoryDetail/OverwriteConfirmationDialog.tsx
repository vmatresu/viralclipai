"use client";

import { AlertTriangle, CheckCircle2, ShieldAlert } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

export interface OverwriteTarget {
  sceneId: number;
  sceneTitle?: string;
  style: string;
}

interface OverwriteConfirmationDialogProps {
  open: boolean;
  conflicts: OverwriteTarget[];
  fresh: OverwriteTarget[];
  onCancel: () => void;
  onConfirm: () => void;
  promptEnabled: boolean;
  onTogglePrompt: (value: boolean) => void;
}

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

function getStyleLabel(style: string): string {
  return STYLE_LABELS[style] ?? style;
}

function TargetList({
  title,
  items,
  tone,
}: {
  title: string;
  items: OverwriteTarget[];
  tone: "warning" | "success";
}) {
  if (items.length === 0) return null;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-sm font-semibold">
        {tone === "warning" ? (
          <AlertTriangle className="h-4 w-4 text-destructive" />
        ) : (
          <CheckCircle2 className="h-4 w-4 text-emerald-500" />
        )}
        <span>{title}</span>
      </div>
      <div className="rounded-md border bg-muted/40 p-3 max-h-60 overflow-y-auto space-y-2">
        {items.map((item) => (
          <div
            key={`${item.sceneId}-${item.style}`}
            className="flex items-center justify-between gap-2 rounded-md bg-background p-2 shadow-sm"
          >
            <div className="space-y-1">
              <div className="text-sm font-medium">
                {item.sceneTitle ?? `Scene ${item.sceneId}`}
              </div>
              <div className="text-xs text-muted-foreground">ID: {item.sceneId}</div>
            </div>
            <Badge variant={tone === "warning" ? "destructive" : "secondary"}>
              {getStyleLabel(item.style)}
            </Badge>
          </div>
        ))}
      </div>
    </div>
  );
}

export function OverwriteConfirmationDialog({
  open,
  conflicts,
  fresh,
  onCancel,
  onConfirm,
  promptEnabled,
  onTogglePrompt,
}: OverwriteConfirmationDialogProps) {
  const overwriteCount = conflicts.length;
  const newCount = fresh.length;

  return (
    <Dialog open={open} onOpenChange={(next) => (!next ? onCancel() : undefined)}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ShieldAlert className="h-5 w-5 text-primary" />
            Overwrite existing clips?
          </DialogTitle>
          <DialogDescription className="space-y-2">
            <p>
              You selected styles that already have generated clips. Continuing will{" "}
              <span className="font-semibold text-destructive">overwrite</span> those
              files in storage. Metadata will also be refreshed.
            </p>
            <p className="text-muted-foreground">
              Review what will be overwritten and what will be created new before
              proceeding.
            </p>
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="grid gap-4 md:grid-cols-2">
            <div className="rounded-lg border bg-amber-50 p-3 text-sm text-amber-900">
              <div className="font-semibold">
                {overwriteCount} clip{overwriteCount === 1 ? "" : "s"} will be replaced
              </div>
              <div className="text-amber-800">
                New outputs will overwrite the existing files for the same scene/style.
              </div>
            </div>
            <div className="rounded-lg border bg-emerald-50 p-3 text-sm text-emerald-900">
              <div className="font-semibold">
                {newCount} clip{newCount === 1 ? "" : "s"} will be created
              </div>
              <div className="text-emerald-800">
                These scene/style combinations do not exist yet.
              </div>
            </div>
          </div>

          <TargetList title="Will overwrite" items={conflicts} tone="warning" />
          <TargetList title="New clips" items={fresh} tone="success" />
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <div className="flex flex-1 flex-col gap-2 text-sm text-muted-foreground">
            <div className="flex items-center justify-between">
              <span>
                Will overwrite {overwriteCount}, create {newCount}
              </span>
              <div className="flex items-center gap-2">
                <Checkbox
                  id="overwrite-prompt-toggle"
                  checked={promptEnabled}
                  onCheckedChange={(checked) => onTogglePrompt(Boolean(checked))}
                />
                <label
                  htmlFor="overwrite-prompt-toggle"
                  className="text-sm select-none cursor-pointer"
                >
                  Always ask before overwriting (this session)
                </label>
              </div>
            </div>
          </div>
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            Overwrite and continue
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
