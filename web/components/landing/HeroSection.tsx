"use client";

import { ArrowRight, Play } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";

const ROTATING_TEXTS = [
  "Without Editing.",
  "Automatically.",
  "Every Single Day.",
  "While You Sleep.",
];

export function HeroSection() {
  const [textIndex, setTextIndex] = useState(0);
  const [isAnimating, setIsAnimating] = useState(false);

  const rotatingText = ROTATING_TEXTS.at(textIndex) ?? "";

  useEffect(() => {
    const interval = setInterval(() => {
      setIsAnimating(true);
      setTimeout(() => {
        setTextIndex((prev) => (prev + 1) % ROTATING_TEXTS.length);
        setIsAnimating(false);
      }, 300);
    }, 3000);

    return () => clearInterval(interval);
  }, []);

  const scrollToProcessor = (e: React.MouseEvent) => {
    e.preventDefault();
    const target = document.querySelector("#process-video");
    if (target) {
      const navHeight = 80;
      const targetPosition =
        target.getBoundingClientRect().top + window.scrollY - navHeight - 20;
      window.scrollTo({ top: targetPosition, behavior: "smooth" });
    }
  };

  const handleWatchDemo = (e: React.MouseEvent) => {
    e.preventDefault();
    // Demo is not functional yet - placeholder
  };

  return (
    <section className="min-h-screen flex items-center pt-24 pb-16 relative overflow-hidden bg-transparent">
      {/* Background glow */}
      <div className="absolute inset-0 pointer-events-none">
        <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[150%] h-[80%] bg-[radial-gradient(ellipse_80%_50%_at_50%_-20%,rgba(124,58,237,0.15),transparent)] dark:bg-[radial-gradient(ellipse_80%_50%_at_50%_-20%,rgba(164,92,255,0.15),transparent)]" />
        <div className="absolute top-1/2 right-0 w-[60%] h-[60%] bg-[radial-gradient(ellipse_60%_40%_at_80%_60%,rgba(6,182,212,0.15),transparent)] dark:bg-[radial-gradient(ellipse_60%_40%_at_80%_60%,rgba(92,255,249,0.08),transparent)]" />
      </div>

      <div className="landing-container">
        <div className="grid lg:grid-cols-2 gap-12 lg:gap-16 items-center">
          {/* Hero Content */}
          <div className="space-y-8 text-center lg:text-left">
            <h1 className="text-4xl sm:text-5xl lg:text-[3.5rem] font-bold leading-[1.1] tracking-tight text-foreground">
              Become a Daily Creator —{" "}
              <span
                className={`gradient-text inline-block transition-all duration-300 ${
                  isAnimating ? "opacity-0 translate-y-2" : "opacity-100 translate-y-0"
                }`}
              >
                {rotatingText}
              </span>
            </h1>

            <p className="text-lg lg:text-xl text-muted-foreground leading-relaxed max-w-xl mx-auto lg:mx-0">
              Paste a YouTube link. Viral Clip AI turns your long videos into automatic,
              daily, viral-ready clips so you grow consistently without burning out.
            </p>

            {/* CTAs */}
            <div className="flex flex-col sm:flex-row gap-4 justify-center lg:justify-start">
              <Button
                onClick={scrollToProcessor}
                className="btn-primary-gradient h-14 px-8 text-lg font-semibold rounded-xl gap-2 group text-brand-dark"
              >
                <span>Generate My First Clip</span>
                <ArrowRight className="w-5 h-5 transition-transform group-hover:translate-x-1" />
              </Button>
              <Button
                variant="outline"
                onClick={handleWatchDemo}
                className="h-14 px-8 text-lg font-medium rounded-xl gap-2 glass-card border-border hover:border-brand-400/30 hover:bg-brand-400/10"
                title="Coming soon"
              >
                <Play className="w-5 h-5" />
                <span>Watch Demo</span>
              </Button>
            </div>

            {/* Micro proof */}
            <p className="text-sm text-muted-foreground/60">
              No credit card required · Free plan available
            </p>

            {/* Social proof line */}
            <p className="text-sm text-muted-foreground pt-6 border-t border-border">
              Trusted by creators, coaches & podcasters who want to stay consistent and
              grow effortlessly.
            </p>
          </div>

          {/* Hero Visual - App Mockup */}
          <div className="relative animate-float">
            <AppMockup />
          </div>
        </div>
      </div>
    </section>
  );
}

