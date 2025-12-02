"use client";

import { Sparkles, Video, Users } from "lucide-react";
import Link from "next/link";
import { Suspense } from "react";

import { ProcessingClient } from "@/components/ProcessingClient";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { analyticsEvents } from "@/lib/analytics";
import { usePageView } from "@/lib/usePageView";

export default function HomePage() {
  usePageView("home");
  return (
    <div className="space-y-12">
      <section className="space-y-4">
        <h1 className="text-3xl md:text-4xl font-extrabold">
          Turn long-form videos into viral clips in minutes.
        </h1>
        <p className="text-muted-foreground max-w-2xl">
          Viral Clip AI analyzes your YouTube commentary videos, finds the most engaging
          moments, and generates social-ready clips optimized for TikTok, Shorts, and
          Reels.
        </p>
        <div className="flex flex-wrap gap-3">
          <Button
            asChild
            variant="brand"
            size="lg"
            onClick={() => {
              void analyticsEvents.ctaClicked({
                ctaName: "try_it_now",
                location: "home",
              });
            }}
          >
            <a href="#app">Try it now</a>
          </Button>
          <Button
            asChild
            variant="outline"
            size="lg"
            onClick={() => {
              void analyticsEvents.ctaClicked({
                ctaName: "view_pricing",
                location: "home",
              });
            }}
          >
            <Link href="/pricing">View pricing</Link>
          </Button>
        </div>
      </section>

      <section className="grid md:grid-cols-3 gap-6">
        <Card className="glass">
          <CardHeader>
            <Sparkles className="h-6 w-6 text-primary mb-2" />
            <CardTitle>AI highlight detection</CardTitle>
          </CardHeader>
          <CardContent>
            <CardDescription>
              Powered by Gemini to find high-retention segments and automatically
              propose clip boundaries.
            </CardDescription>
          </CardContent>
        </Card>
        <Card className="glass">
          <CardHeader>
            <Video className="h-6 w-6 text-primary mb-2" />
            <CardTitle>Vertical-ready formats</CardTitle>
          </CardHeader>
          <CardContent>
            <CardDescription>
              Split view, left/right focus, or all styles—designed for TikTok, Shorts,
              and Reels.
            </CardDescription>
          </CardContent>
        </Card>
        <Card className="glass">
          <CardHeader>
            <Users className="h-6 w-6 text-primary mb-2" />
            <CardTitle>Per-user history & limits</CardTitle>
          </CardHeader>
          <CardContent>
            <CardDescription>
              Firebase Auth, Firestore, and S3-backed storage so every creator has their
              own secure workspace.
            </CardDescription>
          </CardContent>
        </Card>
      </section>

      <section id="app">
        <Suspense fallback={<div className="text-muted-foreground">Loading...</div>}>
          <ProcessingClient />
        </Suspense>
      </section>
    </div>
  );
}
