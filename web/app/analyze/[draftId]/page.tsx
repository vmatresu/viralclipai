"use client";

import {
  AlertTriangle,
  ArrowRight,
  Brain,
  CheckCircle2,
  Clock,
  Download,
  Loader2,
  Sparkles,
  XCircle,
} from "lucide-react";
import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { isActiveStatus, useAnalysisStatus } from "@/lib/analysis";

const STATUS_CONFIG = {
  pending: {
    icon: Clock,
    label: "Queued",
    description: "Your video is queued for analysis",
    color: "text-slate-400",
    bgColor: "bg-slate-500/10",
    progress: 10,
  },
  downloading: {
    icon: Download,
    label: "Downloading",
    description: "Fetching video information",
    color: "text-blue-400",
    bgColor: "bg-blue-500/10",
    progress: 30,
  },
  analyzing: {
    icon: Brain,
    label: "Analyzing",
    description: "AI is identifying viral-worthy moments",
    color: "text-violet-400",
    bgColor: "bg-violet-500/10",
    progress: 70,
  },
  completed: {
    icon: CheckCircle2,
    label: "Complete",
    description: "Analysis finished! Ready to select scenes",
    color: "text-emerald-400",
    bgColor: "bg-emerald-500/10",
    progress: 100,
  },
  failed: {
    icon: XCircle,
    label: "Failed",
    description: "Something went wrong during analysis",
    color: "text-red-400",
    bgColor: "bg-red-500/10",
    progress: 0,
  },
  expired: {
    icon: AlertTriangle,
    label: "Expired",
    description: "This analysis has expired. Please start a new one.",
    color: "text-amber-400",
    bgColor: "bg-amber-500/10",
    progress: 0,
  },
};

export default function AnalysisStatusPage() {
  const params = useParams();
  const router = useRouter();
  const draftId = params.draftId as string;

  const { status, error, isLoading } = useAnalysisStatus(draftId);

  // Redirect to draft page when complete
  useEffect(() => {
    if (status?.status === "completed") {
      // Small delay to show the success state
      const timer = setTimeout(() => {
        router.push(`/drafts/${draftId}`);
      }, 1500);
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [status?.status, draftId, router]);

  const currentStatus = status?.status ?? "pending";
  const config =
    STATUS_CONFIG[currentStatus as keyof typeof STATUS_CONFIG] ?? STATUS_CONFIG.pending;
  const StatusIcon = config.icon;

  if (isLoading && !status) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
        <Loader2 className="w-8 h-8 text-violet-400 animate-spin" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
        <Card className="max-w-md bg-slate-900/50 border-slate-800/50">
          <CardContent className="pt-6 text-center">
            <XCircle className="w-12 h-12 text-red-400 mx-auto mb-4" />
            <h2 className="text-xl font-semibold text-white mb-2">Error</h2>
            <p className="text-slate-400 mb-4">{error.message}</p>
            <Button onClick={() => router.push("/analyze")}>Try Again</Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      {/* Decorative elements */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -top-40 -right-40 w-96 h-96 bg-violet-500/10 rounded-full blur-3xl animate-pulse" />
        <div className="absolute bottom-20 -left-40 w-96 h-96 bg-blue-500/10 rounded-full blur-3xl animate-pulse" />
      </div>

      <div className="relative container mx-auto px-4 py-20">
        <div className="max-w-xl mx-auto text-center">
          {/* Status Icon */}
          <div
            className={`w-24 h-24 rounded-3xl ${config.bgColor} flex items-center justify-center mx-auto mb-8 transition-all duration-500`}
          >
            {isActiveStatus(currentStatus) ? (
              <div className="relative">
                <StatusIcon className={`w-12 h-12 ${config.color}`} />
                <div className="absolute inset-0 animate-ping">
                  <StatusIcon className={`w-12 h-12 ${config.color} opacity-40`} />
                </div>
              </div>
            ) : (
              <StatusIcon className={`w-12 h-12 ${config.color}`} />
            )}
          </div>

          {/* Status Text */}
          <h1 className="text-3xl md:text-4xl font-bold text-white mb-4">
            {config.label}
          </h1>
          <p className="text-slate-400 text-lg mb-8">{config.description}</p>

          {/* Video Title */}
          {status?.video_title && (
            <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-slate-800/50 text-slate-300 text-sm mb-8">
              <Sparkles className="w-4 h-4 text-violet-400" />
              {status.video_title}
            </div>
          )}

          {/* Progress Bar */}
          {isActiveStatus(currentStatus) && (
            <Card className="bg-slate-900/50 border-slate-800/50 backdrop-blur-xl mb-8">
              <CardContent className="pt-6">
                <Progress value={config.progress} className="h-2 bg-slate-800" />
                <div className="flex justify-between text-sm text-slate-500 mt-2">
                  <span>Progress</span>
                  <span>{config.progress}%</span>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Error Message */}
          {status?.error_message && (
            <Card className="bg-red-500/10 border-red-500/20 mb-8">
              <CardContent className="pt-6">
                <p className="text-red-400">{status.error_message}</p>
              </CardContent>
            </Card>
          )}

          {/* Action Buttons */}
          <div className="flex flex-col sm:flex-row gap-4 justify-center">
            {status?.status === "completed" && (
              <Button
                onClick={() => router.push(`/drafts/${draftId}`)}
                className="bg-gradient-to-r from-violet-600 to-blue-600 hover:from-violet-500 hover:to-blue-500"
              >
                View Scenes
                <ArrowRight className="w-4 h-4 ml-2" />
              </Button>
            )}
            {(status?.status === "failed" || status?.status === "expired") && (
              <Button
                onClick={() => router.push("/analyze")}
                variant="outline"
                className="border-slate-700 text-slate-300 hover:bg-slate-800"
              >
                Start New Analysis
              </Button>
            )}
          </div>

          {/* Scene count preview */}
          {status?.status === "completed" && status.scene_count > 0 && (
            <div className="mt-8 p-4 rounded-2xl bg-slate-800/30 border border-slate-700/50">
              <div className="flex items-center justify-center gap-6">
                <div className="text-center">
                  <div className="text-3xl font-bold text-emerald-400">
                    {status.scene_count}
                  </div>
                  <div className="text-sm text-slate-500">Scenes Found</div>
                </div>
                {status.warning_count > 0 && (
                  <div className="text-center">
                    <div className="text-3xl font-bold text-amber-400">
                      {status.warning_count}
                    </div>
                    <div className="text-sm text-slate-500">Warnings</div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
