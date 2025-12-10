import { Check } from "lucide-react";
import Link from "next/link";

import { cn } from "@/lib/utils";

const plans = [
  {
    name: "Free",
    price: "$0",
    cadence: "",
    description: "Up to 20 clips / month",
    features: [
      "100 MB storage",
      "All AI highlight detection features",
      "Basic email support",
      "Great for testing and validation",
    ],
    cta: "Start free",
    href: "/",
    badge: "Best for testing",
    tone: "neutral" as const,
  },
  {
    name: "Pro",
    price: "$29",
    cadence: "per month, per creator",
    description: "Up to 500 clips / month",
    features: [
      "1 GB storage",
      "Priority processing in the queue",
      "TikTok publish integration",
      "Workspace history & reprocessing",
      "Email support",
    ],
    cta: "Choose Pro",
    href: "/",
    badge: "Recommended",
    tone: "primary" as const,
  },
  {
    name: "Studio",
    price: "Contact",
    cadence: "for custom pricing",
    description: "Higher clip limits & dedicated capacity",
    features: [
      "5 GB storage",
      "Team accounts & shared settings",
      "Custom integrations & SLAs",
      "Dedicated support & onboarding",
    ],
    cta: "Talk to us",
    href: "/contact",
    badge: "Custom",
    tone: "custom" as const,
  },
];

export default function PricingPage() {
  return (
    <div className="space-y-10">
      <section className="space-y-3 text-center md:text-left">
        <h1 className="text-4xl font-extrabold text-foreground">Pricing</h1>
        <p className="text-muted-foreground max-w-2xl">
          Start free, then upgrade when you are ready to produce clips at scale. Plans
          are per-user and can be changed at any time.
        </p>
      </section>

      <section className="grid gap-6 md:grid-cols-3">
        {plans.map((plan) => (
          <div
            key={plan.name}
            className={cn(
              "relative rounded-2xl border bg-white p-6 shadow-lg shadow-brand-500/10 ring-1 ring-brand-50/60 transition-transform duration-300 hover:-translate-y-1 hover:shadow-xl dark:border-white/10 dark:bg-slate-900/80 dark:ring-0 h-full flex flex-col gap-6",
              plan.tone === "primary" && "border-brand-300 ring-brand-200/80",
              plan.tone === "custom" &&
                "border-brand-200 bg-gradient-to-br from-brand-50 via-white to-brand-100 text-foreground"
            )}
          >
            {plan.badge && (
              <span
                className={cn(
                  "absolute -top-3 right-4 inline-flex items-center rounded-full px-3 py-1 text-[11px] font-semibold uppercase tracking-wide",
                  plan.tone === "primary"
                    ? "bg-brand-100 text-brand-800 border border-brand-200"
                    : "bg-slate-100 text-slate-700 border border-slate-200",
                  plan.tone === "custom" &&
                    "bg-brand-100 text-brand-800 border border-brand-200 dark:bg-white/10 dark:text-white dark:border-white/15"
                )}
              >
                {plan.badge}
              </span>
            )}

            <div className="space-y-2">
              <h2
                className={cn(
                  "text-xl font-bold",
                  plan.tone === "custom"
                    ? "text-foreground dark:text-white"
                    : "text-foreground"
                )}
              >
                {plan.name}
              </h2>
              <p
                className={cn(
                  "text-3xl font-extrabold",
                  plan.tone === "custom"
                    ? "text-foreground dark:text-white"
                    : "text-foreground"
                )}
              >
                {plan.price}
              </p>
              {plan.cadence && (
                <p
                  className={cn(
                    "text-xs text-muted-foreground",
                    plan.tone === "custom" && "dark:text-white/70"
                  )}
                >
                  {plan.cadence}
                </p>
              )}
              <p
                className={cn(
                  "text-sm text-muted-foreground",
                  plan.tone === "custom" && "dark:text-white/80"
                )}
              >
                {plan.description}
              </p>
            </div>

            <ul className="mt-6 space-y-3">
              {plan.features.map((feature) => (
                <li key={feature} className="flex items-start gap-2 text-sm">
                  <Check
                    className={cn(
                      "mt-0.5 h-4 w-4 text-brand-500",
                      plan.tone === "custom" && "dark:text-brand-200"
                    )}
                  />
                  <span
                    className={cn(
                      "text-muted-foreground",
                      plan.tone === "custom" && "dark:text-white/80"
                    )}
                  >
                    {feature}
                  </span>
                </li>
              ))}
            </ul>

            <div className="mt-auto pt-2">
              <Link
                href={plan.href}
                className={cn(
                  "inline-flex w-full items-center justify-center rounded-xl px-4 h-12 text-sm font-semibold transition-all",
                  plan.tone === "primary"
                    ? "bg-brand-500 text-white shadow-md shadow-brand-500/30 hover:bg-brand-600"
                    : "bg-white text-foreground border border-brand-200 hover:border-brand-300 hover:bg-brand-50",
                  plan.tone === "custom" &&
                    "bg-white text-foreground border border-brand-200 hover:border-brand-300 hover:bg-brand-50 dark:bg-transparent dark:text-white dark:border-white/40 dark:hover:bg-white/10 dark:hover:border-white/50"
                )}
              >
                {plan.cta}
              </Link>
            </div>
          </div>
        ))}
      </section>
    </div>
  );
}
