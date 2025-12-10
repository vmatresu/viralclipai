"use client";

import { Clock, Copy } from "lucide-react";
import React, { useCallback } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";

export interface Highlight {
  id: number;
  title: string;
  start: string;
  end: string;
  duration: number;
  hook_category?: string;
  reason?: string;
  description?: string;
}

/**
 * Build social media copy text from highlight title and description.
 */
export function buildHighlightCopyText(highlight: Highlight): string {
  const parts: string[] = [highlight.title];
  if (highlight.description) {
    parts.push(highlight.description);
  }
  return parts.join("\n\n");
}

interface SceneCardProps {
  highlight: Highlight;
  selected: boolean;
  disabled?: boolean;
  onToggle: (sceneId: number) => void;
  formatTime: (timeStr: string) => string;
}

export function SceneCard({
  highlight,
  selected,
  disabled = false,
  onToggle,
  formatTime,
}: SceneCardProps) {
  const handleCopyForSocial = useCallback(
    async (e: React.MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation();
      const text = buildHighlightCopyText(highlight);
      try {
        await navigator.clipboard.writeText(text);
        toast.success("Copied title & description for social media");
      } catch {
        toast.error("Failed to copy");
      }
    },
    [highlight]
  );

  return (
    <Card
      className={`cursor-pointer transition-all ${
        selected ? "border-primary bg-primary/5" : "hover:border-primary/50"
      } ${disabled ? "opacity-50 cursor-not-allowed" : ""}`}
      onClick={() => {
        if (!disabled) {
          onToggle(highlight.id);
        }
      }}
    >
      <CardContent className="p-4">
        <div className="flex items-start gap-3">
          <Checkbox
            checked={selected}
            onCheckedChange={() => {
              if (!disabled) {
                onToggle(highlight.id);
              }
            }}
            disabled={disabled}
            className="mt-1"
          />
          <div className="flex-1 min-w-0 space-y-2">
            <div className="flex items-start justify-between gap-2">
              <h4 className="font-semibold text-sm leading-tight">{highlight.title}</h4>
              <Badge variant="outline" className="text-xs shrink-0">
                {formatTime(highlight.start)} - {formatTime(highlight.end)}
              </Badge>
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Clock className="h-3 w-3" />
              <span>{highlight.duration}s</span>
              {highlight.hook_category && (
                <>
                  <span>•</span>
                  <span>{highlight.hook_category}</span>
                </>
              )}
            </div>
            {/* Show full reason without truncation */}
            {highlight.reason && (
              <p className="text-xs text-muted-foreground">{highlight.reason}</p>
            )}
            {/* Show description if present */}
            {highlight.description && (
              <p className="text-xs text-muted-foreground/80 italic">
                {highlight.description}
              </p>
            )}
            {/* Copy button for social media */}
            <Button
              variant="outline"
              size="sm"
              className="gap-1.5 h-7 text-xs mt-1"
              onClick={handleCopyForSocial}
            >
              <Copy className="h-3 w-3" />
              Copy for social
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
