"use client";

import { Clock } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
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
  return (
    <Card
      className={`cursor-pointer transition-all ${
        selected
          ? "border-primary bg-primary/5"
          : "hover:border-primary/50"
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
              <h4 className="font-semibold text-sm leading-tight">
                {highlight.title}
              </h4>
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
            {highlight.reason && (
              <p
                className="text-xs text-muted-foreground line-clamp-2"
                title={highlight.reason}
              >
                {highlight.reason}
              </p>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

