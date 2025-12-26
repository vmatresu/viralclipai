"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Check } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useAuth } from "@/lib/auth";

import { LandingSection, SectionHeader } from "./LandingSection";

interface PlanConfig {
  name: string;
  monthlyPrice: number;
  annualPrice: number;
  description: string;
  features: string[];
  featured: boolean;
  badge?: string;
}

const PLANS: PlanConfig[] = [
  {
    name: "Free",
    monthlyPrice: 0,
    annualPrice: 0,
    description: "Perfect to test the engine.",
    features: [
      "200 credits/month (~20 clips)",
      "Static & Basic AI detection",
      "Streamer styles",
      "1 GB storage",
      "Watermarked exports",
    ],
    featured: false,
  },
  {
    name: "Pro",
    monthlyPrice: 29,
    annualPrice: 290,
    description: "For serious content creators.",
    features: [
      "4,000 credits/month (~400 clips)",
      "Everything in Free, plus:",
      "Smart Face & Motion detection (Beta)",
      "30 GB storage",
      "No watermark",
      "Priority processing",
    ],
    featured: true,
    badge: "Most popular",
  },
  {
    name: "Studio",
    monthlyPrice: 99,
    annualPrice: 990,
    description: "For teams and agencies.",
    features: [
      "12,000 credits/month (~1,200 clips)",
      "Everything in Pro, plus:",
      "Premium Cinematic AI (Beta)",
      "150 GB storage",
      "API access (Available Soon)",
      "Channel monitoring — 2 included (Available Soon)",
    ],
    featured: false,
  },
];

export function PricingSection() {
  const [isAnnual, setIsAnnual] = useState(false);
  const router = useRouter();
  const { user } = useAuth();

  const handleGetStarted = (planName: string) => {
    if (planName === "Free") {
      // Scroll to processor for free plan
      const target = document.querySelector("#process-video");
      if (target) {
        const navHeight = 80;
        const targetPosition =
          target.getBoundingClientRect().top + window.scrollY - navHeight - 20;
        window.scrollTo({ top: targetPosition, behavior: "smooth" });
      }
    } else if (user) {
      // If user is logged in, go to billing page
      router.push("/billing");
    } else {
      // If not logged in, go to sign up/login
      router.push("/auth/login?redirect=/billing");
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

      {/* Billing interval toggle */}
      <div className="flex justify-center mb-10">
        <div className="inline-flex items-center rounded-full bg-muted/50 p-1.5 backdrop-blur-sm border border-white/5">
          <button
            onClick={() => setIsAnnual(false)}
            className={cn(
              "rounded-full px-5 py-2 text-sm font-medium transition-all duration-200",
              !isAnnual
                ? "bg-background text-foreground shadow-md"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Monthly
          </button>
          <button
            onClick={() => setIsAnnual(true)}
            className={cn(
              "rounded-full px-5 py-2 text-sm font-medium transition-all duration-200 flex items-center gap-2",
              isAnnual
                ? "bg-background text-foreground shadow-md"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Annual
            <span className="rounded-full bg-green-500/20 px-2 py-0.5 text-xs font-semibold text-green-400">
              2 months free
            </span>
          </button>
        </div>
      </div>

      <div className="grid md:grid-cols-3 gap-6 max-w-5xl mx-auto">
        {PLANS.map((plan) => {
          const price = isAnnual ? plan.annualPrice : plan.monthlyPrice;
          const period = plan.monthlyPrice === 0 ? "" : isAnnual ? "/year" : "/month";
          const savings = isAnnual && plan.monthlyPrice > 0
            ? plan.monthlyPrice * 12 - plan.annualPrice
            : 0;

          return (
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
                  <span className="text-4xl font-bold gradient-text">
                    ${price}
                  </span>
                  <span className="text-muted-foreground">{period}</span>
                </div>
                {savings > 0 && (
                  <p className="text-sm text-green-400 mt-1">
                    Save ${savings}/year
                  </p>
                )}
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
                onClick={() => handleGetStarted(plan.name)}
                className={cn(
                  "w-full h-12 rounded-xl font-semibold transition-all",
                  plan.featured
                    ? "btn-primary-gradient"
                    : "glass-card border-border dark:border-white/10 hover:border-brand-400/30 hover:bg-brand-400/10"
                )}
                variant={plan.featured ? "default" : "outline"}
              >
                {plan.name === "Free" ? "Get Started" : "Upgrade Now"}
              </Button>
            </div>
          );
        })}
      </div>
    </LandingSection>
  );
}
