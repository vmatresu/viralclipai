"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import {
    useDeleteDraft,
    useDraft,
    useProcessDraft,
    useProcessingEstimate,
    type DraftScene,
    type SceneSelection,
} from "@/lib/analysis";
import {
    AlertCircle,
    ArrowRight,
    Clock,
    ExternalLink,
    Film,
    Loader2,
    Sparkles,
    SplitSquareHorizontal,
    Trash2
} from "lucide-react";
import { useParams, useRouter } from "next/navigation";
import { useCallback, useMemo, useState } from "react";

// Style options for Full format
const FULL_STYLES = [
  { value: "intelligent_speaker", label: "Active Speaker" },
  { value: "intelligent", label: "Smart Face" },
  { value: "intelligent_motion", label: "Motion" },
  { value: "original", label: "Original" },
];

// Style options for Split format
const SPLIT_STYLES = [
  { value: "intelligent_split_speaker", label: "Active Speaker (Split)" },
  { value: "intelligent_split", label: "Smart Face (Split)" },
  { value: "intelligent_split_motion", label: "Motion (Split)" },
  { value: "split", label: "Static Split" },
];

export default function DraftPage() {
  const params = useParams();
  const router = useRouter();
  const draftId = params.draftId as string;

  const { draft, scenes, isLoading, error } = useDraft(draftId);
  const { trigger: processDraft, isMutating: isProcessing } = useProcessDraft();
  const { trigger: deleteDraftAction, isMutating: isDeleting } = useDeleteDraft();

  // Selection state: Map of sceneId -> { full: boolean, split: boolean }
  const [selections, setSelections] = useState<Record<number, { full: boolean; split: boolean }>>({});
  const [fullStyle, setFullStyle] = useState("intelligent_speaker");
  const [splitStyle, setSplitStyle] = useState("intelligent_split_speaker");
  const [processError, setProcessError] = useState<string | null>(null);

  // Initialize selections when scenes load
  useMemo(() => {
    if (scenes.length > 0 && Object.keys(selections).length === 0) {
      const initial: Record<number, { full: boolean; split: boolean }> = {};
      scenes.forEach((scene) => {
        // Auto-select high confidence scenes (or all if no confidence)
        const autoSelect = scene.confidence === undefined || scene.confidence === null || scene.confidence >= 0.7;
        initial[scene.id] = { full: autoSelect, split: false };
      });
      setSelections(initial);
    }
  }, [scenes, selections]);

  // Calculate selection counts
  const { selectedSceneIds, fullCount, splitCount } = useMemo(() => {
    const ids: number[] = [];
    let full = 0;
    let split = 0;

    Object.entries(selections).forEach(([id, sel]) => {
      if (sel.full || sel.split) {
        ids.push(Number(id));
        if (sel.full) full++;
        if (sel.split) split++;
      }
    });

    return { selectedSceneIds: ids, fullCount: full, splitCount: split };
  }, [selections]);

  // Get processing estimate
  const { estimate, isLoading: estimateLoading } = useProcessingEstimate(
    draftId,
    selectedSceneIds,
    fullCount,
    splitCount
  );

  const toggleScene = useCallback((sceneId: number, format: "full" | "split") => {
    setSelections((prev) => {
      const current = prev[sceneId] ?? { full: false, split: false };
      return {
        ...prev,
        [sceneId]: {
          ...current,
          [format]: !current[format],
        },
      };
    });
  }, []);

  const selectAll = useCallback((format: "full" | "split") => {
    setSelections((prev) => {
      const updated: Record<number, { full: boolean; split: boolean }> = { ...prev };
      scenes.forEach((scene) => {
        const current = updated[scene.id] ?? { full: false, split: false };
        updated[scene.id] = {
          ...current,
          [format]: true,
        };
      });
      return updated;
    });
  }, [scenes]);

  const deselectAll = useCallback(() => {
    setSelections((prev) => {
      const updated = { ...prev };
      Object.keys(updated).forEach((id) => {
        updated[Number(id)] = { full: false, split: false };
      });
      return updated;
    });
  }, []);

  const handleProcess = async () => {
    setProcessError(null);

    if (selectedSceneIds.length === 0) {
      setProcessError("Please select at least one scene");
      return;
    }

    const sceneSelections: SceneSelection[] = selectedSceneIds.map((id) => ({
      scene_id: id,
      render_full: selections[id]?.full ?? false,
      render_split: selections[id]?.split ?? false,
    }));

    try {
      const result = await processDraft({
        draftId,
        request: {
          analysis_draft_id: draftId,
          selected_scenes: sceneSelections,
          full_style: fullStyle,
          split_style: splitStyle,
        },
      });

      // Redirect to history page for the new video
      router.push(`/history/${result.video_id}`);
    } catch (err) {
      setProcessError(err instanceof Error ? err.message : "Failed to start processing");
    }
  };

  const handleDelete = async () => {
    if (!confirm("Are you sure you want to delete this draft? This cannot be undone.")) {
      return;
    }

    try {
      await deleteDraftAction(draftId);
      router.push("/analyze");
    } catch (err) {
      setProcessError(err instanceof Error ? err.message : "Failed to delete draft");
    }
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
        <Loader2 className="w-8 h-8 text-violet-400 animate-spin" />
      </div>
    );
  }

  if (error || !draft) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
        <Card className="max-w-md bg-slate-900/50 border-slate-800/50">
          <CardContent className="pt-6 text-center">
            <AlertCircle className="w-12 h-12 text-red-400 mx-auto mb-4" />
            <h2 className="text-xl font-semibold text-white mb-2">Error</h2>
            <p className="text-slate-400 mb-4">{error?.message || "Draft not found"}</p>
            <Button onClick={() => router.push("/analyze")}>
              Start New Analysis
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      {/* Decorative elements */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -top-40 -right-40 w-96 h-96 bg-violet-500/10 rounded-full blur-3xl" />
        <div className="absolute bottom-20 -left-40 w-96 h-96 bg-blue-500/10 rounded-full blur-3xl" />
      </div>

      <div className="relative container mx-auto px-4 py-8">
        {/* Header */}
        <div className="mb-8">
          <div className="flex items-center gap-2 text-sm text-slate-500 mb-2">
            <Sparkles className="w-4 h-4 text-violet-400" />
            <span>Analysis Complete</span>
          </div>
          <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
            <div>
              <h1 className="text-2xl md:text-3xl font-bold text-white">
                {draft.video_title || "Untitled Video"}
              </h1>
              <p className="text-slate-400 mt-1">
                {scenes.length} scenes detected • Select scenes and styles to render
              </p>
            </div>
            <div className="flex items-center gap-3">
              <a
                href={draft.source_url}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors"
              >
                <ExternalLink className="w-4 h-4" />
                View Original
              </a>
              <Button
                variant="outline"
                size="sm"
                onClick={handleDelete}
                disabled={isDeleting}
                className="border-red-500/20 text-red-400 hover:bg-red-500/10"
              >
                {isDeleting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Trash2 className="w-4 h-4" />}
              </Button>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Scenes List */}
          <div className="lg:col-span-2 space-y-4">
            {/* Quick Actions */}
            <div className="flex items-center gap-3 flex-wrap">
              <Button
                variant="outline"
                size="sm"
                onClick={() => selectAll("full")}
                className="border-slate-700 text-slate-300 hover:bg-slate-800"
              >
                <Film className="w-4 h-4 mr-2" />
                Select All Full
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => selectAll("split")}
                className="border-slate-700 text-slate-300 hover:bg-slate-800"
              >
                <SplitSquareHorizontal className="w-4 h-4 mr-2" />
                Select All Split
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={deselectAll}
                className="text-slate-500 hover:text-slate-300"
              >
                Clear All
              </Button>
            </div>

            {/* Scene Cards */}
            <div className="space-y-3">
              {scenes.map((scene) => (
                <SceneCard
                  key={scene.id}
                  scene={scene}
                  selection={selections[scene.id] || { full: false, split: false }}
                  onToggle={toggleScene}
                />
              ))}
            </div>
          </div>

          {/* Sidebar - Style Selection & Summary */}
          <div className="space-y-6">
            {/* Global Style Selectors */}
            <Card className="bg-slate-900/50 border-slate-800/50 backdrop-blur-xl sticky top-4">
              <CardHeader>
                <CardTitle className="text-lg text-white">Output Settings</CardTitle>
                <CardDescription className="text-slate-400">
                  Choose styles for your clips
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                {/* Full Style */}
                <div className="space-y-2">
                  <label className="text-sm font-medium text-slate-300 flex items-center gap-2">
                    <Film className="w-4 h-4 text-blue-400" />
                    Full Screen Style
                  </label>
                  <Select value={fullStyle} onValueChange={setFullStyle}>
                    <SelectTrigger className="bg-slate-800/50 border-slate-700 text-white">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent className="bg-slate-900 border-slate-700">
                      {FULL_STYLES.map((style) => (
                        <SelectItem key={style.value} value={style.value} className="text-white">
                          {style.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                {/* Split Style */}
                <div className="space-y-2">
                  <label className="text-sm font-medium text-slate-300 flex items-center gap-2">
                    <SplitSquareHorizontal className="w-4 h-4 text-emerald-400" />
                    Split Screen Style
                  </label>
                  <Select value={splitStyle} onValueChange={setSplitStyle}>
                    <SelectTrigger className="bg-slate-800/50 border-slate-700 text-white">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent className="bg-slate-900 border-slate-700">
                      {SPLIT_STYLES.map((style) => (
                        <SelectItem key={style.value} value={style.value} className="text-white">
                          {style.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </CardContent>
            </Card>

            {/* Cost Estimate */}
            <Card className="bg-slate-900/50 border-slate-800/50 backdrop-blur-xl">
              <CardHeader>
                <CardTitle className="text-lg text-white">Estimate</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                {estimateLoading ? (
                  <div className="flex items-center justify-center py-4">
                    <Loader2 className="w-5 h-5 text-violet-400 animate-spin" />
                  </div>
                ) : estimate ? (
                  <>
                    <div className="grid grid-cols-2 gap-4 text-center">
                      <div className="p-3 rounded-lg bg-slate-800/50">
                        <div className="text-2xl font-bold text-white">
                          {estimate.full_render_count + estimate.split_render_count}
                        </div>
                        <div className="text-xs text-slate-500">Clips</div>
                      </div>
                      <div className="p-3 rounded-lg bg-slate-800/50">
                        <div className="text-2xl font-bold text-violet-400">
                          {estimate.estimated_credits}
                        </div>
                        <div className="text-xs text-slate-500">Credits</div>
                      </div>
                    </div>
                    <div className="flex items-center gap-2 text-sm text-slate-400">
                      <Clock className="w-4 h-4" />
                      <span>
                        ~{Math.ceil(estimate.estimated_time_min_secs / 60)}-
                        {Math.ceil(estimate.estimated_time_max_secs / 60)} min
                      </span>
                    </div>
                    {estimate.exceeds_quota && (
                      <div className="p-3 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400 text-sm">
                        <AlertCircle className="w-4 h-4 inline mr-2" />
                        This would exceed your quota
                      </div>
                    )}
                  </>
                ) : (
                  <p className="text-slate-500 text-sm text-center py-4">
                    Select scenes to see estimate
                  </p>
                )}
              </CardContent>
            </Card>

            {/* Process Button */}
            {processError && (
              <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
                {processError}
              </div>
            )}
            <Button
              onClick={handleProcess}
              disabled={isProcessing || selectedSceneIds.length === 0 || estimate?.exceeds_quota}
              className="w-full h-12 bg-gradient-to-r from-violet-600 to-blue-600 hover:from-violet-500 hover:to-blue-500 text-white font-semibold shadow-lg shadow-violet-500/25"
            >
              {isProcessing ? (
                <>
                  <Loader2 className="w-5 h-5 mr-2 animate-spin" />
                  Starting...
                </>
              ) : (
                <>
                  Process {fullCount + splitCount} Clips
                  <ArrowRight className="w-5 h-5 ml-2" />
                </>
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function SceneCard({
  scene,
  selection,
  onToggle,
}: {
  scene: DraftScene;
  selection: { full: boolean; split: boolean };
  onToggle: (id: number, format: "full" | "split") => void;
}) {
  const formatDuration = (secs: number) => {
    const mins = Math.floor(secs / 60);
    const remSecs = secs % 60;
    return mins > 0 ? `${mins}m ${remSecs}s` : `${secs}s`;
  };

  const isSelected = selection.full || selection.split;

  return (
    <Card className={`bg-slate-900/50 border-slate-800/50 backdrop-blur-xl transition-all duration-200 ${
      isSelected ? "ring-2 ring-violet-500/50" : ""
    }`}>
      <CardContent className="p-4">
        <div className="flex flex-col md:flex-row md:items-center gap-4">
          {/* Scene Info */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-xs font-medium px-2 py-0.5 rounded bg-slate-800 text-slate-400">
                #{scene.id}
              </span>
              <span className="text-xs text-slate-500">
                {scene.start} → {scene.end}
              </span>
              <Badge variant="outline" className="text-xs border-slate-700 text-slate-400">
                {formatDuration(scene.duration_secs)}
              </Badge>
              {scene.hook_category && (
                <Badge variant="outline" className="text-xs border-violet-500/30 text-violet-400">
                  {scene.hook_category}
                </Badge>
              )}
            </div>
            <h3 className="text-white font-medium truncate">{scene.title}</h3>
            {scene.reason && (
              <p className="text-sm text-slate-400 mt-1 line-clamp-2">{scene.reason}</p>
            )}
          </div>

          {/* Format toggles */}
          <div className="flex items-center gap-4 shrink-0">
            <label className="flex items-center gap-2 cursor-pointer group">
              <Checkbox
                checked={selection.full}
                onCheckedChange={() => onToggle(scene.id, "full")}
                className="border-slate-600 data-[state=checked]:bg-blue-600 data-[state=checked]:border-blue-600"
              />
              <span className="text-sm text-slate-400 group-hover:text-white transition-colors flex items-center gap-1">
                <Film className="w-4 h-4" />
                Full
              </span>
            </label>
            <label className="flex items-center gap-2 cursor-pointer group">
              <Checkbox
                checked={selection.split}
                onCheckedChange={() => onToggle(scene.id, "split")}
                className="border-slate-600 data-[state=checked]:bg-emerald-600 data-[state=checked]:border-emerald-600"
              />
              <span className="text-sm text-slate-400 group-hover:text-white transition-colors flex items-center gap-1">
                <SplitSquareHorizontal className="w-4 h-4" />
                Split
              </span>
            </label>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
