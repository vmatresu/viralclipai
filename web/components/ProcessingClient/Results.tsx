/**
 * Results Display Component
 *
 * Displays processing results and clips.
 */

import { Sparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

import { ClipGrid, type Clip } from "../ClipGrid";

interface ResultsProps {
  videoId: string;
  clips: Clip[];
  customPromptUsed: string | null;
  log: (msg: string, type?: "info" | "error" | "success") => void;
  onReset: () => void;
}

export function Results({
  videoId,
  clips,
  customPromptUsed,
  log,
  onReset,
}: ResultsProps) {
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
      {customPromptUsed && (
        <Card className="glass">
          <CardHeader>
            <CardTitle className="text-sm">Custom prompt used</CardTitle>
          </CardHeader>
          <CardContent>
            <CardDescription className="text-xs whitespace-pre-wrap">
              {customPromptUsed}
            </CardDescription>
          </CardContent>
        </Card>
      )}
      <ClipGrid videoId={videoId} clips={clips} log={log} />
    </section>
  );
}
