/**
 * Video Processing Form Component
 *
 * Form for submitting video processing requests.
 */

import { type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

const STYLES = [
  { value: "split", label: "Split View", subtitle: "Top/Bottom" },
  { value: "left_focus", label: "Left Focus", subtitle: "Full Height" },
  { value: "right_focus", label: "Right Focus", subtitle: "Full Height" },
  { value: "all", label: "All Styles", subtitle: "Generate All" },
];

interface VideoFormProps {
  url: string;
  setUrl: (url: string) => void;
  style: string;
  setStyle: (style: string) => void;
  customPrompt: string;
  setCustomPrompt: (prompt: string) => void;
  onSubmit: (e: FormEvent) => void;
  submitting: boolean;
}

export function VideoForm({
  url,
  setUrl,
  style,
  setStyle,
  customPrompt,
  setCustomPrompt,
  onSubmit,
  submitting,
}: VideoFormProps) {
  return (
    <Card className="glass shadow-2xl">
      <CardHeader>
        <CardTitle>Process Video</CardTitle>
        <CardDescription>
          Enter a YouTube URL to generate viral clips using AI
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={onSubmit} className="space-y-6">
          <div className="space-y-2">
            <Label htmlFor="youtube-url" className="uppercase tracking-wider">
              YouTube Source URL
            </Label>
            <Input
              id="youtube-url"
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://www.youtube.com/watch?v=..."
              className="text-lg"
              required
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="custom-prompt" className="uppercase tracking-wider">
              Custom prompt (optional)
            </Label>
            <Textarea
              id="custom-prompt"
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              rows={3}
              placeholder="e.g. Find the most emotional moments where the speaker shares a personal story."
            />
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setCustomPrompt(
                    "Find the most emotional and vulnerable moments in this video that would resonate strongly on TikTok."
                  )
                }
              >
                Emotional moments
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setCustomPrompt(
                    "Find the best high-retention viral clip candidates for short-form social media (TikTok, Shorts, Reels)."
                  )
                }
              >
                Best viral clips
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setCustomPrompt(
                    "Find segments with intense discussion about the main subject, where there is strong opinion or debate."
                  )
                }
              >
                Subject discussion
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setCustomPrompt(
                    "Find moments with interesting sounds or reactions that would work well in sound-on social media clips."
                  )
                }
              >
                Sound-focused clips
              </Button>
            </div>
          </div>

          <div className="space-y-2">
            <Label className="uppercase tracking-wider">Output Style</Label>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              {STYLES.map((s) => (
                <label
                  key={s.value}
                  htmlFor={`style-${s.value}`}
                  className="cursor-pointer"
                >
                  <input
                    id={`style-${s.value}`}
                    type="radio"
                    name="style"
                    value={s.value}
                    checked={style === s.value}
                    onChange={() => setStyle(s.value)}
                    className="peer sr-only"
                    aria-label={`${s.label} - ${s.subtitle}`}
                  />
                  <div
                    className={cn(
                      "p-4 rounded-xl border transition-all text-center",
                      "bg-card hover:bg-accent",
                      "peer-checked:border-primary peer-checked:bg-primary/10"
                    )}
                  >
                    <span className="font-medium block">{s.label}</span>
                    <span className="block text-xs text-muted-foreground mt-1">
                      {s.subtitle}
                    </span>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <Button
            type="submit"
            disabled={submitting}
            variant="brand"
            size="lg"
            className="w-full gap-2"
          >
            <span>🚀</span>
            <span>Launch Processor</span>
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
