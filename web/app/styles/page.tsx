"use client";

import { Activity, Gauge, ScanFace, Sparkles, Video } from "lucide-react";
import Link from "next/link";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { usePageView } from "@/lib/usePageView";

export default function StylesPage() {
  usePageView("styles");

  return (
    <div className="space-y-12">
      <section className="space-y-4">
        <h1 className="text-3xl md:text-4xl font-extrabold">How Video Styles Work</h1>
        <p className="text-muted-foreground max-w-3xl text-lg">
          Transform your landscape videos into perfect portrait clips optimized for
          TikTok, Instagram Reels, and YouTube Shorts. Choose the style that best fits
          your content across four tiers: Static, Motion, Smart Face, and Active Speaker
          (Premium).
        </p>
        <div className="flex gap-3">
          <Button asChild>
            <Link href="/">Try It Now</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link href="/docs">View Documentation</Link>
          </Button>
        </div>
      </section>

      <section className="space-y-6">
        <h2 className="text-2xl font-bold">Understanding Video Formats</h2>
        <div className="grid md:grid-cols-2 gap-4">
          <Card>
            <CardHeader>
              <CardTitle>Landscape (16:9)</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                Wide videos like most YouTube videos (1920x1080 pixels). Perfect for
                desktop viewing and traditional video platforms.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Portrait (9:16)</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                Tall videos optimized for mobile viewing. The standard format for
                TikTok, Instagram Reels, and YouTube Shorts (1080x1920 pixels).
              </p>
            </CardContent>
          </Card>
        </div>
      </section>

      <section className="space-y-6">
        <h2 className="text-2xl font-bold">Available Styles</h2>

        <div className="space-y-6">
          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Gauge className="h-6 w-6 text-primary" />
                <CardTitle>Static Styles (Tier 0)</CardTitle>
              </div>
              <CardDescription>
                Fastest, deterministic. No AI detection.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Choose a fixed framing without any AI involvement. Ideal when you
                already know exactly which part of the frame you want to highlight.
              </p>
              <div className="grid sm:grid-cols-2 gap-2 text-sm text-muted-foreground">
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Layouts</div>
                  <ul className="list-disc list-inside">
                    <li>split / split_fast</li>
                    <li>left_focus / center_focus / right_focus</li>
                    <li>original</li>
                  </ul>
                </div>
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Best for</div>
                  <ul className="list-disc list-inside">
                    <li>Guaranteed framing & speed</li>
                    <li>Simple tutorials with fixed subjects</li>
                    <li>When you need zero AI variability</li>
                  </ul>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Activity className="h-6 w-6 text-primary" />
                <CardTitle>Motion Mode (Tier 1)</CardTitle>
              </div>
              <CardDescription>
                Follows movement & gestures using fast heuristics (no neural nets).
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Great for high-energy footage where speed matters more than face
                detection.
              </p>
              <div className="grid sm:grid-cols-2 gap-2 text-sm text-muted-foreground">
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Styles</div>
                  <ul className="list-disc list-inside">
                    <li>intelligent_motion (full)</li>
                    <li>intelligent_split_motion (split)</li>
                  </ul>
                </div>
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Best for</div>
                  <ul className="list-disc list-inside">
                    <li>Gaming & esports POVs</li>
                    <li>Sports & fitness clips</li>
                    <li>High-motion stage content</li>
                  </ul>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <ScanFace className="h-6 w-6 text-primary" />
                <CardTitle>Smart Face Mode (Tier 2)</CardTitle>
              </div>
              <CardDescription>
                Balanced AI face detection to keep the main face centered.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Ideal for talking-head content that needs reliable framing without the
                premium mesh tracker.
              </p>
              <div className="grid sm:grid-cols-2 gap-2 text-sm text-muted-foreground">
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Styles</div>
                  <ul className="list-disc list-inside">
                    <li>intelligent (full)</li>
                    <li>intelligent_split (split)</li>
                  </ul>
                </div>
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Best for</div>
                  <ul className="list-disc list-inside">
                    <li>Talking-head explainers</li>
                    <li>Screen-share + webcam tutorials</li>
                    <li>Standard interviews</li>
                  </ul>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Sparkles className="h-6 w-6 text-primary" />
                <CardTitle>Active Speaker Mode (Tier 3, Premium)</CardTitle>
              </div>
              <CardDescription>
                Premium YuNet + Face Mesh tracking that focuses on the person speaking.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Best choice for multi-speaker conversations where you need accurate
                speaker focus and minimal camera drift.
              </p>
              <div className="grid sm:grid-cols-2 gap-2 text-sm text-muted-foreground">
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Styles</div>
                  <ul className="list-disc list-inside">
                    <li>intelligent_speaker (full)</li>
                    <li>intelligent_split_speaker (split)</li>
                  </ul>
                </div>
                <div className="space-y-1">
                  <div className="font-semibold text-foreground">Best for</div>
                  <ul className="list-disc list-inside">
                    <li>Podcasts & debates</li>
                    <li>Panel interviews</li>
                    <li>Any multi-speaker conversation</li>
                  </ul>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Video className="h-6 w-6 text-primary" />
                <CardTitle>Original</CardTitle>
              </div>
              <CardDescription>Keep your video exactly as it is</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                No cropping, no format changes, no modifications. Preserves your video
                exactly as it was recorded.
              </p>
              <div>
                <span className="font-semibold">Best for:</span>
                <ul className="list-disc list-inside text-muted-foreground ml-2 mt-1">
                  <li>When you want to preserve the original format</li>
                  <li>Videos that are already in the perfect format</li>
                  <li>When you just want to extract clips without changes</li>
                </ul>
              </div>
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold">Output:</span> Same as input (no
                changes)
              </p>
            </CardContent>
          </Card>
        </div>
      </section>

      <section className="space-y-4">
        <h2 className="text-2xl font-bold">Quick Decision Guide</h2>
        <div className="grid md:grid-cols-2 gap-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Choose Motion Mode if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                You need speed for fast movement (gaming, sports, dance) and don&apos;t
                require face awareness.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Choose Smart Face if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                You&apos;re making talking-head content, tutorials, or interviews and
                want the main face centered reliably.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Choose Active Speaker if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                You have multiple people and want the crop to follow whoever is speaking
                in real time (premium).
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">
                Choose Static Split/Left/Center/Right if...
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                You prefer deterministic framing, fastest processing, or already know
                the exact subject placement.
              </p>
            </CardContent>
          </Card>
        </div>
      </section>

      <section className="space-y-4">
        <h2 className="text-2xl font-bold">Ready to Get Started?</h2>
        <p className="text-muted-foreground">
          Try processing a video with different styles to see which works best for your
          content. You can select multiple styles or choose &quot;All Styles&quot; to
          generate every variation at once.
        </p>
        <Button size="lg" asChild>
          <Link href="/">Start Processing Videos</Link>
        </Button>
      </section>
    </div>
  );
}
