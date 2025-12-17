"use client";

import { useSearchParams } from "next/navigation";
import { Suspense, useEffect } from "react";

import {
  AnimatedBackground,
  FinalCTASection,
  ForCreatorsSection,
  HeroSection,
  HowItWorksSection,
  PricingSection,
  TestimonialsSection,
  WhyMattersSection,
} from "@/components/landing";
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
      {/* Animated gradient background */}
      <AnimatedBackground />

      {/* Hero Section */}
      <HeroSection />

      {/* Main Processor Interface - The real working processor */}
      <section
        id="process-video"
        className="landing-container relative z-10 -mt-16 mb-24"
      >
        <div className="max-w-4xl mx-auto">
          <ProcessVideoInterface />
        </div>
      </section>

      {/* Why This Matters */}
      <WhyMattersSection />

      {/* How It Works */}
      <HowItWorksSection />

      {/* For Creators */}
      <ForCreatorsSection />

      {/* Testimonials */}
      <TestimonialsSection />

      {/* Pricing */}
      <PricingSection />

      {/* Final CTA */}
      <FinalCTASection />
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
