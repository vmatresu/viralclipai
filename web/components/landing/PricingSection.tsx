"use client";

import { Check } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { LandingSection, SectionHeader } from "./LandingSection";

const PLANS = [
  {
    name: "Free",
    price: "$0",
    period: "/month",
    description: "Perfect to test the engine.",
    features: [
      "200 credits/month (~20 clips)",
      "Static & Basic AI detection",
      "Streamer styles",
      "1 GB storage",
      "Watermarked exports",
    ],
    cta: "Get Started",
    featured: false,
  },
  {
    name: "Pro",
    price: "$29",
    period: "/month",
    description: "For serious content creators.",
    features: [
      "4,000 credits/month (~400 clips)",
      "Everything in Free, plus:",
      "Smart Face & Motion detection (Beta)",
      "30 GB storage",
      "No watermark",
      "Priority processing",
    ],
    cta: "Get Started",
    featured: true,
    badge: "Most popular",
  },
  {
    name: "Studio",
    price: "$99",
    period: "/month",
    description: "For teams and agencies.",
    features: [
      "12,000 credits/month (~1,200 clips)",
      "Everything in Pro, plus:",
      "Premium Cinematic AI (Beta)",
      "150 GB storage",
      "API access (Available Soon)",
      "Channel monitoring — 2 included (Available Soon)",
    ],
    cta: "Get Started",
    featured: false,
  },
];

export function PricingSection() {
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
    <LandingSection
      id="pricing"
      className="dark:bg-gradient-to-b dark:from-transparent dark:via-brand-400/[0.03] dark:to-transparent"
    >
      <SectionHeader
        title={
          <>
            Start free. <span className="gradient-text">Grow at your own pace.</span>
          </>
        }
      />

      <div className="grid md:grid-cols-3 gap-6 max-w-5xl mx-auto">
        {PLANS.map((plan) => (
          <div
            key={plan.name}
            className={cn(
              "glass-card rounded-3xl p-8 flex flex-col relative transition-all duration-300 hover:-translate-y-2",
              plan.featured &&
                "border-brand-400/30 shadow-[0_25px_60px_rgba(0,0,0,0.4),0_0_40px_rgba(164,92,255,0.15)]"
            )}
          >
            {/* Badge */}
            {plan.badge && (
              <div className="absolute -top-3 left-1/2 -translate-x-1/2 btn-primary-gradient px-4 py-1 rounded-full text-xs font-semibold uppercase tracking-wide">
                {plan.badge}
              </div>
            )}

            {/* Header */}
            <div className="mb-6">
              <h3 className="text-xl font-semibold mb-2">{plan.name}</h3>
              <div className="flex items-baseline gap-1">
                <span className="text-4xl font-bold gradient-text">{plan.price}</span>
                <span className="text-muted-foreground">{plan.period}</span>
              </div>
            </div>

            <p className="text-muted-foreground mb-6">{plan.description}</p>

            {/* Features */}
            <ul className="space-y-3 mb-8 flex-1">
              {plan.features.map((feature) => (
                <li
                  key={feature}
                  className="flex items-center gap-3 text-sm text-muted-foreground"
                >
                  <Check className="w-5 h-5 text-brand-cyan flex-shrink-0" />
                  <span>{feature}</span>
                </li>
              ))}
            </ul>

            {/* CTA */}
            <Button
              onClick={scrollToProcessor}
              className={cn(
                "w-full h-12 rounded-xl font-semibold transition-all",
                plan.featured
                  ? "btn-primary-gradient"
                  : "glass-card border-border dark:border-white/10 hover:border-brand-400/30 hover:bg-brand-400/10"
              )}
              variant={plan.featured ? "default" : "outline"}
            >
              {plan.cta}
            </Button>
          </div>
        ))}
      </div>
    </LandingSection>
  );
}
