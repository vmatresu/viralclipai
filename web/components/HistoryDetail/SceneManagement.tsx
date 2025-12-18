"use client";

import {
  AlertCircle,
  Check,
  Clock,
  Loader2,
  Plus,
  Sparkles,
  Upload,
} from "lucide-react";
import { useCallback, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  addScene,
  bulkAddScenes,
  generateMoreScenes,
  updateSceneTimestamps,
  type AddSceneRequest,
  type BulkSceneEntry,
} from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";

const HOOK_CATEGORIES = [
  { value: "emotional", label: "Emotional" },
  { value: "educational", label: "Educational" },
  { value: "controversial", label: "Controversial" },
  { value: "inspirational", label: "Inspirational" },
  { value: "humorous", label: "Humorous" },
  { value: "dramatic", label: "Dramatic" },
  { value: "surprising", label: "Surprising" },
  { value: "other", label: "Other" },
];

const BULK_ADD_FORMAT_EXAMPLE = `Scene 1 Title | 00:01:30 | 00:02:15 | This moment is hilarious | Optional description
Scene 2 Title | 00:05:00 | 00:05:45 | Great advice here | Another description
My Cool Scene | 10:30 | 11:15 | Viral potential`;

interface SceneManagementProps {
  videoId: string;
  onScenesUpdated: () => void;
}

// ============================================================================
// Edit Timestamps Dialog
// ============================================================================

interface EditTimestampsDialogProps {
  videoId: string;
  sceneId: number;
  currentStart: string;
  currentEnd: string;
  onSuccess: () => void;
  trigger: React.ReactNode;
}

