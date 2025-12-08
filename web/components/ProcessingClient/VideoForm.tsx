/**
 * Video Processing Form Component
 *
 * Form for submitting video processing requests.
 */

import { type FormEvent } from "react";

import { StyleQualitySelector } from "@/components/style-quality/StyleQualitySelector";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

interface VideoFormProps {
  url: string;
  setUrl: (url: string) => void;
  styles: string[];
  setStyles: (styles: string[]) => void;
  customPrompt: string;
  setCustomPrompt: (prompt: string) => void;
  onSubmit: (e: FormEvent) => void;
  submitting: boolean;
}

export function VideoForm({
  url,
  setUrl,
  styles,
  setStyles,
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
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setCustomPrompt(
                    '**Role:**\n\nYou are an elite short-form video editor. The video format is a split-screen: a viral clip (usually a woman) on the Left, and a male commentator on the Right.\n\n**Your Goal:**\n\nExtract a batch of **3 to 10 viral segments** that prioritize Interaction over simple monologues.\n\n**Segment Structure (The "Call & Response" Formula):**\n\n1. **The Setup (Left Side):** Start exactly when the person makes a controversial claim, states a statistic, or complains about men.\n\n2. **The Pivot:** The moment the host pauses the video or speaks up.\n\n3. **The Slam (Right Side):** The host\'s immediate counter-argument, insult, or reality check.\n\n4. **The End:** Cut after the punchline.\n\n**Constraints:**\n\n* **Quantity:** Extract at least 3 distinct segments.\n\n* **Duration:** Each individual segment must be **20 to 60 seconds** long.\n\n* **Narrative:** [Setup] -> [Reaction] -> [Punchline].'
                  )
                }
              >
                Manosphere
              </Button>
            </div>
          </div>

          <StyleQualitySelector selectedStyles={styles} onChange={setStyles} />

          <Button
            type="submit"
            disabled={submitting || styles.length === 0}
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
