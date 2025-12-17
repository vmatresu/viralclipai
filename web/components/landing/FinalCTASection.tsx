"use client";

import { ArrowRight } from "lucide-react";

import { Button } from "@/components/ui/button";

import { LandingSection } from "./LandingSection";

export function FinalCTASection() {
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

  return (
    <LandingSection className="relative overflow-hidden">
      {/* Background */}
      <div className="absolute inset-0 glass-card" />
      <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-brand-400/50 to-transparent" />
      <div className="absolute bottom-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-brand-400/50 to-transparent" />

      <div className="relative glass-card rounded-3xl p-12 max-w-3xl mx-auto text-center">
        <h2 className="text-3xl md:text-4xl font-bold mb-6">
          Your ideas deserve <span className="gradient-text">momentum.</span>
        </h2>
        <p className="text-xl text-muted-foreground mb-8">
          Let Viral Clip AI turn your long videos into an always-on growth engine.
        </p>
        <Button
          onClick={scrollToProcessor}
          className="btn-primary-gradient h-14 px-8 text-lg font-semibold rounded-xl gap-2 group"
        >
          <span>Generate My First Clip</span>
          <ArrowRight className="w-5 h-5 transition-transform group-hover:translate-x-1" />
        </Button>
      </div>
    </LandingSection>
  );
}
