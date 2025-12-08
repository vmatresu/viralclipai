"use client";

import { useSearchParams } from "next/navigation";
import { Suspense, useEffect } from "react";

// New Components
import { FeatureHighlights } from "@/components/home/FeaturesSection";
import { HeroSection } from "@/components/home/HeroSection";
import { HowItWorks } from "@/components/home/HowItWorks";
import { SocialProof } from "@/components/home/SocialProof";
import { ProcessVideoInterface } from "@/components/process/ProcessVideoInterface";
import { ProcessingClient } from "@/components/ProcessingClient";
import { usePageView } from "@/lib/usePageView";

function HomePageContent() {
  usePageView("home");
  const searchParams = useSearchParams();
  const videoId = searchParams.get("id");
  const isViewingVideo = Boolean(videoId);

  // Redirect old URL format to new format
  useEffect(() => {
    if (videoId && window.location.pathname === "/") {
      // Redirect to /history?id=videoId
      window.history.replaceState({}, "", `/history?id=${videoId}`);
    }
  }, [videoId]);

  if (isViewingVideo) {
    return (
      <section id="app" className="page-container">
        <Suspense fallback={<div className="text-muted-foreground">Loading...</div>}>
          <ProcessingClient />
        </Suspense>
      </section>
    );
  }

  return (
    <div className="flex flex-col min-h-screen">
      {/* Hero Section */}
      <HeroSection />

      {/* Main Processor Interface - Anchored for conversion */}
      <section
        id="process-video"
        className="container px-4 relative z-10 -mt-20 lg:-mt-32 mb-20"
      >
        <ProcessVideoInterface />
      </section>

      {/* Social Proof */}
      <SocialProof />

      {/* How it Works */}
      <HowItWorks />

      {/* Features */}
      <FeatureHighlights />

      {/* Final CTA Area */}
      <section className="py-24 text-center">
        <div className="container px-4">
          <h2 className="text-3xl md:text-4xl font-bold mb-6">Ready to go viral?</h2>
          <p className="text-muted-foreground mb-8 max-w-2xl mx-auto">
            Join thousands of creators using AI to dominate TikTok and Reels.
          </p>
          <a
            href="#process-video"
            className="inline-flex items-center justify-center h-12 px-8 rounded-full bg-primary text-primary-foreground font-medium hover:bg-primary/90 transition-colors shadow-[0_0_20px_-5px_theme(colors.primary.DEFAULT)] hover:shadow-[0_0_30px_-5px_theme(colors.primary.DEFAULT)]"
          >
            Create your first clip
          </a>
        </div>
      </section>
    </div>
  );
}

export default function HomePage() {
  return (
    <Suspense fallback={<div className="text-muted-foreground">Loading...</div>}>
      <HomePageContent />
    </Suspense>
  );
}
