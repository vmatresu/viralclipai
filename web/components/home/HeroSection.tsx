import { ArrowRight, CheckCircle2, Play } from "lucide-react";
import Link from "next/link";

import { Button } from "@/components/ui/button";
import { analyticsEvents } from "@/lib/analytics";

export function HeroSection() {
  return (
    <section className="relative pt-8 pb-20 lg:pt-24 lg:pb-32 overflow-hidden">
      {/* Background Gradients */}
      <div className="absolute top-0 center w-full h-screen pointer-events-none overflow-hidden -z-10">
        <div className="absolute top-[-10%] left-1/2 -translate-x-1/2 w-[1200px] h-[600px] bg-primary/20 blur-[100px] rounded-full opacity-50" />
        <div className="absolute bottom-0 left-0 w-[800px] h-[800px] bg-indigo-500/10 blur-[100px] rounded-full opacity-30" />
      </div>

      <div className="container px-4 md:px-6 grid lg:grid-cols-2 gap-12 lg:gap-8 items-center">
        {/* Left Content */}
        <div className="space-y-8 max-w-2xl">
          <div className="space-y-4">
            <div className="inline-flex items-center rounded-full border border-primary/20 bg-primary/10 px-3 py-1 text-sm font-medium text-primary backdrop-blur-sm mb-4">
              <span className="flex h-2 w-2 rounded-full bg-primary mr-2 animate-pulse" />
              New: Intelligent Face Tracking
            </div>
            <h1 className="h1 text-5xl md:text-6xl lg:text-7xl leading-tight">
              Turn YouTube videos into{" "}
              <span className="text-transparent bg-clip-text bg-gradient-to-r from-primary to-indigo-400">
                Viral Shorts
              </span>
            </h1>
            <p className="text-xl text-muted-foreground leading-relaxed max-w-lg">
              Auto-detect engaging moments, crop to vertical with AI face tracking, and
              export specifically for TikTok, Reels, and YouTube Shorts.
            </p>
          </div>

          <div className="flex flex-col sm:flex-row gap-4">
            <Button
              asChild
              variant="brand"
              size="lg"
              className="h-14 px-8 text-lg shadow-[0_0_30px_-10px_theme(colors.primary.DEFAULT)] hover:shadow-[0_0_40px_-10px_theme(colors.primary.DEFAULT)] transition-all"
              onClick={() => {
                void analyticsEvents.ctaClicked({
                  ctaName: "try_it_now",
                  location: "home_hero",
                });
              }}
            >
              <a href="#process-video" className="flex items-center gap-2">
                Try it now <ArrowRight className="w-5 h-5" />
              </a>
            </Button>
            <Button
              asChild
              variant="outline"
              size="lg"
              className="h-14 px-8 text-lg border-white/10 bg-white/5 hover:bg-white/10 backdrop-blur-sm"
            >
              <Link href="/pricing">View pricing</Link>
            </Button>
          </div>

          <div className="flex items-center gap-6 text-sm text-muted-foreground">
            <div className="flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 text-green-500" /> No credit card
              required
            </div>
            <div className="flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 text-green-500" /> Free tier available
            </div>
          </div>
        </div>

        {/* Right Visual - Mockup */}
        <div className="relative mx-auto lg:ml-auto w-full max-w-[600px] lg:max-w-none">
          {/* Main Image Container */}
          <div className="relative rounded-2xl border border-white/10 bg-white/5 backdrop-blur-sm p-2 shadow-2xl skew-y-1 lg:skew-y-2 transform transition-transform hover:skew-y-0 duration-500 ease-out hover:scale-[1.02] z-20">
            <div className="relative aspect-[16/10] overflow-hidden rounded-xl bg-slate-900/50">
              <img
                src="/images/ai-clipper-saas-hero-image.png"
                alt="Viral Clip AI Interface"
                className="w-full h-full object-cover opacity-90"
              />

              {/* Floating Elements for effect */}
              <div className="absolute top-1/2 left-10 transform -translate-y-1/2 bg-black/80 text-white text-xs px-2 py-1 rounded border border-white/10 flex items-center gap-2 shadow-xl">
                <Play className="w-3 h-3 fill-red-500 text-red-500" />
                Original Source
              </div>
            </div>
          </div>

          {/* Secondary Layer - Premium Mockup peaking from behind */}
          <div className="absolute top-10 -right-10 w-full h-full rounded-2xl border border-white/5 bg-slate-900/80 backdrop-blur-sm -z-10 skew-y-1 lg:skew-y-2 scale-95 opacity-60 overflow-hidden hidden md:block">
            <img
              src="/images/premium-saas-hero-image.png"
              alt="Premium Dashboard View"
              className="w-full h-full object-cover opacity-50 blur-[1px]"
            />
          </div>

          {/* Background blobs for the image */}
          <div className="absolute -inset-4 bg-gradient-to-r from-primary to-purple-600 rounded-3xl blur-2xl opacity-20 -z-10" />
        </div>
      </div>
    </section>
  );
}