function AppMockup() {
  return (
    <div className="relative max-w-lg mx-auto lg:mx-0">
      {/* Glow effect */}
      <div className="absolute -inset-4 bg-gradient-to-r from-brand-400 to-brand-cyan rounded-3xl opacity-10 dark:opacity-20 blur-3xl animate-glow-pulse" />

      <div className="glass-card rounded-3xl p-6 relative bg-white/50 dark:bg-black/40">
        {/* Viral badge */}
        <div className="absolute -top-3 right-6 flex items-center gap-1.5 bg-brand-cyan/10 border border-brand-cyan/30 px-3 py-1 rounded-full backdrop-blur-sm shadow-sm">
          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
            <path
              d="M8 3L9.5 6L13 6.5L10.5 9L11 12.5L8 11L5 12.5L5.5 9L3 6.5L6.5 6L8 3Z"
              fill="#5CFFF9"
              className="dark:fill-[#5CFFF9] fill-[#06b6d4]"
            />
          </svg>
          <span className="text-xs font-medium text-brand-cyan dark:text-brand-cyan text-cyan-600">
            AI detected 14 viral moments
          </span>
        </div>

        {/* Input field mockup */}
        <div className="flex gap-3 mb-4">
          <div className="flex-1 flex items-center gap-2 bg-muted/50 dark:bg-white/5 border border-border dark:border-white/10 rounded-lg px-4 py-3">
            <svg
              className="w-4 h-4 text-muted-foreground/50"
              viewBox="0 0 24 24"
              fill="none"
            >
              <path
                d="M13.5 6H5.25A2.25 2.25 0 003 8.25v10.5A2.25 2.25 0 005.25 21h10.5A2.25 2.25 0 0018 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            <span className="text-sm text-muted-foreground/50">
              Paste your YouTube link…
            </span>
          </div>
          <button className="btn-primary-gradient px-4 py-2 rounded-lg text-sm font-semibold whitespace-nowrap text-white dark:text-brand-dark">
            Analyze Video
          </button>
        </div>

        {/* Progress bar */}
        <div className="mb-4">
          <div className="relative h-1 bg-muted dark:bg-white/10 rounded-full mb-2">
            <div className="absolute left-0 top-0 h-full w-[48%] bg-gradient-to-r from-brand-400 to-brand-cyan rounded-full animate-progress-scan" />
            <div className="absolute inset-0">
              {[15, 32, 48, 67, 85].map((pos, i) => (
                <div
                  key={pos}
                  className={`absolute top-1/2 -translate-y-1/2 w-2 h-2 rounded-full border-2 ${
                    i < 3
                      ? "bg-brand-cyan border-brand-cyan shadow-[0_0_10px_var(--brand-cyan)]"
                      : "bg-background border-muted-foreground dark:bg-brand-darker"
                  }`}
                  style={{ left: `${pos}%` }}
                />
              ))}
            </div>
          </div>
          <span className="text-xs text-muted-foreground/60">
            Scanning for viral moments...
          </span>
        </div>

        {/* Clips grid */}
        <div className="grid grid-cols-3 gap-3">
          <ClipCard
            label="Best hook"
            labelClass="bg-brand-400/10 text-brand-400 dark:bg-brand-400/20"
            duration="0:32"
          />
          <ClipCard
            label="High-energy moment"
            labelClass="bg-amber-500/10 text-amber-600 dark:text-amber-400 dark:bg-amber-500/20"
            duration="0:58"
          />
          <ClipCard
            label="Viral potential"
            labelClass="bg-brand-cyan/10 text-cyan-600 dark:text-brand-cyan dark:bg-brand-cyan/20"
            duration="0:45"
          />
        </div>
      </div>
    </div>
  );
}

function ClipCard({
  label,
  labelClass,
  duration,
}: {
  label: string;
  labelClass: string;
  duration: string;
}) {
  return (
    <div className="bg-white/40 dark:bg-white/5 border border-white/20 dark:border-white/10 rounded-xl overflow-hidden hover:-translate-y-1 transition-transform shadow-sm">
      <div className="aspect-[9/12] bg-gradient-to-br from-brand-400/5 to-brand-cyan/5 flex items-center justify-center relative">
        <div className="w-10 h-10 bg-white/20 dark:bg-white/10 rounded-full flex items-center justify-center backdrop-blur-sm">
          <svg
            className="w-5 h-5 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="currentColor"
          >
            <path d="M5.25 5.653c0-.856.917-1.398 1.667-.986l11.54 6.347a1.125 1.125 0 010 1.972l-11.54 6.347a1.125 1.125 0 01-1.667-.986V5.653z" />
          </svg>
        </div>
        <span className="absolute bottom-2 right-2 bg-black/60 backdrop-blur-md text-white px-1.5 py-0.5 rounded text-xs font-medium">
          {duration}
        </span>
      </div>
      <div className={`px-2 py-1.5 text-center text-[10px] font-medium ${labelClass}`}>
        {label}
      </div>
    </div>
  );
}
