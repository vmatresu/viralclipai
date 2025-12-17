"use client";

import {
  AlertCircle,
  CornerRightDown,
  Crown,
  Sparkles,
  TrendingUp,
} from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { sanitizePrompt, validateVideoUrl } from "@/lib/security";
import { cn } from "@/lib/utils";
import { type SceneProgress } from "@/types/processing";

import { DetailedProcessingStatus } from "../shared/DetailedProcessingStatus";

import { getRequiredPlan, isTierGated, type AiLevel } from "./AiAssistanceSlider";
import { type LayoutOption } from "./LayoutSelector";

interface StorageInfo {
  used_bytes: number;
  limit_bytes: number;
  used_formatted: string;
  limit_formatted: string;
}

interface UserSettings {
  plan: string;
  max_clips_per_month: number;
  clips_used_this_month: number;
  storage: StorageInfo;
}

type StaticFocusOption = "left" | "center" | "right";

export function ProcessVideoInterface() {
  const router = useRouter();
  const { getIdToken, user, loading: authLoading } = useAuth();

  const [url, setUrl] = useState("");
  // Default layout and AI level - these are now fixed defaults since the UI was removed
  // Users will select styles per-scene in the draft selection screen after analysis
  const layout = "split" as LayoutOption;
  const aiLevel = "motion" as AiLevel;
  const staticCropSide = "center" as StaticFocusOption;
  const [prompt, setPrompt] = useState("");
  const [exportOriginal] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [shouldAnimateInput, setShouldAnimateInput] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const [userPlan, setUserPlan] = useState<string | undefined>(undefined);
  const [showUpgradeDialog, setShowUpgradeDialog] = useState(false);

  // Quota tracking state
  const [quotaInfo, setQuotaInfo] = useState<{
    clipsUsed: number;
    clipsLimit: number;
    storageUsed: number;
    storageLimit: number;
    storageUsedFormatted: string;
    storageLimitFormatted: string;
  } | null>(null);

  // Progress tracking state
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [sceneProgress, setSceneProgress] = useState<Map<number, SceneProgress>>(
    new Map()
  );

  // Load user settings to get plan info and quota
  const loadUserSettings = useCallback(async () => {
    if (authLoading || !user) {
      setUserPlan(undefined);
      setQuotaInfo(null);
      return;
    }
    try {
      const token = await getIdToken();
      if (!token) {
        setUserPlan(undefined);
        setQuotaInfo(null);
        return;
      }
      const settings = await apiFetch<UserSettings>("/api/settings", { token });
      setUserPlan(settings.plan);
      setQuotaInfo({
        clipsUsed: settings.clips_used_this_month,
        clipsLimit: settings.max_clips_per_month,
        storageUsed: settings.storage.used_bytes,
        storageLimit: settings.storage.limit_bytes,
        storageUsedFormatted: settings.storage.used_formatted,
        storageLimitFormatted: settings.storage.limit_formatted,
      });
    } catch (err) {
      frontendLogger.error("Failed to load user settings:", err);
      setUserPlan(undefined);
      setQuotaInfo(null);
    }
  }, [authLoading, user, getIdToken]);

  useEffect(() => {
    void loadUserSettings();
  }, [loadUserSettings]);

  const predefinedPrompts = [
    {
      label: "Emotional moments",
      prompt:
        "Find the most emotional and vulnerable moments in this video that would resonate strongly on TikTok",
    },
    {
      label: "Best viral clips",
      prompt:
        "Find the best high-retention viral clip candidates for short-form social media (TikTok, Shorts, Reels)",
    },
    {
      label: "High energy discussion",
      prompt:
        "Find segments with intense discussion about the main subject, where there is strong opinion or debate",
    },
    {
      label: "Funny references",
      prompt:
        "Find funny references, jokes, or humorous moments that would work well for comedy content",
    },
    {
      label: "Sound-focused clips",
      prompt:
        "Find moments with interesting sounds or reactions that would work well in sound-on social media clips",
    },
  ];

  const handlePromptClick = (promptText: string) => {
    setPrompt(promptText);
  };

  // Helper function to add log messages
  const log = (
    msg: string,
    type: "info" | "error" | "success" = "info",
    timestamp?: string
  ) => {
    let prefix = ">";
    if (type === "error") {
      prefix = "[ERROR]";
    } else if (type === "success") {
      prefix = "[OK]";
    }

    // Format timestamp if provided
    let timestampStr = "";
    if (timestamp) {
      try {
        const date = new Date(timestamp);
        timestampStr = date.toLocaleTimeString("en-US", {
          hour12: false,
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        });
        timestampStr = `[${timestampStr}] `;
      } catch {
        // Ignore timestamp parsing errors
      }
    }

    setLogs((prev) => [...prev, `${timestampStr}${prefix} ${msg}`]);
  };

  // Map UI selections to backend styles
  const getStylesFromSelection = (
    layout: LayoutOption,
    aiTier: AiLevel,
    cropSide: StaticFocusOption = "center"
  ): string[] => {
    const isSplit = layout === "split";

    switch (aiTier) {
      case "static":
        if (isSplit) return ["split_fast"];
        switch (cropSide) {
          case "right":
            return ["right_focus"];
          case "center":
            return ["center_focus"];
          default:
            return ["left_focus"];
        }
      case "motion":
        return isSplit ? ["intelligent_split_motion"] : ["intelligent_motion"];
      case "basic_face":
        return isSplit ? ["intelligent_split"] : ["intelligent"];
      case "active_face":
        return isSplit ? ["intelligent_split_speaker"] : ["intelligent_speaker"];
      default:
        return isSplit ? ["intelligent_split"] : ["intelligent"];
    }
  };

  // Check if the selected tier requires an upgrade
  const isSelectedTierGated = isTierGated(aiLevel, userPlan);
  const requiredPlan = getRequiredPlan(aiLevel);

  // Check if user is over quota (clips or storage)
  const isOverClipQuota = quotaInfo
    ? quotaInfo.clipsUsed >= quotaInfo.clipsLimit
    : false;
  const isOverStorageQuota = quotaInfo
    ? quotaInfo.storageUsed >= quotaInfo.storageLimit
    : false;
  const isOverQuota = isOverClipQuota || isOverStorageQuota;

  const handleLaunch = async () => {
    if (!url) {
      // Validation: If no URL, focus input and trigger attention animation
      if (inputRef.current) {
        inputRef.current.focus();
      }
      setShouldAnimateInput(true);
      // Reset animation state after it plays to allow re-triggering
      setTimeout(() => setShouldAnimateInput(false), 1000);
      return;
    }

    // Check if the selected tier requires an upgrade
    if (isSelectedTierGated) {
      setShowUpgradeDialog(true);
      return;
    }

    // Check if user is over quota
    if (isOverQuota) {
      toast.error(
        "You've exceeded your plan limits. Please upgrade or delete existing clips."
      );
      return;
    }

    setIsProcessing(true);
    // Reset progress state
    setLogs([]);
    setProgress(0);
    setSceneProgress(new Map());

    try {
      // Get auth token
      const token = await getIdToken();
      if (!token) {
        toast.error("Please sign in with your Google account to use this app.");
        setIsProcessing(false);
        return;
      }

      // SECURITY: Validate and sanitize inputs before sending to backend
      const urlValidation = validateVideoUrl(url);
      if (!urlValidation.isValid || !urlValidation.sanitizedUrl) {
        toast.error(
          urlValidation.error ??
            "Invalid video URL. Please use a supported platform (YouTube, Vimeo, TikTok, etc.)"
        );
        setIsProcessing(false);
        return;
      }
      const sanitizedUrl = urlValidation.sanitizedUrl;

      const sanitizedPrompt = sanitizePrompt(prompt.trim());
      const styles = getStylesFromSelection(layout, aiLevel, staticCropSide);
      const cropMode = exportOriginal ? "none" : "intelligent";

      // Track processing start
      void analyticsEvents.videoProcessingStarted({
        style: styles.join(","),
        hasCustomPrompt: sanitizedPrompt.length > 0,
        videoUrl: sanitizedUrl,
      });

      // Submit via REST API instead of WebSocket
      log("Submitting video for processing...", "info");

      const response = await apiFetch<{
        video_id: string;
        job_id: string;
        status: string;
        message?: string;
      }>("/api/videos/process", {
        method: "POST",
        token,
        body: {
          url: sanitizedUrl,
          styles,
          prompt: sanitizedPrompt || undefined,
          crop_mode: cropMode,
          target_aspect: "9:16",
        },
      });

      log("Processing started! Redirecting to history page...", "success");
      setProgress(10);

      toast.success(
        "Video submitted for processing! You'll be redirected to track progress."
      );

      // Navigate to history page with video ID
      setTimeout(() => {
        router.push(`/history/${response.video_id}`);
      }, 1500);
    } catch (err: unknown) {
      frontendLogger.error("Failed to start processing", err);
      const errorMessage =
        err instanceof Error ? err.message : "Failed to start processing";
      toast.error(errorMessage);
      setIsProcessing(false);

      void analyticsEvents.videoProcessingFailed({
        errorType: "initialization_error",
        errorMessage,
        style: getStylesFromSelection(layout, aiLevel, staticCropSide).join(","),
      });
    }
  };

  return (
    <div className="w-full max-w-4xl mx-auto space-y-8 p-6 md:p-8 rounded-3xl glass-card relative overflow-hidden bg-white/95 dark:bg-black/80">

      {/* Header */}
      <div className="space-y-2 relative mb-6">
        <h2 className="text-2xl font-bold tracking-tight text-foreground">
          Process Video
        </h2>
        <p className="text-muted-foreground">
          Paste a YouTube link to generate AI-edited vertical clips.
        </p>
      </div>

      {/* Over Quota Warning Banner */}
      {isOverQuota && quotaInfo && (
        <div className="rounded-xl border border-destructive bg-destructive/10 p-4 space-y-3 mb-6">
          <div className="flex items-start gap-3">
            <AlertCircle className="h-5 w-5 text-destructive mt-0.5 flex-shrink-0" />
            <div className="flex-1 space-y-2">
              <p className="font-semibold text-destructive">
                You&apos;ve exceeded your plan limits!
              </p>
              <p className="text-sm text-muted-foreground">
                {isOverClipQuota && (
                  <>
                    You&apos;ve used {quotaInfo.clipsUsed} of {quotaInfo.clipsLimit}{" "}
                    monthly clips.{" "}
                  </>
                )}
                {isOverStorageQuota && (
                  <>
                    Storage is full ({quotaInfo.storageUsedFormatted} /{" "}
                    {quotaInfo.storageLimitFormatted}).{" "}
                  </>
                )}
                You cannot create new clips until you upgrade or delete existing clips.
              </p>
              <div className="flex gap-2 mt-2">
                <Button asChild variant="default" size="sm">
                  <Link href="/pricing">
                    <TrendingUp className="h-4 w-4 mr-2" />
                    Upgrade Plan
                  </Link>
                </Button>
                <Button asChild variant="outline" size="sm">
                  <Link href="/history">Manage Clips</Link>
                </Button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Main Input Area matching mockup-header */}
      <div className="flex flex-col md:flex-row gap-4 mb-6">
        {/* Input Field Wrapper matching .input-field */}
        <div className="flex-1 flex items-center gap-3 bg-muted/50 dark:bg-white/[0.03] backdrop-blur-[20px] border border-border dark:border-white/[0.08] rounded-lg p-4 shadow-inner group transition-all duration-300 focus-within:border-[#A45CFF]/50 focus-within:bg-background dark:focus-within:bg-white/[0.05]">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            className="w-[18px] h-[18px] text-muted-foreground opacity-50"
          >
            <path
              d="M13.5 6H5.25A2.25 2.25 0 003 8.25v10.5A2.25 2.25 0 005.25 21h10.5A2.25 2.25 0 0018 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <input
            ref={inputRef}
            placeholder="Paste your YouTube link…"
            className="flex-1 bg-transparent border-none p-0 text-sm text-foreground placeholder:text-muted-foreground focus:ring-0 focus:outline-none"
            value={url}
            onChange={(e) => {
              setUrl(e.target.value);
              if (shouldAnimateInput && e.target.value) setShouldAnimateInput(false);
            }}
          />
        </div>

        {/* Analyze Button matching .analyze-btn */}
        <button
          onClick={handleLaunch}
          disabled={isProcessing || isOverQuota}
          className={cn(
            "bg-gradient-to-br from-[#A45CFF] to-[#5CFFF9] text-[#05060D] px-6 py-4 rounded-lg font-semibold text-sm whitespace-nowrap transition-transform active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed",
            isProcessing && "opacity-70 cursor-wait"
          )}
        >
          {isProcessing ? "Processing..." : "Analyze"}
        </button>
      </div>

      {/* Custom Prompt Section (Moved below but kept functional) */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label className="text-xs text-muted-foreground uppercase tracking-wider font-semibold">
            Custom Instructions (Optional)
          </Label>
        </div>

        <Textarea
          placeholder="e.g. Find moments about crypto, funny jokes, or specific topics..."
          className="min-h-[80px] rounded-xl border border-border dark:border-white/10 bg-muted/30 dark:bg-white/5 p-4 text-sm leading-relaxed shadow-sm focus:border-[#A45CFF]/50 focus:ring-2 focus:ring-[#A45CFF]/25 text-foreground placeholder:text-muted-foreground"
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
        />

        {/* Chips with Sticker */}
        <div className="relative mt-4 pt-4">
          <div className="flex items-center gap-2 mb-3 md:hidden">
            <div className="bg-gradient-to-r from-[#A45CFF] to-[#5CFFF9] text-[#05060D] text-[10px] uppercase tracking-wider font-bold px-3 py-1 rounded-sm shadow-sm border border-white/20 flex items-center gap-1.5">
              <Sparkles className="w-3 h-3" />
              Try these:
            </div>
          </div>
          <div className="hidden md:flex items-center gap-2 mb-3">
            <div className="bg-gradient-to-r from-[#A45CFF] to-[#5CFFF9] text-[#05060D] text-[10px] uppercase tracking-wider font-bold px-3 py-1 rounded-sm -rotate-2 shadow-sm border border-white/20 flex items-center gap-1.5">
              <Sparkles className="w-3 h-3" />
              Try these:
            </div>
            <CornerRightDown
              className="w-4 h-4 text-[#A45CFF]"
              strokeWidth={2.5}
            />
          </div>

          <div className="flex flex-wrap gap-2">
            {predefinedPrompts.map((p) => (
              <button
                key={p.label}
                onClick={() => handlePromptClick(p.prompt)}
                className="text-xs px-3 py-1.5 rounded-full border transition-all font-medium bg-[#A45CFF]/5 dark:bg-[#A45CFF]/10 text-[#A45CFF] border-[#A45CFF]/20 hover:border-[#A45CFF]/40 hover:bg-[#A45CFF]/10 dark:hover:bg-[#A45CFF]/20 active:scale-95"
              >
                + {p.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Progress Display */}
      {isProcessing && (
        <div className="mt-8">
          <DetailedProcessingStatus
            progress={progress}
            logs={logs}
            sceneProgress={sceneProgress}
          />
        </div>
      )}

      {/* Upgrade Dialog */}
      <Dialog open={showUpgradeDialog} onOpenChange={setShowUpgradeDialog}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Crown className="h-5 w-5 text-amber-400" />
              Upgrade to {requiredPlan}
            </DialogTitle>
            <DialogDescription className="pt-2">
              {aiLevel === "active_face" ? (
                <>
                  <span className="font-semibold text-foreground">Premium AI</span> uses
                  advanced face mesh tracking to follow whoever is actively speaking.
                  This feature is available on the{" "}
                  <span className="font-semibold text-amber-400">Studio</span> plan.
                </>
              ) : (
                <>
                  <span className="font-semibold text-foreground">Smart Face</span> uses
                  AI face detection to keep the main subject perfectly centered. This
                  feature is available on{" "}
                  <span className="font-semibold text-blue-400">Pro</span> and{" "}
                  <span className="font-semibold text-amber-400">Studio</span> plans.
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <div className="rounded-lg border border-white/10 bg-white/5 p-4 space-y-3">
              <div className="flex items-center gap-2 text-sm">
                <Sparkles className="h-4 w-4 text-primary" />
                <span>Unlock premium AI detection tiers</span>
              </div>
              <div className="flex items-center gap-2 text-sm">
                <Sparkles className="h-4 w-4 text-primary" />
                <span>Process more videos per month</span>
              </div>
              <div className="flex items-center gap-2 text-sm">
                <Sparkles className="h-4 w-4 text-primary" />
                <span>Priority processing queue</span>
              </div>
            </div>
          </div>
          <DialogFooter className="flex-col sm:flex-row gap-2">
            <Button
              variant="outline"
              onClick={() => setShowUpgradeDialog(false)}
              className="w-full sm:w-auto"
            >
              Maybe later
            </Button>
            <Button
              asChild
              className="w-full sm:w-auto bg-gradient-to-r from-amber-500 to-orange-500 hover:from-amber-600 hover:to-orange-600"
            >
              <Link href="/pricing">
                <Crown className="mr-2 h-4 w-4" />
                View Plans
              </Link>
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
