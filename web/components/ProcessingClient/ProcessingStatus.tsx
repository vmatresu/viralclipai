/**
 * Processing Status Component
 *
 * Displays processing progress and logs.
 */

import { Loader2 } from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface ProcessingStatusProps {
  progress: number;
  logs: string[];
}

export function ProcessingStatus({ progress, logs }: ProcessingStatusProps) {
  return (
    <section className="space-y-6">
      <Card className="glass border-l-4 border-l-primary">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
            Processing Video...
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="w-full bg-muted rounded-full h-4 overflow-hidden">
            <div
              className="bg-gradient-to-r from-brand-500 to-brand-700 h-4 rounded-full transition-all duration-500 ease-out"
              style={{ width: `${progress}%` }}
            />
          </div>

          <div className="bg-muted/50 rounded-xl p-4 font-mono text-sm h-64 overflow-y-auto border space-y-1">
            {logs.length === 0 ? (
              <div className="text-muted-foreground italic">Waiting for task...</div>
            ) : (
              logs.map((l, idx) => (
                <div key={idx} className="text-foreground">
                  {/* Timestamps are already formatted in the log string */}
                  {l}
                </div>
              ))
            )}
          </div>
        </CardContent>
      </Card>
    </section>
  );
}
