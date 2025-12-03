"use client";

import { Video, Sparkles, Target, Zap, Eye, Film } from "lucide-react";
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
        <h1 className="text-3xl md:text-4xl font-extrabold">
          How Video Styles Work
        </h1>
        <p className="text-muted-foreground max-w-3xl text-lg">
          Transform your landscape videos into perfect portrait clips optimized for TikTok,
          Instagram Reels, and YouTube Shorts. Choose the style that best fits your content.
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
                Wide videos like most YouTube videos (1920x1080 pixels). Perfect for desktop
                viewing and traditional video platforms.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Portrait (9:16)</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                Tall videos optimized for mobile viewing. The standard format for TikTok,
                Instagram Reels, and YouTube Shorts (1080x1920 pixels).
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
                <Film className="h-6 w-6 text-primary" />
                <CardTitle>Split View</CardTitle>
              </div>
              <CardDescription>Perfect for two-person conversations</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Takes your wide video and splits it into two halves, then stacks them on top
                of each other. Great for interviews, conversations, or videos where both
                sides of the screen are important.
              </p>
              <div>
                <span className="font-semibold">Best for:</span>
                <ul className="list-disc list-inside text-muted-foreground ml-2 mt-1">
                  <li>Videos with two people talking</li>
                  <li>Interviews or panel discussions</li>
                  <li>When both sides of the screen matter</li>
                </ul>
              </div>
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold">Output:</span> Portrait format (1080x1920)
              </p>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Eye className="h-6 w-6 text-primary" />
                <CardTitle>Left Focus</CardTitle>
              </div>
              <CardDescription>Focus on the left side of your video</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Crops and focuses on the left side of your video, making it full-height
                portrait. Perfect when your main subject is positioned on the left.
              </p>
              <div>
                <span className="font-semibold">Best for:</span>
                <ul className="list-disc list-inside text-muted-foreground ml-2 mt-1">
                  <li>Single-person videos with left positioning</li>
                  <li>Presentations where the presenter is on the left</li>
                  <li>When the left side has the most important content</li>
                </ul>
              </div>
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold">Output:</span> Portrait format (1080x1920)
              </p>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Eye className="h-6 w-6 text-primary" />
                <CardTitle>Right Focus</CardTitle>
              </div>
              <CardDescription>Focus on the right side of your video</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Crops and focuses on the right side of your video, making it full-height
                portrait. Perfect when your main subject is positioned on the right.
              </p>
              <div>
                <span className="font-semibold">Best for:</span>
                <ul className="list-disc list-inside text-muted-foreground ml-2 mt-1">
                  <li>Single-person videos with right positioning</li>
                  <li>Explanations where content is on the right</li>
                  <li>When the right side has the most important content</li>
                </ul>
              </div>
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold">Output:</span> Portrait format (1080x1920)
              </p>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Sparkles className="h-6 w-6 text-primary" />
                <CardTitle>Intelligent Crop</CardTitle>
              </div>
              <CardDescription>AI-powered face and subject tracking</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Uses artificial intelligence to automatically track faces and important
                subjects in your video, keeping them centered and in focus as the video
                plays. The smartest option for videos with people.
              </p>
              <div>
                <span className="font-semibold">Best for:</span>
                <ul className="list-disc list-inside text-muted-foreground ml-2 mt-1">
                  <li>Videos with people (faces are automatically detected)</li>
                  <li>Videos where subjects move around</li>
                  <li>When you want the most professional-looking crop</li>
                  <li>Dynamic videos with movement</li>
                </ul>
              </div>
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold">Output:</span> Portrait format (default 9:16,
                customizable)
              </p>
            </CardContent>
          </Card>

          <Card className="glass">
            <CardHeader>
              <div className="flex items-center gap-3">
                <Target className="h-6 w-6 text-primary" />
                <CardTitle>Intelligent Split View</CardTitle>
              </div>
              <CardDescription>AI-powered 9:16 optimized for TikTok & Reels</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-muted-foreground">
                Uses the same AI-powered tracking as Intelligent Crop, but specifically
                optimized for 9:16 portrait format. Perfect for TikTok, Instagram Reels,
                and YouTube Shorts.
              </p>
              <div>
                <span className="font-semibold">Best for:</span>
                <ul className="list-disc list-inside text-muted-foreground ml-2 mt-1">
                  <li>Videos with people for TikTok/Reels/Shorts</li>
                  <li>When you need guaranteed 9:16 format</li>
                  <li>Maximum compatibility with short-form platforms</li>
                </ul>
              </div>
              <p className="text-sm text-muted-foreground">
                <span className="font-semibold">Output:</span> Portrait format (9:16 -
                guaranteed)
              </p>
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
                <span className="font-semibold">Output:</span> Same as input (no changes)
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
              <CardTitle className="text-lg">Choose Split View if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                Your video has two people or two focal points, or both sides of the screen
                are equally important.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Choose Left/Right Focus if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                The main subject is on one side and you want a simple, focused crop.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Choose Intelligent Crop if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                Your video has people or moving subjects and you want the best automatic
                tracking.
              </p>
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">Choose Intelligent Split View if...</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">
                You&apos;re posting on TikTok, Instagram Reels, or YouTube Shorts and want
                guaranteed 9:16 format with AI tracking.
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

