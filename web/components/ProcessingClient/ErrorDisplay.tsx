/**
 * Error Display Component
 *
 * Displays error messages and details.
 */

import { AlertCircle } from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface ErrorDisplayProps {
  error: string;
  errorDetails: string | null;
}

export function ErrorDisplay({ error, errorDetails }: ErrorDisplayProps) {
  return (
    <section>
      <Card className="glass border-l-4 border-l-destructive bg-destructive/10">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-destructive">
            <AlertCircle className="h-5 w-5" />
            Processing Failed
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-foreground">{error}</p>
          {errorDetails && (
            <pre className="bg-muted/50 p-4 rounded-lg text-xs text-destructive overflow-x-auto whitespace-pre-wrap border">
              {errorDetails}
            </pre>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