export function EditTimestampsDialog({
  videoId,
  sceneId,
  currentStart,
  currentEnd,
  onSuccess,
  trigger,
}: EditTimestampsDialogProps) {
  const { getIdToken } = useAuth();
  const [open, setOpen] = useState(false);
  const [start, setStart] = useState(currentStart);
  const [end, setEnd] = useState(currentEnd);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleOpen = useCallback(
    (isOpen: boolean) => {
      setOpen(isOpen);
      if (isOpen) {
        setStart(currentStart);
        setEnd(currentEnd);
        setError(null);
      }
    },
    [currentStart, currentEnd]
  );

  const handleSave = useCallback(async () => {
    if (!start.trim() || !end.trim()) {
      setError("Both start and end times are required");
      return;
    }

    setSaving(true);
    setError(null);

    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to edit timestamps");
        return;
      }

      await updateSceneTimestamps(videoId, sceneId, start, end, token);
      toast.success("Timestamps updated successfully");
      setOpen(false);
      onSuccess();
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Failed to update timestamps";
      setError(message);
    } finally {
      setSaving(false);
    }
  }, [videoId, sceneId, start, end, getIdToken, onSuccess]);

  return (
    <Dialog open={open} onOpenChange={handleOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Clock className="h-5 w-5" />
            Edit Timestamps
          </DialogTitle>
          <DialogDescription>
            Applies to newly generated clips only (existing clips wont change). Format:
            HH:MM:SS or MM:SS.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="edit-start">Start Time</Label>
              <Input
                id="edit-start"
                value={start}
                onChange={(e) => setStart(e.target.value)}
                placeholder="00:01:30"
                disabled={saving}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-end">End Time</Label>
              <Input
                id="edit-end"
                value={end}
                onChange={(e) => setEnd(e.target.value)}
                placeholder="00:02:15"
                disabled={saving}
              />
            </div>
          </div>

          {error && (
            <div className="flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4" />
              {error}
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <DialogClose asChild>
            <Button variant="outline" disabled={saving}>
              Cancel
            </Button>
          </DialogClose>
          <Button onClick={handleSave} disabled={saving}>
            {saving ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Saving...
              </>
            ) : (
              <>
                <Check className="mr-2 h-4 w-4" />
                Save
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ============================================================================
// Add Scene Dialog
// ============================================================================

interface AddSceneDialogProps {
  videoId: string;
  onSuccess: () => void;
}

export function AddSceneDialog({ videoId, onSuccess }: AddSceneDialogProps) {
  const { getIdToken } = useAuth();
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [title, setTitle] = useState("");
  const [reason, setReason] = useState("");
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");
  const [description, setDescription] = useState("");
  const [hookCategory, setHookCategory] = useState<string>("");

  const resetForm = useCallback(() => {
    setTitle("");
    setReason("");
    setStart("");
    setEnd("");
    setDescription("");
    setHookCategory("");
    setError(null);
  }, []);

  const handleOpen = useCallback(
    (isOpen: boolean) => {
      setOpen(isOpen);
      if (isOpen) {
        resetForm();
      }
    },
    [resetForm]
  );

  const handleSave = useCallback(async () => {
    if (!title.trim()) {
      setError("Title is required");
      return;
    }
    if (!reason.trim()) {
      setError("Reason is required");
      return;
    }
    if (!start.trim() || !end.trim()) {
      setError("Both start and end times are required");
      return;
    }

    setSaving(true);
    setError(null);

    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to add scenes");
        return;
      }

      const request: AddSceneRequest = {
        title: title.trim(),
        reason: reason.trim(),
        start: start.trim(),
        end: end.trim(),
        description: description.trim() || undefined,
        hook_category: hookCategory || undefined,
      };

      await addScene(videoId, request, token);
      toast.success("Scene added successfully");
      setOpen(false);
      onSuccess();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to add scene";
      setError(message);
    } finally {
      setSaving(false);
    }
  }, [
    videoId,
    title,
    reason,
    start,
    end,
    description,
    hookCategory,
    getIdToken,
    onSuccess,
  ]);

  return (
    <Dialog open={open} onOpenChange={handleOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="gap-2">
          <Plus className="h-4 w-4" />
          Add Scene
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Plus className="h-5 w-5" />
            Add New Scene
          </DialogTitle>
          <DialogDescription>
            Manually add a new scene with custom timestamps. This is free.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="add-title">Title *</Label>
            <Input
              id="add-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Enter a catchy title"
              disabled={saving}
              maxLength={200}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="add-reason">Reason *</Label>
            <Textarea
              id="add-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Why is this a good clip?"
              disabled={saving}
              rows={2}
              maxLength={500}
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="add-start">Start Time *</Label>
              <Input
                id="add-start"
                value={start}
                onChange={(e) => setStart(e.target.value)}
                placeholder="00:01:30"
                disabled={saving}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="add-end">End Time *</Label>
              <Input
                id="add-end"
                value={end}
                onChange={(e) => setEnd(e.target.value)}
                placeholder="00:02:15"
                disabled={saving}
              />
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="add-category">Category</Label>
            <Select
              value={hookCategory}
              onValueChange={setHookCategory}
              disabled={saving}
            >
              <SelectTrigger id="add-category">
                <SelectValue placeholder="Select category (optional)" />
              </SelectTrigger>
              <SelectContent>
                {HOOK_CATEGORIES.map((cat) => (
                  <SelectItem key={cat.value} value={cat.value}>
                    {cat.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="add-description">Description</Label>
            <Textarea
              id="add-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional social media caption"
              disabled={saving}
              rows={2}
            />
          </div>

          {error && (
            <div className="flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4" />
              {error}
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <DialogClose asChild>
            <Button variant="outline" disabled={saving}>
              Cancel
            </Button>
          </DialogClose>
          <Button onClick={handleSave} disabled={saving}>
            {saving ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Adding...
              </>
            ) : (
              <>
                <Plus className="mr-2 h-4 w-4" />
                Add Scene
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ============================================================================
// Bulk Add Scenes Dialog
// ============================================================================

interface BulkAddScenesDialogProps {
  videoId: string;
  onSuccess: () => void;
}

export function BulkAddScenesDialog({ videoId, onSuccess }: BulkAddScenesDialogProps) {
  const { getIdToken } = useAuth();
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [bulkText, setBulkText] = useState("");
  const [parseErrors, setParseErrors] = useState<string[]>([]);

  const handleOpen = useCallback((isOpen: boolean) => {
    setOpen(isOpen);
    if (isOpen) {
      setBulkText("");
      setError(null);
      setParseErrors([]);
    }
  }, []);

  const parseBulkText = useCallback(
    (text: string): { scenes: BulkSceneEntry[]; errors: string[] } => {
      const lines = text.split("\n").filter((line) => line.trim());
      const scenes: BulkSceneEntry[] = [];
      const errors: string[] = [];

      for (const [index, rawLine] of lines.entries()) {
        const lineNum = index + 1;
        const line = rawLine.trim();
        if (!line) continue;

        const parts = line.split("|").map((p) => p.trim());
        if (parts.length < 4) {
          errors.push(
            `Line ${lineNum}: Must have at least 4 parts (title | start | end | reason)`
          );
          continue;
        }

        const [title, start, end, reason, description] = parts;

        if (!title) {
          errors.push(`Line ${lineNum}: Title is required`);
          continue;
        }
        if (!start) {
          errors.push(`Line ${lineNum}: Start time is required`);
          continue;
        }
        if (!end) {
          errors.push(`Line ${lineNum}: End time is required`);
          continue;
        }
        if (!reason) {
          errors.push(`Line ${lineNum}: Reason is required`);
          continue;
        }

        scenes.push({
          title,
          start,
          end,
          reason,
          description: description || undefined,
        });
      }

      return { scenes, errors };
    },
    []
  );

  const handleSave = useCallback(async () => {
    const { scenes, errors } = parseBulkText(bulkText);

    if (errors.length > 0) {
      setParseErrors(errors);
      return;
    }

    if (scenes.length === 0) {
      setError("No valid scenes found. Please check the format.");
      return;
    }

    if (scenes.length > 30) {
      setError("Cannot add more than 30 scenes at once");
      return;
    }

    setSaving(true);
    setError(null);
    setParseErrors([]);

    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to add scenes");
        return;
      }

      const response = await bulkAddScenes(videoId, scenes, token);

      if (response.errors.length > 0) {
        const serverErrors = response.errors.map(
          (e) => `Scene ${e.index + 1}: ${e.error}`
        );
        setParseErrors(serverErrors);
      }

      if (response.added_count > 0) {
        toast.success(
          `Added ${response.added_count} scene${response.added_count === 1 ? "" : "s"}`
        );
        if (response.errors.length === 0) {
          setOpen(false);
        }
        onSuccess();
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to add scenes";
      setError(message);
    } finally {
      setSaving(false);
    }
  }, [videoId, bulkText, parseBulkText, getIdToken, onSuccess]);

  return (
    <Dialog open={open} onOpenChange={handleOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="gap-2">
          <Upload className="h-4 w-4" />
          Bulk Add
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Upload className="h-5 w-5" />
            Bulk Add Scenes
          </DialogTitle>
          <DialogDescription>
            Add up to 30 scenes at once. Use the format below (one scene per line).
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="rounded-lg border bg-muted/50 p-3">
            <p className="text-sm font-medium mb-2">Format:</p>
            <code className="text-xs text-muted-foreground">
              Title | Start | End | Reason | Description (optional)
            </code>
            <p className="text-sm font-medium mt-3 mb-2">Example:</p>
            <pre className="text-xs text-muted-foreground whitespace-pre-wrap">
              {BULK_ADD_FORMAT_EXAMPLE}
            </pre>
          </div>

          <div className="space-y-2">
            <Label htmlFor="bulk-text">Scenes (one per line)</Label>
            <Textarea
              id="bulk-text"
              value={bulkText}
              onChange={(e) => {
                setBulkText(e.target.value);
                setParseErrors([]);
                setError(null);
              }}
              placeholder="Enter scenes in the format above..."
              disabled={saving}
              rows={8}
              className="font-mono text-sm"
            />
          </div>

          {parseErrors.length > 0 && (
            <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-3 space-y-1">
              <p className="text-sm font-medium text-destructive flex items-center gap-2">
                <AlertCircle className="h-4 w-4" />
                Validation Errors:
              </p>
              <ul className="text-sm text-destructive space-y-1">
                {parseErrors.slice(0, 5).map((err) => (
                  <li key={err}>• {err}</li>
                ))}
                {parseErrors.length > 5 && (
                  <li>...and {parseErrors.length - 5} more errors</li>
                )}
              </ul>
            </div>
          )}

          {error && (
            <div className="flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4" />
              {error}
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <DialogClose asChild>
            <Button variant="outline" disabled={saving}>
              Cancel
            </Button>
          </DialogClose>
          <Button onClick={handleSave} disabled={saving || !bulkText.trim()}>
            {saving ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Adding...
              </>
            ) : (
              <>
                <Plus className="mr-2 h-4 w-4" />
                Add Scenes
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ============================================================================
// Generate More Scenes Dialog
// ============================================================================

interface GenerateMoreScenesDialogProps {
  videoId: string;
  onSuccess: () => void;
}

export function GenerateMoreScenesDialog({
  videoId,
  onSuccess,
}: GenerateMoreScenesDialogProps) {
  const { getIdToken } = useAuth();
  const [open, setOpen] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleOpen = useCallback((isOpen: boolean) => {
    setOpen(isOpen);
    if (isOpen) {
      setError(null);
    }
  }, []);

  const handleGenerate = useCallback(async () => {
    setGenerating(true);
    setError(null);

    try {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to generate scenes");
        return;
      }

      const idempotencyKey = crypto.randomUUID();
      const response = await generateMoreScenes(videoId, 10, idempotencyKey, token);

      toast.success(
        `Generated ${response.generated_count} new scene${response.generated_count === 1 ? "" : "s"} (${response.credits_charged} credits)`
      );
      setOpen(false);
      onSuccess();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to generate scenes";
      setError(message);
    } finally {
      setGenerating(false);
    }
  }, [videoId, getIdToken, onSuccess]);

  return (
    <Dialog open={open} onOpenChange={handleOpen}>
      <DialogTrigger asChild>
        <Button variant="default" size="sm" className="gap-2">
          <Sparkles className="h-4 w-4" />
          Generate More
          <Badge variant="secondary" className="ml-1">
            3 credits
          </Badge>
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            Generate More Scenes
          </DialogTitle>
          <DialogDescription>
            Use AI to find up to 10 additional viral moments in your video that
            don&apos;t overlap with existing scenes.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="rounded-lg border bg-muted/50 p-4 space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">Scenes to generate:</span>
              <span className="text-sm">Up to 10</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">Cost:</span>
              <Badge variant="secondary">3 credits</Badge>
            </div>
          </div>

          <p className="text-sm text-muted-foreground">
            The AI will analyze your video&apos;s transcript and find new viral moments
            that are different from your existing scenes. This may take a minute.
          </p>

          {error && (
            <div className="flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4" />
              {error}
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <DialogClose asChild>
            <Button variant="outline" disabled={generating}>
              Cancel
            </Button>
          </DialogClose>
          <Button onClick={handleGenerate} disabled={generating}>
            {generating ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Generating...
              </>
            ) : (
              <>
                <Sparkles className="mr-2 h-4 w-4" />
                Generate (3 credits)
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ============================================================================
// Scene Management Toolbar
// ============================================================================

export function SceneManagementToolbar({
  videoId,
  onScenesUpdated,
}: SceneManagementProps) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <AddSceneDialog videoId={videoId} onSuccess={onScenesUpdated} />
      <BulkAddScenesDialog videoId={videoId} onSuccess={onScenesUpdated} />
      <GenerateMoreScenesDialog videoId={videoId} onSuccess={onScenesUpdated} />
    </div>
  );
}
