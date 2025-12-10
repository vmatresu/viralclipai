"use client";

import { ArrowRight, Crown, Link2, Lock, Sparkles } from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { analyticsEvents } from "@/lib/analytics";
import { apiFetch } from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";
import { frontendLogger } from "@/lib/logger";
import { sanitizePrompt, validateVideoUrl } from "@/lib/security";
import { cn } from "@/lib/utils";
import { type ClipProcessingStep } from "@/lib/websocket";
import {
  handleWSMessage,
  type MessageHandlerCallbacks,
} from "@/lib/websocket/messageHandler";
import { createWebSocketConnection, getWebSocketUrl } from "@/lib/websocket-client";
import { type SceneProgress } from "@/types/processing";

import { DetailedProcessingStatus } from "../shared/DetailedProcessingStatus";

import {
  AiAssistanceSlider,
  getRequiredPlan,
  isTierGated,
  type AiLevel,
} from "./AiAssistanceSlider";
import { LayoutSelector, type LayoutOption } from "./LayoutSelector";

interface UserSettings {
  plan: string;
}

type StaticFocusOption = "left" | "center" | "right";

export function ProcessVideoInterface() {
  const router = useRouter();
  const { getIdToken, user, loading: authLoading } = useAuth();

  const [url, setUrl] = useState("");
  const [layout, setLayout] = useState<LayoutOption>("split");
  // Default to "motion" which is available for all users (free tier)
  const [aiLevel, setAiLevel] = useState<AiLevel>("motion");
  const [prompt, setPrompt] = useState("");
  const [exportOriginal, setExportOriginal] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [shouldAnimateInput, setShouldAnimateInput] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const [staticCropSide, setStaticCropSide] = useState<StaticFocusOption>("center");
  const [userPlan, setUserPlan] = useState<string | undefined>(undefined);
  const [showUpgradeDialog, setShowUpgradeDialog] = useState(false);

  // Progress tracking state
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [sceneProgress, setSceneProgress] = useState<Map<number, SceneProgress>>(
    new Map()
  );

  // Load user settings to get plan info
  const loadUserSettings = useCallback(async () => {
    if (authLoading || !user) {
      setUserPlan(undefined);
      return;
    }
    try {
      const token = await getIdToken();
      if (!token) {
        setUserPlan(undefined);
        return;
      }
      const settings = await apiFetch<UserSettings>("/api/settings", { token });
      setUserPlan(settings.plan);
    } catch (err) {
      frontendLogger.error("Failed to load user settings:", err);
      setUserPlan(undefined);
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

  // Scene progress handlers
  const handleSceneStarted = (
    sceneId: number,
    sceneTitle: string,
    styleCount: number,
    startSec: number,
    durationSec: number
  ) => {
    setSceneProgress((prev) => {
      const next = new Map(prev);
      next.set(sceneId, {
        sceneId,
        sceneTitle,
        styleCount,
        startSec,
        durationSec,
        status: "processing",
        clipsCompleted: 0,
        clipsFailed: 0,
        currentSteps: new Map(),
      });
      return next;
    });
  };

  const handleSceneCompleted = (
    sceneId: number,
    clipsCompleted: number,
    clipsFailed: number
  ) => {
    setSceneProgress((prev) => {
      const next = new Map(prev);
      const scene = next.get(sceneId);
      if (scene) {
        next.set(sceneId, {
          ...scene,
          status: clipsFailed > 0 ? "failed" : "completed",
          clipsCompleted,
          clipsFailed,
        });
      }
      return next;
    });
  };

  const handleClipProgress = (
    sceneId: number,
    style: string,
    step: ClipProcessingStep,
    details?: string
  ) => {
    setSceneProgress((prev) => {
      const next = new Map(prev);
      const scene = next.get(sceneId);
      if (scene) {
        const newSteps = new Map(scene.currentSteps);
        newSteps.set(style, { step: step as any, details });
        next.set(sceneId, { ...scene, currentSteps: newSteps });
      }
      return next;
    });
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
          urlValidation.error ||
            "Invalid video URL. Please use a supported platform (YouTube, Vimeo, TikTok, etc.)"
        );
        setIsProcessing(false);
        return;
      }
      const sanitizedUrl = urlValidation.sanitizedUrl;

      const sanitizedPrompt = sanitizePrompt(prompt.trim());
      const styles = getStylesFromSelection(layout, aiLevel, staticCropSide);
      const cropMode = exportOriginal ? "none" : "auto";

      // Track processing start
      void analyticsEvents.videoProcessingStarted({
        style: styles.join(","),
        hasCustomPrompt: sanitizedPrompt.length > 0,
        videoUrl: sanitizedUrl,
      });

      // Create WebSocket connection
      const wsUrl = getWebSocketUrl(process.env.NEXT_PUBLIC_API_BASE_URL);
      const ws = createWebSocketConnection(
        wsUrl,
        // onOpen
        () => {
          frontendLogger.info("WebSocket connected, sending process request");
          ws.send(
            JSON.stringify({
              url: sanitizedUrl,
              styles,
              token,
              prompt: sanitizedPrompt || undefined,
              crop_mode: cropMode,
              target_aspect: "9:16",
            })
          );
        },
        // onMessage
        (message: unknown) => {
          const callbacks: MessageHandlerCallbacks = {
            onLog: (logMessage) => {
              // Backend already includes timestamp in message, don't add another
              log(logMessage, "info");
            },
            onProgress: (progressValue) => {
              setProgress(progressValue);
            },
            onError: (errorMessage, errorDetails) => {
              ws.close();
              toast.error(errorMessage);
              setIsProcessing(false);
              void analyticsEvents.videoProcessingFailed({
                errorType: errorDetails ?? "unknown",
                errorMessage,
                style: styles.join(","),
              });
            },
            onDone: (videoId) => {
              ws.close();
              setIsProcessing(false);
              toast.success("Video processed successfully!");

              // Navigate to history page with video ID
              router.push(`/history/${videoId}`);
            },
            onClipUploaded: (_videoId, clipCount, totalClips) => {
              if (clipCount > 0 && totalClips > 0) {
                log(`📦 Clip ${clipCount}/${totalClips} uploaded`, "success");
              }
            },
            onSceneStarted: handleSceneStarted,
            onSceneCompleted: handleSceneCompleted,
            onClipProgress: handleClipProgress,
          };

          const handled = handleWSMessage(message, callbacks, null);
          if (!handled) {
            frontendLogger.error("Invalid WebSocket message format", { message });
            ws.close();
            toast.error("Invalid message format");
            setIsProcessing(false);
          }
        },
        // onError
        () => {
          frontendLogger.error("WebSocket error occurred");
          toast.error("Connection error occurred");
        },
        // onClose
        () => {
          setIsProcessing(false);
        }
      );

      // Register job in processing context for global tracking
      // We don't have the videoId yet, but we'll track it when we get the Done message
      // For now, just mark that processing has started
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
    <div className="w-full max-w-4xl mx-auto space-y-8 p-4 md:p-8 rounded-2xl border border-brand-100/80 bg-white/90 backdrop-blur-xl shadow-[0_30px_80px_rgba(99,102,241,0.15)] relative overflow-hidden dark:border-white/10 dark:bg-background/50 dark:shadow-2xl">
      {/* Glow effect background */}
      <div className="absolute -top-20 -right-20 w-64 h-64 bg-primary/10 rounded-full blur-[80px] pointer-events-none" />
      <div className="absolute -bottom-20 -left-20 w-64 h-64 bg-indigo-500/10 rounded-full blur-[80px] pointer-events-none" />

      {/* Header */}
      <div className="space-y-2 relative">
        <h2 className="text-2xl font-bold tracking-tight flex items-center gap-2">
          <Sparkles className="w-5 h-5 text-primary" />
          Process Video
        </h2>
        <p className="text-muted-foreground">
          Paste a YouTube link to generate AI-edited vertical clips.
        </p>
      </div>

      {/* Step 1: Input */}
      <div className="space-y-4">
        <div className="relative group">
          <div
            className={cn(
              "absolute -inset-0.5 rounded-xl bg-gradient-to-r from-primary to-indigo-500 opacity-20 blur transition duration-500",
              shouldAnimateInput
                ? "opacity-100 animate-pulse"
                : "group-hover:opacity-40"
            )}
          />
          <Input
            ref={inputRef}
            placeholder="Paste YouTube URL here..."
            className={cn(
              "relative h-14 pl-12 text-lg shadow-sm transition-all duration-300 border border-brand-100/80 bg-white text-foreground placeholder:text-muted-foreground/70 focus:ring-2 focus:ring-brand-500/25 focus:border-brand-300 dark:bg-black/40 dark:border-white/10 dark:placeholder:text-white/60",
              shouldAnimateInput
                ? "ring-2 ring-primary border-primary animate-shake"
                : undefined
            )}
            value={url}
            onChange={(e) => {
              setUrl(e.target.value);
              if (shouldAnimateInput && e.target.value) setShouldAnimateInput(false);
            }}
          />
          <Link2
            className={cn(
              "absolute left-4 top-1/2 -translate-y-1/2 w-6 h-6 transition-colors duration-300",
              shouldAnimateInput ? "text-primary" : "text-muted-foreground"
            )}
          />
        </div>

        {/* Optional Custom Prompt */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <Label className="text-xs text-muted-foreground uppercase tracking-wider font-semibold">
              Custom Instructions (Optional)
            </Label>
          </div>

          <Textarea
            placeholder="e.g. Find moments about crypto, funny jokes, or specific topics..."
            className="min-h-[100px] rounded-xl border border-brand-100/80 bg-white p-4 text-base leading-relaxed shadow-sm focus:border-brand-300 focus:ring-2 focus:ring-brand-500/25 dark:border-white/10 dark:bg-white/5"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
          />

          <div className="flex items-center gap-4 pt-2">
            <div className="hidden md:flex items-center gap-2 text-primary animate-pulse">
              <span className="text-xs font-bold uppercase tracking-widest whitespace-nowrap">
                Try these
              </span>
              <ArrowRight className="w-3 h-3" />
            </div>
            <div className="flex flex-wrap gap-2">
              {predefinedPrompts.map((p) => (
                <button
                  key={p.label}
                  onClick={() => handlePromptClick(p.prompt)}
                  className="text-xs px-3 py-1.5 rounded-full border transition-all font-medium bg-brand-50 text-brand-700 border-brand-100 hover:border-brand-200 hover:bg-brand-100/60 dark:bg-secondary/50 dark:text-secondary-foreground dark:border-white/5 dark:hover:border-white/20"
                >
                  + {p.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      <hr className="border-white/5" />

      {/* Step 2: Options - Vertical Stack */}
      <div className="space-y-12 relative">
        {/* Row 1: Layout */}
        <div className="space-y-6">
          <div className="flex items-center gap-3 mb-2">
            <span className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/20 text-primary text-sm font-bold border border-primary/20">
              1
            </span>
            <h3 className="text-xl font-semibold tracking-tight">Choose your layout</h3>
          </div>
          <div className="pl-11">
            <LayoutSelector selectedLayout={layout} onSelect={setLayout} />
          </div>
        </div>

        {/* Row 2: AI Level */}
        <div className="space-y-6">
          <div className="flex items-center gap-3 mb-2">
            <span className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/20 text-primary text-sm font-bold border border-primary/20">
              2
            </span>
            <h3 className="text-xl font-semibold tracking-tight">
              Select detection tier
            </h3>
          </div>
          <div className="pl-11">
            <AiAssistanceSlider
              value={aiLevel}
              onChange={setAiLevel}
              userPlan={userPlan}
            />
            {layout === "full" && aiLevel === "static" && (
              <div className="mt-4 flex flex-wrap items-center gap-3">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
                  Static focus
                </Label>
                <div className="inline-flex rounded-full border border-white/10 bg-white/5 p-1 shadow-sm">
                  {(["left", "center", "right"] as StaticFocusOption[]).map((side) => {
                    const isActive = staticCropSide === side;
                    return (
                      <button
                        key={side}
                        type="button"
                        onClick={() => setStaticCropSide(side)}
                        className={cn(
                          "px-3 py-1.5 text-xs font-semibold rounded-full transition-colors",
                          isActive
                            ? "bg-primary text-white"
                            : "text-muted-foreground hover:text-foreground"
                        )}
                      >
                        {side === "left"
                          ? "Left"
                          : side === "right"
                            ? "Right"
                            : "Center"}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
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

      <hr className="border-white/5" />

      {/* Footer Actions */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-6 pt-4">
        <div className="flex items-center space-x-3">
          <Checkbox
            id="export-orig"
            checked={exportOriginal}
            onCheckedChange={(c) => setExportOriginal(Boolean(c))}
            className="w-5 h-5 border-white/20 data-[state=checked]:bg-primary data-[state=checked]:border-primary"
          />
          <Label
            htmlFor="export-orig"
            className="cursor-pointer text-muted-foreground hover:text-foreground transition-colors text-base"
          >
            Also export clipped segments in original landscape format (no cropping)
          </Label>
        </div>

        <Button
          size="lg"
          className={cn(
            "w-full md:w-auto text-lg h-14 px-8 transition-all duration-300",
            isSelectedTierGated
              ? "bg-gradient-to-r from-amber-500 to-orange-500 hover:from-amber-600 hover:to-orange-600 shadow-[0_0_20px_-5px_theme(colors.amber.500)] hover:shadow-[0_0_30px_-5px_theme(colors.amber.500)]"
              : "shadow-[0_0_20px_-5px_theme(colors.primary.DEFAULT)] hover:shadow-[0_0_30px_-5px_theme(colors.primary.DEFAULT)]"
          )}
          onClick={handleLaunch}
          disabled={isProcessing}
        >
          {isProcessing ? (
            "Processing..."
          ) : isSelectedTierGated ? (
            <>
              <Lock className="mr-2 w-5 h-5" />
              Upgrade to {requiredPlan}
            </>
          ) : (
            <>
              Launch Processor
              <ArrowRight className="ml-2 w-5 h-5" />
            </>
          )}
        </Button>
      </div>

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
