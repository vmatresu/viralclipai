"use client";

import { useState } from "react";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  PLANS,
  getPriceId,
  type PlanTier,
  type BillingInterval,
  type SubscriptionInfo,
} from "@/types/billing";

interface PlanSelectorProps {
  currentSubscription: SubscriptionInfo;
  onSelectPlan: (priceId: string, planTier: PlanTier) => void;
  isLoading?: boolean;
  loadingPlan?: PlanTier;
}

export function PlanSelector({
  currentSubscription,
  onSelectPlan,
  isLoading,
  loadingPlan,
}: PlanSelectorProps) {
  const [interval, setInterval] = useState<BillingInterval>(
    currentSubscription.interval ?? "monthly"
  );

  const currentPlan = currentSubscription.plan;

  return (
    <div className="space-y-6">
      {/* Billing interval toggle */}
      <div className="flex justify-center">
        <div className="inline-flex items-center rounded-lg bg-muted p-1">
          <button
            onClick={() => setInterval("monthly")}
            className={cn(
              "rounded-md px-4 py-2 text-sm font-medium transition-colors",
              interval === "monthly"
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Monthly
          </button>
          <button
            onClick={() => setInterval("annual")}
            className={cn(
              "rounded-md px-4 py-2 text-sm font-medium transition-colors",
              interval === "annual"
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Annual
            <span className="ml-1.5 rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-700 dark:bg-green-900 dark:text-green-300">
              2 months free
            </span>
          </button>
        </div>
      </div>

      {/* Plan cards */}
      <div className="grid gap-6 md:grid-cols-3">
        {PLANS.map((plan) => {
          const isCurrentPlan = plan.tier === currentPlan;
          const price =
            interval === "monthly" ? plan.monthlyPrice : plan.annualPrice;
          const priceId = getPriceId(plan.tier, interval);
          const isUpgrade =
            (plan.tier === "pro" && currentPlan === "free") ||
            (plan.tier === "studio" &&
              (currentPlan === "free" || currentPlan === "pro"));
          const isDowngrade =
            (plan.tier === "free" &&
              (currentPlan === "pro" || currentPlan === "studio")) ||
            (plan.tier === "pro" && currentPlan === "studio");

          return (
            <Card
              key={plan.tier}
              className={cn(
                "relative",
                isCurrentPlan && "border-primary ring-2 ring-primary",
                plan.tier === "pro" && "border-blue-500"
              )}
            >
              {plan.tier === "pro" && (
                <div className="absolute -top-3 left-1/2 -translate-x-1/2">
                  <span className="rounded-full bg-blue-500 px-3 py-1 text-xs font-medium text-white">
                    Most Popular
                  </span>
                </div>
              )}
              {isCurrentPlan && (
                <div className="absolute -top-3 right-4">
                  <span className="rounded-full bg-primary px-3 py-1 text-xs font-medium text-primary-foreground">
                    Current Plan
                  </span>
                </div>
              )}

              <CardHeader>
                <CardTitle>{plan.name}</CardTitle>
                <CardDescription>
                  <span className="text-3xl font-bold text-foreground">
                    ${price}
                  </span>
                  {plan.tier !== "free" && (
                    <span className="text-muted-foreground">
                      /{interval === "monthly" ? "mo" : "yr"}
                    </span>
                  )}
                </CardDescription>
              </CardHeader>

              <CardContent className="space-y-4">
                <p className="text-sm text-muted-foreground">
                  {plan.credits} credits/month
                </p>
                <ul className="space-y-2">
                  {plan.features.map((feature) => (
                    <li key={feature} className="flex items-center text-sm">
                      <Check className="mr-2 h-4 w-4 text-green-500" />
                      {feature}
                    </li>
                  ))}
                </ul>
              </CardContent>

              <CardFooter>
                {isCurrentPlan ? (
                  <Button disabled className="w-full">
                    Current Plan
                  </Button>
                ) : plan.tier === "free" ? (
                  isDowngrade ? (
                    <Button
                      variant="outline"
                      className="w-full"
                      onClick={() => onSelectPlan("", plan.tier)}
                      disabled={isLoading}
                    >
                      {isLoading && loadingPlan === plan.tier
                        ? "Processing..."
                        : "Downgrade to Free"}
                    </Button>
                  ) : (
                    <Button disabled variant="outline" className="w-full">
                      Free Forever
                    </Button>
                  )
                ) : (
                  <Button
                    className={cn(
                      "w-full",
                      plan.tier === "pro" && "bg-blue-500 hover:bg-blue-600"
                    )}
                    onClick={() => priceId && onSelectPlan(priceId, plan.tier)}
                    disabled={isLoading || !priceId}
                  >
                    {isLoading && loadingPlan === plan.tier
                      ? "Processing..."
                      : isUpgrade
                        ? `Upgrade to ${plan.name}`
                        : isDowngrade
                          ? `Downgrade to ${plan.name}`
                          : `Select ${plan.name}`}
                  </Button>
                )}
              </CardFooter>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
