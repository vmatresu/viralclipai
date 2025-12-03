"use client";

import { Loader2 } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import Link from "next/link";

interface ProcessingStatusProps {
  videoId: string;
}

export function ProcessingStatus({ videoId }: ProcessingStatusProps) {
  return (
    <Card className="glass border-l-4 border-l-primary">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Loader2 className="h-5 w-5 animate-spin text-primary" />
          Processing in Background
        </CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">
          Your video is being processed. You can{" "}
          <Link
            href={`/?id=${encodeURIComponent(videoId)}`}
            className="text-primary hover:underline"
          >
            view progress here
          </Link>
          {" "}or continue browsing. Processing will continue even if you leave
          this page.
        </p>
      </CardContent>
    </Card>
  );
}

