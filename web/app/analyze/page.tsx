"use client";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useStartAnalysis } from "@/lib/analysis";
import { useAuth } from "@/lib/auth";
import { ArrowRight, Film, Loader2, Sparkles, Zap } from "lucide-react";
import { useRouter } from "next/navigation";
import { useState } from "react";

export default function AnalyzePage() {
  const router = useRouter();
  const { user, loading: authLoading } = useAuth();
  const { trigger: startAnalysis, isMutating } = useStartAnalysis();

  const [url, setUrl] = useState("");
  const [prompt, setPrompt] = useState("");
  const [showPrompt, setShowPrompt] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!user) {
      setError("Please sign in to analyze videos");
      return;
    }

    if (!url.trim()) {
      setError("Please enter a YouTube URL");
      return;
    }

    if (!url.includes("youtube.com") && !url.includes("youtu.be")) {
      setError("Please enter a valid YouTube URL");
      return;
    }

    try {
      const result = await startAnalysis({
        url: url.trim(),
        prompt: showPrompt && prompt.trim() ? prompt.trim() : undefined,
      });

      // Redirect to status page
      router.push(`/analyze/${result.draft_id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to start analysis");
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      {/* Decorative elements */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -top-40 -right-40 w-96 h-96 bg-violet-500/10 rounded-full blur-3xl" />
        <div className="absolute top-1/3 -left-40 w-96 h-96 bg-blue-500/10 rounded-full blur-3xl" />
        <div className="absolute bottom-20 right-1/4 w-64 h-64 bg-emerald-500/10 rounded-full blur-3xl" />
      </div>

      <div className="relative container mx-auto px-4 py-12 md:py-20">
        {/* Header */}
        <div className="text-center mb-12">
          <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-violet-500/10 border border-violet-500/20 text-violet-300 text-sm font-medium mb-6">
            <Sparkles className="w-4 h-4" />
            AI-Powered Video Analysis
          </div>
          <h1 className="text-4xl md:text-5xl lg:text-6xl font-bold text-white mb-4 tracking-tight">
            Turn Videos Into
            <span className="bg-gradient-to-r from-violet-400 via-blue-400 to-emerald-400 text-transparent bg-clip-text"> Viral Clips</span>
          </h1>
          <p className="text-slate-400 text-lg md:text-xl max-w-2xl mx-auto">
            Paste a YouTube URL and let AI find the most engaging moments.
            Then choose exactly which clips to create.
          </p>
        </div>

        {/* Main form card */}
        <Card className="max-w-2xl mx-auto bg-slate-900/50 border-slate-800/50 backdrop-blur-xl shadow-2xl">
          <CardHeader className="space-y-1">
            <CardTitle className="text-2xl text-white">Analyze Video</CardTitle>
            <CardDescription className="text-slate-400">
              Enter a YouTube URL to find viral-worthy moments
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit} className="space-y-6">
              {/* URL Input */}
              <div className="space-y-2">
                <label htmlFor="url" className="text-sm font-medium text-slate-300">
                  YouTube URL
                </label>
                <div className="relative">
                  <Film className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-slate-500" />
                  <Input
                    id="url"
                    type="url"
                    placeholder="https://youtube.com/watch?v=..."
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    className="pl-11 bg-slate-800/50 border-slate-700 text-white placeholder:text-slate-500 focus:border-violet-500 focus:ring-violet-500/20"
                    disabled={isMutating}
                  />
                </div>
              </div>

              {/* AI Instructions Toggle */}
              <div className="space-y-2">
                <button
                  type="button"
                  onClick={() => setShowPrompt(!showPrompt)}
                  className="text-sm font-medium text-violet-400 hover:text-violet-300 transition-colors flex items-center gap-1"
                >
                  <Zap className="w-4 h-4" />
                  {showPrompt ? "Hide" : "Add"} AI instructions (optional)
                </button>
                {showPrompt && (
                  <Textarea
                    placeholder="E.g., Focus on controversial statements, funny moments, or educational content..."
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    className="bg-slate-800/50 border-slate-700 text-white placeholder:text-slate-500 focus:border-violet-500 focus:ring-violet-500/20 min-h-[80px]"
                    disabled={isMutating}
                  />
                )}
              </div>

              {/* Error */}
              {error && (
                <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
                  {error}
                </div>
              )}

              {/* Submit button */}
              <Button
                type="submit"
                disabled={isMutating || authLoading}
                className="w-full h-12 bg-gradient-to-r from-violet-600 to-blue-600 hover:from-violet-500 hover:to-blue-500 text-white font-semibold text-lg shadow-lg shadow-violet-500/25 transition-all duration-300"
              >
                {isMutating ? (
                  <>
                    <Loader2 className="w-5 h-5 mr-2 animate-spin" />
                    Starting Analysis...
                  </>
                ) : (
                  <>
                    Analyze Video
                    <ArrowRight className="w-5 h-5 ml-2" />
                  </>
                )}
              </Button>
            </form>
          </CardContent>
        </Card>

        {/* Features */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 max-w-4xl mx-auto mt-16">
          <FeatureCard
            icon={<Sparkles className="w-6 h-6 text-violet-400" />}
            title="AI Scene Detection"
            description="Automatically find the most engaging moments using advanced AI analysis"
          />
          <FeatureCard
            icon={<Film className="w-6 h-6 text-blue-400" />}
            title="Preview Before Render"
            description="Review all detected scenes and choose exactly which ones to export"
          />
          <FeatureCard
            icon={<Zap className="w-6 h-6 text-emerald-400" />}
            title="Multiple Styles"
            description="Apply Full or Split screen formats with smart face tracking"
          />
        </div>
      </div>
    </div>
  );
}

function FeatureCard({
  icon,
  title,
  description,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
}) {
  return (
    <div className="p-6 rounded-2xl bg-slate-900/30 border border-slate-800/50 backdrop-blur">
      <div className="w-12 h-12 rounded-xl bg-slate-800/50 flex items-center justify-center mb-4">
        {icon}
      </div>
      <h3 className="text-lg font-semibold text-white mb-2">{title}</h3>
      <p className="text-slate-400 text-sm">{description}</p>
    </div>
  );
}
