"use client";

import { ArrowRight, Link2, Sparkles } from "lucide-react";
import { useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

import { AiAssistanceSlider, type AiLevel } from "./AiAssistanceSlider";
import { LayoutSelector, type LayoutOption } from "./LayoutSelector";

export function ProcessVideoInterface() {
  const [url, setUrl] = useState("");
  const [layout, setLayout] = useState<LayoutOption>("split");
  const [aiLevel, setAiLevel] = useState<AiLevel>("face_aware");
  const [prompt, setPrompt] = useState("");
  const [exportOriginal, setExportOriginal] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [shouldAnimateInput, setShouldAnimateInput] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const predefinedPrompts = [
    {
      label: "Emotional moments",
      prompt: "Find the most emotional and vulnerable moments in this video that would resonate strongly on TikTok"
    },
    {
      label: "Best viral clips",
      prompt: "Find the best high-retention viral clip candidates for short-form social media (TikTok, Shorts, Reels)"
    },
    {
      label: "High energy discussion",
      prompt: "Find segments with intense discussion about the main subject, where there is strong opinion or debate"
    },
    {
      label: "Funny references",
      prompt: "Find funny references, jokes, or humorous moments that would work well for comedy content"
    },
    {
      label: "Sound-focused clips",
      prompt: "Find moments with interesting sounds or reactions that would work well in sound-on social media clips"
    },
  ];

  const handlePromptClick = (promptText: string) => {
    setPrompt(promptText);
  };

  const handleLaunch = () => {
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

    // TODO: Connect to backend
    setIsProcessing(true);
    setTimeout(() => setIsProcessing(false), 2000);
    console.log("Launching job:", { url, layout, aiLevel, prompt, exportOriginal });
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
              shouldAnimateInput ? "ring-2 ring-primary border-primary animate-shake" : undefined
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
              Select intelligence level
            </h3>
          </div>
          <div className="pl-11">
            <AiAssistanceSlider value={aiLevel} onChange={setAiLevel} />
          </div>
        </div>
      </div>

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
          className="w-full md:w-auto text-lg h-14 px-8 shadow-[0_0_20px_-5px_theme(colors.primary.DEFAULT)] hover:shadow-[0_0_30px_-5px_theme(colors.primary.DEFAULT)] transition-all duration-300"
          onClick={handleLaunch}
          disabled={isProcessing}
        >
          {isProcessing ? "Processing..." : "Launch Processor"}
          {!isProcessing && <ArrowRight className="ml-2 w-5 h-5" />}
        </Button>
      </div>
    </div>
  );
}
