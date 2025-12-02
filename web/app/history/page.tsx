"use client";

import { useEffect, useState } from "react";
import { Clock, Film, AlertCircle } from "lucide-react";

import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { usePageView } from "@/lib/usePageView";
import { Button } from "@/components/ui/button";
import { SignInDialog } from "@/components/SignInDialog";

interface UserVideo {
  id?: string;
  video_id?: string;
  video_title?: string;
  video_url?: string;
  created_at?: string;
  custom_prompt?: string;
}

export default function HistoryPage() {
  usePageView("history");
  const { getIdToken, user, loading: authLoading } = useAuth();
  const [videos, setVideos] = useState<UserVideo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    
    async function load() {
      if (authLoading) return;
      
      if (!user) {
        // Not logged in - stop loading but don't error yet (let UI handle it)
        setLoading(false);
        return;
      }

      try {
        const token = await getIdToken();
        if (!token) {
          throw new Error("Failed to get authentication token");
        }
        const data = (await apiFetch<{ videos: UserVideo[] }>("/api/user/videos", {
          token,
        })) as { videos: UserVideo[] };
        if (!cancelled) {
          setVideos(data.videos ?? []);
          setError(null);
        }
      } catch (err: unknown) {
        if (!cancelled) {
          const errorMessage =
            err instanceof Error ? err.message : "Failed to load history";
          setError(errorMessage);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [getIdToken, user, authLoading]);

  if (authLoading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
        <p className="text-muted-foreground">Checking authentication...</p>
      </div>
    );
  }

  if (!user) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-6 text-center">
        <div className="bg-muted/30 p-4 rounded-full">
          <Clock className="h-12 w-12 text-muted-foreground" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">Sign in to view history</h2>
          <p className="text-muted-foreground max-w-md">
            Your processing history is stored securely in your account. Sign in to access your past videos.
          </p>
        </div>
        <SignInDialog>
          <Button size="lg" className="gap-2">
            Sign In
          </Button>
        </SignInDialog>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
        <p className="text-muted-foreground">Loading your processing history...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-4 text-center">
        <AlertCircle className="h-12 w-12 text-destructive" />
        <div className="space-y-2">
          <h3 className="text-xl font-semibold">Failed to load history</h3>
          <p className="text-muted-foreground">{error}</p>
        </div>
        <Button variant="outline" onClick={() => window.location.reload()}>
          Try Again
        </Button>
      </div>
    );
  }

  if (videos.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-24 space-y-6 text-center">
        <div className="bg-muted/30 p-4 rounded-full">
          <Film className="h-12 w-12 text-muted-foreground" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">No history found</h2>
          <p className="text-muted-foreground max-w-md">
            You haven't processed any videos yet. Start by processing your first video on the home page.
          </p>
        </div>
        <Button asChild>
          <a href="/">Process Video</a>
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6 page-container">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold tracking-tight">History</h1>
        <p className="text-muted-foreground text-sm">{videos.length} videos processed</p>
      </div>
      
      <div className="grid gap-4">
        {videos.map((v) => {
          const id = v.video_id ?? v.id ?? "";
          // Format date if possible
          let dateStr = v.created_at ?? "";
          try {
            if (dateStr) {
              dateStr = new Date(dateStr).toLocaleDateString(undefined, {
                year: 'numeric',
                month: 'long',
                day: 'numeric',
                hour: '2-digit',
                minute: '2-digit'
              });
            }
          } catch (e) {
            // keep original string if parse fails
          }

          return (
            <a
              key={id}
              href={`/?id=${encodeURIComponent(id)}`}
              className="glass p-6 rounded-xl hover:shadow-md hover:border-primary/30 transition-all group block border border-border/50"
            >
              <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-4">
                <div className="space-y-2 min-w-0 flex-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <h3 className="text-lg font-bold text-foreground group-hover:text-primary transition-colors truncate">
                      {v.video_title || "Generated Clips"}
                    </h3>
                    <span className="inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80">
                      ID: {id.substring(0, 8)}...
                    </span>
                  </div>
                  
                  <div className="text-sm text-muted-foreground truncate font-mono bg-muted/30 px-2 py-1 rounded w-fit max-w-full">
                    {v.video_url}
                  </div>
                  
                  {v.custom_prompt && (
                    <div className="mt-2 text-xs text-muted-foreground bg-muted/20 p-2 rounded border border-border/30">
                      <span className="font-semibold mr-1">Custom Prompt:</span>
                      <span className="italic">{v.custom_prompt}</span>
                    </div>
                  )}
                </div>
                
                <div className="text-xs text-muted-foreground whitespace-nowrap flex items-center gap-1 sm:text-right">
                  <Clock className="h-3 w-3" />
                  {dateStr}
                </div>
              </div>
            </a>
          );
        })}
      </div>
    </div>
  );
}