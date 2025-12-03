/**
 * Processing Status Component
 *
 * Displays processing progress and logs.
 */

import { Loader2, Info } from "lucide-react";
import Link from "next/link";

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

          <div className="bg-primary/10 border border-primary/20 rounded-lg p-4 flex items-start gap-3">
            <Info className="h-5 w-5 text-primary mt-0.5 flex-shrink-0" />
            <div className="space-y-1 text-sm">
              <p className="font-semibold text-foreground">
                You can safely leave this page
              </p>
              <p className="text-muted-foreground">
                Your video is being processed in the background. You can navigate away
                and check your{" "}
                <Link
                  href="/history"
                  className="text-primary hover:underline font-medium"
                >
                  history page
                </Link>{" "}
                to see progress. Processing will continue even if you close this tab.
              </p>
            </div>
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
